// Update check and one-click update, the macOS counterpart of windows/src/Updater.cs.
//
// A minute after start and then once a day it reads latest-macos.json from the latest GitHub release. When there
// is a newer version the menu bar icon changes and a notification is shown once; nothing is downloaded until the
// user clicks it. Then KakaoAdHide-macos.dmg is downloaded and checked before anything is replaced:
//   1. the feed (same format, signature and keys as the Windows version; KakaoAdHideCore.FeedVerifier):
//      download link inside this repository's releases, size, SHA-256, RSA signature over version + SHA-256,
//   2. the app inside the disk image: bundle identifier, version equal to the feed's, and a valid code signature
//      that meets Config.signingRequirement (our certificate).
// The verified copy replaces this one after it quits (a detached script; the old copy is restored if the swap
// fails) and is started. If anything does not verify, nothing is replaced and the release page is offered.

import AppKit
import KakaoAdHideCore
import Security

enum UpdateCheckResult { case upToDate, available(UpdateFeed), failed(Error) }

final class Updater {
  private(set) var pending: UpdateFeed?
  private(set) var busy = false
  var onChange: (() -> Void)?
  var notify: ((UpdateFeed) -> Void)?

  private var timer: Timer?
  private lazy var session: URLSession = {
    let c = URLSessionConfiguration.ephemeral
    c.httpAdditionalHeaders = ["User-Agent": "KakaoAdHide/\(Config.version)"]   // nothing else is sent
    c.timeoutIntervalForRequest = 30
    c.httpCookieStorage = nil
    c.urlCache = nil
    return URLSession(configuration: c)
  }()

  func startSchedule() {
    timer?.invalidate()
    guard Settings.updateCheck else { return }
    timer = Timer.scheduledTimer(withTimeInterval: 60, repeats: false) { [weak self] _ in
      self?.backgroundCheck()
      self?.timer = Timer.scheduledTimer(withTimeInterval: 24 * 60 * 60, repeats: true) { [weak self] _ in self?.backgroundCheck() }
    }
  }

  private func backgroundCheck() {
    Task { @MainActor in
      if case .available(let f) = await check(), f.version != Settings.skippedVersion {
        if Settings.notifiedVersion != f.version {   // the notification once per version; the menu item stays
          Settings.notifiedVersion = f.version
          notify?(f)
        }
      }
    }
  }

  @MainActor
  func check() async -> UpdateCheckResult {
    do {
      let (data, resp) = try await session.data(from: UpdateConfig.feedURL)
      guard (resp as? HTTPURLResponse)?.statusCode == 200, data.count <= UpdateConfig.maxFeedBytes else {
        throw URLError(.badServerResponse)
      }
      let feed = try UpdateFeed.parse(data)
      guard let new = AppVersion(feed.version), let cur = AppVersion(Config.version), new > cur else {
        pending = nil; onChange?()
        return .upToDate
      }
      pending = feed.version == Settings.skippedVersion ? nil : feed
      onChange?()
      return .available(feed)
    } catch {
      return .failed(error)
    }
  }

  func skip(_ feed: UpdateFeed) {
    Settings.skippedVersion = feed.version
    pending = nil
    onChange?()
  }

  /// Downloads, verifies, and hands over to the replacement script; the app then quits.
  @MainActor
  func install(_ feed: UpdateFeed) async throws {
    guard !busy else { return }
    busy = true; onChange?()
    defer { busy = false; onChange?() }

    let destination = Bundle.main.bundleURL
    guard Installer.canReplace(destination) else { throw InstallError.notInApplications }

    let url = try FeedVerifier.checkURL(feed)
    let work = try Installer.makeWorkDir()
    do {
      let dmg = work.appendingPathComponent("KakaoAdHide-macos.dmg")
      let (tmp, resp) = try await session.download(from: url)
      guard (resp as? HTTPURLResponse)?.statusCode == 200 else { throw URLError(.badServerResponse) }
      let size = (try? FileManager.default.attributesOfItem(atPath: tmp.path)[.size] as? Int64) ?? 0
      guard size <= UpdateConfig.maxPackageBytes else { throw UpdateError.tooLarge }
      try FileManager.default.moveItem(at: tmp, to: dmg)

      let app = try await Task.detached { () throws -> URL in
        try FeedVerifier.verify(feed, file: dmg)
        return try Installer.extractVerifiedApp(from: dmg, into: work, version: feed.version)
      }.value
      try Installer.startReplacement(source: app, destination: destination, workDir: work)
    } catch {
      try? FileManager.default.removeItem(at: work)
      throw error
    }
    NSApp.terminate(nil)
  }
}

enum InstallError: Error, CustomStringConvertible {
  case notInApplications
  case tool(String)
  var description: String {
    switch self {
    case .notInApplications:
      return "KakaoAdHide가 응용 프로그램 폴더 밖에 있거나 바꿀 권한이 없어서 자동으로 업데이트할 수 없습니다."
    case .tool(let what): return "\(what) 실행에 실패했습니다."
    }
  }
}

enum Installer {
  static func makeWorkDir() throws -> URL {
    let dir = FileManager.default.temporaryDirectory.appendingPathComponent("KakaoAdHide-update-\(UUID().uuidString)")
    try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
    return dir
  }

  static func isInApplications(_ app: URL) -> Bool {
    let parent = app.standardizedFileURL.deletingLastPathComponent().path
    let home = FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent("Applications").standardizedFileURL.path
    return parent == "/Applications" || parent == home
  }

  /// The swap renames the bundle inside its folder, so this user must be able to write both.
  static func canReplace(_ app: URL) -> Bool {
    isInApplications(app) && FileManager.default.isWritableFile(atPath: app.deletingLastPathComponent().path)
      && FileManager.default.isWritableFile(atPath: app.path)
  }

  /// Mounts the disk image read-only and invisibly, copies KakaoAdHide.app out and checks the copy.
  /// The image is always detached before this returns.
  static func extractVerifiedApp(from dmg: URL, into work: URL, version: String) throws -> URL {
    let mount = work.appendingPathComponent("mount", isDirectory: true)
    try FileManager.default.createDirectory(at: mount, withIntermediateDirectories: true)
    try run("/usr/bin/hdiutil", ["attach", "-nobrowse", "-readonly", "-noautoopen", "-noverify", "-mountpoint", mount.path, dmg.path])
    defer {
      if (try? run("/usr/bin/hdiutil", ["detach", mount.path])) == nil {
        _ = try? run("/usr/bin/hdiutil", ["detach", "-force", mount.path])
      }
    }
    // Exactly <volume>/KakaoAdHide.app, a real directory (the image's Applications link is never followed).
    let source = mount.appendingPathComponent(Config.appName)
    let values = try? source.resourceValues(forKeys: [.isSymbolicLinkKey, .isDirectoryKey])
    guard values?.isDirectory == true, values?.isSymbolicLink == false else { throw UpdateError.badPackage("앱이 없습니다") }
    let copy = work.appendingPathComponent(Config.appName)
    try run("/usr/bin/ditto", ["--noqtn", source.path, copy.path])
    try verifyApp(copy, version: version)
    return copy
  }

  static func verifyApp(_ app: URL, version: String) throws {
    let info = NSDictionary(contentsOf: app.appendingPathComponent("Contents/Info.plist")) as? [String: Any]
    guard info?["CFBundleIdentifier"] as? String == Config.bundleID else { throw UpdateError.badPackage("번들 ID가 다릅니다") }
    guard info?["CFBundleShortVersionString"] as? String == version else { throw UpdateError.badPackage("버전이 다릅니다") }
    try verifySignature(app)
  }

  /// The whole bundle must be validly signed, by our certificate.
  static func verifySignature(_ app: URL, requirement: String = Config.signingRequirement) throws {
    var code: SecStaticCode?
    var req: SecRequirement?
    let flags = SecCSFlags(rawValue: kSecCSStrictValidate | kSecCSCheckAllArchitectures | kSecCSCheckNestedCode)
    guard SecStaticCodeCreateWithPath(app as CFURL, [], &code) == errSecSuccess, let code,
          SecRequirementCreateWithString(requirement as CFString, [], &req) == errSecSuccess, let req,
          SecStaticCodeCheckValidity(code, flags, req) == errSecSuccess else {
      throw UpdateError.badPackage("코드 서명이 맞지 않습니다")
    }
  }

  /// Starts a detached script that swaps the app once this process has exited, then opens whichever copy is in
  /// place (the new one, or the old one if the swap failed).
  static func startReplacement(source: URL, destination: URL, workDir: URL) throws {
    let script = workDir.appendingPathComponent("replace.sh")
    try """
    #!/bin/bash
    pid="$1"; src="$2"; dst="$3"; work="$4"
    for _ in $(seq 1 300); do kill -0 "$pid" 2>/dev/null || break; sleep 0.2; done
    sleep 0.3
    dir="$(dirname "$dst")"; name="$(basename "$dst")"
    staged="$dir/.$name.update-$$"; previous="$dir/.$name.previous-$$"
    if /usr/bin/ditto "$src" "$staged" && /bin/mv "$dst" "$previous"; then
      if /bin/mv "$staged" "$dst"; then /bin/rm -rf "$previous"; else /bin/mv "$previous" "$dst"; fi
    fi
    /bin/rm -rf "$staged"
    /usr/bin/open "$dst"
    /bin/rm -rf "$work"
    """.write(to: script, atomically: true, encoding: .utf8)
    let p = Process()
    p.executableURL = URL(fileURLWithPath: "/bin/bash")
    p.arguments = [script.path, String(ProcessInfo.processInfo.processIdentifier), source.path, destination.path, workDir.path]
    p.standardInput = FileHandle.nullDevice
    p.standardOutput = FileHandle.nullDevice
    p.standardError = FileHandle.nullDevice
    try p.run()
  }

  @discardableResult
  static func run(_ tool: String, _ args: [String]) throws -> Int32 {
    let p = Process()
    p.executableURL = URL(fileURLWithPath: tool)
    p.arguments = args
    p.standardInput = FileHandle.nullDevice
    p.standardOutput = FileHandle.nullDevice
    p.standardError = FileHandle.nullDevice
    try p.run()
    p.waitUntilExit()
    guard p.terminationStatus == 0 else { throw InstallError.tool((tool as NSString).lastPathComponent) }
    return p.terminationStatus
  }
}

/// KakaoAdHide --verify-feed latest-macos.json KakaoAdHide-macos.dmg : for the release workflow. The app being
/// released checks its own feed and disk image with the same code users run, so this also proves the feed's
/// version is the version built into the app and that the image's app carries the expected signature.
/// Exit codes: 0 ok, 3 not verified, 4 version mismatch, 1 other error.
enum VerifyCommand {
  static func run(_ args: [String]) -> Int32 {
    guard args.count == 4 else { return 1 }
    do {
      let feed = try UpdateFeed.parse(Data(contentsOf: URL(fileURLWithPath: args[2])))
      guard feed.version == Config.version else { return 4 }
      _ = try FeedVerifier.checkURL(feed)
      let dmg = URL(fileURLWithPath: args[3])
      try FeedVerifier.verify(feed, file: dmg)
      let work = try Installer.makeWorkDir()
      defer { try? FileManager.default.removeItem(at: work) }
      _ = try Installer.extractVerifiedApp(from: dmg, into: work, version: feed.version)
      return 0
    } catch let e as UpdateError {
      FileHandle.standardError.write(Data("not verified: \(e)\n".utf8))
      return 3
    } catch {
      FileHandle.standardError.write(Data("error: \(error)\n".utf8))
      return 1
    }
  }
}
