// Menu bar app: setup on first run (move to Applications, Accessibility permission, start at login), the menu,
// update notifications, and uninstall.

import AppKit
import ApplicationServices
import KakaoAdHideCore
import ServiceManagement
import UserNotifications

final class AppDelegate: NSObject, NSApplicationDelegate, NSMenuDelegate, UNUserNotificationCenterDelegate {
  private let cover = CoverController()
  private let updater = Updater()
  private var statusItem: NSStatusItem!
  private var permissionTimer: Timer?

  func applicationDidFinishLaunching(_ notification: Notification) {
    if otherInstanceRunning() { NSApp.terminate(nil); return }
    if offerMoveToApplications() { return }

    statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength)
    let menu = NSMenu()
    menu.delegate = self
    statusItem.menu = menu
    updateIcon()

    UNUserNotificationCenter.current().delegate = self
    cover.onStateChange = { [weak self] _ in self?.updateIcon() }
    updater.onChange = { [weak self] in self?.updateIcon() }
    updater.notify = { [weak self] f in self?.postNotification(id: "update", title: "KakaoAdHide \(f.version)",
                                                               body: "새 버전이 있습니다. 클릭하면 업데이트합니다.") }

    firstRunSetup()
    noteVersion()
    cover.start()
    updater.startSchedule()
    if !AXIsProcessTrusted() { askForPermission() }
  }

  // MARK: setup

  private func otherInstanceRunning() -> Bool {
    NSRunningApplication.runningApplications(withBundleIdentifier: Config.bundleID)
      .contains { $0.processIdentifier != ProcessInfo.processInfo.processIdentifier }
  }

  /// Run from the disk image or Downloads: offer to copy into Applications (needed for updates and for a stable
  /// login item). Returns true when the app is relaunching from there.
  private func offerMoveToApplications() -> Bool {
    let here = Bundle.main.bundleURL
    guard !Installer.isInApplications(here), here.pathExtension == "app" else { return false }
    NSApp.activate(ignoringOtherApps: true)
    let a = NSAlert()
    a.messageText = "응용 프로그램 폴더로 옮길까요?"
    a.informativeText = "KakaoAdHide는 응용 프로그램 폴더에 있어야 로그인할 때 실행되고 업데이트도 됩니다."
    a.addButton(withTitle: "옮기기")
    a.addButton(withTitle: "그대로 실행")
    guard a.runModal() == .alertFirstButtonReturn else { return false }

    let fm = FileManager.default
    let system = URL(fileURLWithPath: "/Applications")
    let folder = fm.isWritableFile(atPath: system.path) ? system
      : fm.homeDirectoryForCurrentUser.appendingPathComponent("Applications")
    let dest = folder.appendingPathComponent(Config.appName)
    do {
      try fm.createDirectory(at: folder, withIntermediateDirectories: true)
      if fm.fileExists(atPath: dest.path) { try fm.trashItem(at: dest, resultingItemURL: nil) }
      try Installer.run("/usr/bin/ditto", [here.path, dest.path])
      let cfg = NSWorkspace.OpenConfiguration()
      cfg.createsNewApplicationInstance = true
      NSWorkspace.shared.openApplication(at: dest, configuration: cfg) { _, _ in
        DispatchQueue.main.async { NSApp.terminate(nil) }
      }
      return true
    } catch {
      alert("옮기지 못했습니다", "\(folder.path)에 직접 끌어다 놓아 주세요.\n\n\(error.localizedDescription)")
      return false
    }
  }

  private func firstRunSetup() {
    guard !Settings.setupDone else { return }
    Settings.setupDone = true
    if Installer.isInApplications(Bundle.main.bundleURL) { try? SMAppService.mainApp.register() }   // like the Windows startup entry
  }

  private func noteVersion() {
    let last = Settings.lastVersion
    if let last, last != Config.version {
      postNotification(id: "updated", title: "KakaoAdHide \(Config.version)", body: "\(last)에서 \(Config.version)(으)로 업데이트했습니다.")
    }
    Settings.lastVersion = Config.version
  }

  private func askForPermission() {
    NSApp.activate(ignoringOtherApps: true)
    let a = NSAlert()
    a.messageText = "손쉬운 사용 권한이 필요합니다"
    a.informativeText = """
      카카오톡 창의 위치와 광고 영역을 읽으려면 권한이 필요합니다. 창 위치를 읽는 데만 쓰고, 카카오톡의 내용은 읽거나 저장하지 않습니다.

      시스템 설정 > 개인정보 보호 및 보안 > 손쉬운 사용에서 KakaoAdHide를 켜 주세요. 켜면 바로 동작합니다.
      """
    a.addButton(withTitle: "설정 열기")
    a.addButton(withTitle: "나중에")
    if a.runModal() == .alertFirstButtonReturn { openAccessibilitySettings() }
    waitForPermission()
  }

  private func openAccessibilitySettings() {
    _ = AXIsProcessTrustedWithOptions([kAXTrustedCheckOptionPrompt.takeUnretainedValue() as String: true] as CFDictionary)
    if let url = URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility") {
      NSWorkspace.shared.open(url)
    }
  }

  private func waitForPermission() {
    permissionTimer?.invalidate()
    permissionTimer = Timer.scheduledTimer(withTimeInterval: 1, repeats: true) { [weak self] t in
      if AXIsProcessTrusted() { t.invalidate(); self?.cover.settingsChanged(); self?.updateIcon() }
    }
  }

  // MARK: menu

  private func updateIcon() {
    guard let button = statusItem?.button else { return }
    let name: String
    if updater.pending != nil { name = "arrow.down.circle" }
    else if cover.state == .noPermission { name = "exclamationmark.triangle" }
    else if cover.state == .disabled { name = "eye" }
    else { name = "eye.slash" }
    button.image = NSImage(systemSymbolName: name, accessibilityDescription: "KakaoAdHide")
    button.image?.isTemplate = true
    button.toolTip = "KakaoAdHide \(Config.version)"
  }

  private var stateText: String {
    switch cover.state {
    case .disabled: return "광고 가리기 꺼짐"
    case .noPermission: return "손쉬운 사용 권한이 필요합니다"
    case .waitingForKakao: return "카카오톡을 기다리는 중"
    case .covering: return "광고를 가리는 중"
    case .notFound: return "카카오톡 메인 창이 보이지 않음"
    }
  }

  func menuNeedsUpdate(_ menu: NSMenu) {
    menu.removeAllItems()
    menu.addItem(disabled("KakaoAdHide \(Config.version)"))
    menu.addItem(disabled(stateText))
    menu.addItem(.separator())

    menu.addItem(item("광고 가리기", #selector(toggleCover), on: Settings.coverEnabled))
    let colors = NSMenu()
    for (c, title) in [(Settings.CoverColor.auto, "시스템 설정 따라가기"), (.light, "라이트"), (.dark, "다크")] {
      let i = item(title, #selector(setColor(_:)), on: Settings.coverColor == c)
      i.representedObject = c.rawValue
      colors.addItem(i)
    }
    let colorItem = NSMenuItem(title: "가림막 색", action: nil, keyEquivalent: "")
    colorItem.submenu = colors
    menu.addItem(colorItem)
    if cover.state == .noPermission { menu.addItem(item("손쉬운 사용 설정 열기…", #selector(openPermission))) }
    menu.addItem(.separator())

    if let f = updater.pending {
      menu.addItem(item(updater.busy ? "\(f.version) 받는 중…" : "\(f.version)(으)로 업데이트", updater.busy ? nil : #selector(installUpdate)))
      menu.addItem(item("변경 내역 보기", #selector(openChanges)))
      menu.addItem(item("이 버전 건너뛰기", #selector(skipUpdate)))
    } else {
      menu.addItem(item("업데이트 확인", #selector(checkNow)))
    }
    menu.addItem(item("업데이트 자동 확인", #selector(toggleUpdateCheck), on: Settings.updateCheck))
    menu.addItem(item("로그인할 때 실행", #selector(toggleLoginItem), on: SMAppService.mainApp.status == .enabled))
    menu.addItem(.separator())
    menu.addItem(item("KakaoAdHide 제거…", #selector(uninstall)))
    menu.addItem(item("종료", #selector(quit)))
  }

  private func disabled(_ title: String) -> NSMenuItem {
    let i = NSMenuItem(title: title, action: nil, keyEquivalent: "")
    i.isEnabled = false
    return i
  }
  private func item(_ title: String, _ action: Selector?, on: Bool? = nil) -> NSMenuItem {
    let i = NSMenuItem(title: title, action: action, keyEquivalent: "")
    i.target = self
    if let on { i.state = on ? .on : .off }
    return i
  }

  @objc private func toggleCover() {
    Settings.coverEnabled.toggle()
    cover.settingsChanged()
    updateIcon()
  }
  @objc private func setColor(_ sender: NSMenuItem) {
    Settings.coverColor = Settings.CoverColor(rawValue: sender.representedObject as? String ?? "") ?? .auto
    cover.settingsChanged()
  }
  @objc private func openPermission() { openAccessibilitySettings(); waitForPermission() }
  @objc private func toggleUpdateCheck() {
    Settings.updateCheck.toggle()
    updater.startSchedule()
  }
  @objc private func toggleLoginItem() {
    do {
      if SMAppService.mainApp.status == .enabled { try SMAppService.mainApp.unregister() }
      else { try SMAppService.mainApp.register() }
    } catch {
      alert("로그인 항목을 바꾸지 못했습니다", error.localizedDescription)
    }
  }

  @objc private func checkNow() {
    Task { @MainActor in
      switch await updater.check() {
      case .upToDate: alert("최신 버전입니다", "KakaoAdHide \(Config.version)")
      case .available(let f): offerUpdate(f)
      case .failed(let e): alert("업데이트를 확인하지 못했습니다", e.localizedDescription)
      }
    }
  }

  private func offerUpdate(_ f: UpdateFeed) {
    NSApp.activate(ignoringOtherApps: true)
    let a = NSAlert()
    a.messageText = "KakaoAdHide \(f.version)이 나왔습니다"
    a.informativeText = "지금 업데이트할까요? 받은 파일의 서명을 확인한 뒤에만 설치합니다."
    a.addButton(withTitle: "업데이트")
    a.addButton(withTitle: "변경 내역 보기")
    a.addButton(withTitle: "나중에")
    switch a.runModal() {
    case .alertFirstButtonReturn: startInstall(f)
    case .alertSecondButtonReturn: NSWorkspace.shared.open(f.pageURL)
    default: break
    }
  }

  @objc private func installUpdate() { if let f = updater.pending { startInstall(f) } }
  @objc private func openChanges() { NSWorkspace.shared.open(updater.pending?.pageURL ?? UpdateConfig.releasesPage) }
  @objc private func skipUpdate() { if let f = updater.pending { updater.skip(f) } }

  private func startInstall(_ f: UpdateFeed) {
    Task { @MainActor in
      do {
        try await updater.install(f)   // quits on success
      } catch let e as UpdateError {
        failed(f, "새 버전(\(f.version))을 검증하지 못해 설치하지 않았습니다.\n\(e)\n\n릴리스 페이지에서 직접 받아 주세요.")
      } catch let e as InstallError {
        failed(f, "\(e)\n\n릴리스 페이지에서 직접 받아 주세요.")
      } catch {
        alert("업데이트를 받지 못했습니다", "인터넷 연결을 확인하고 다시 시도해 주세요.\n\n\(error.localizedDescription)")
      }
    }
  }

  private func failed(_ f: UpdateFeed, _ text: String) {
    NSApp.activate(ignoringOtherApps: true)
    let a = NSAlert()
    a.messageText = "업데이트를 설치하지 않았습니다"
    a.informativeText = text
    a.addButton(withTitle: "릴리스 페이지 열기")
    a.addButton(withTitle: "닫기")
    if a.runModal() == .alertFirstButtonReturn { NSWorkspace.shared.open(f.pageURL) }
  }

  @objc private func uninstall() {
    NSApp.activate(ignoringOtherApps: true)
    let a = NSAlert()
    a.messageText = "KakaoAdHide를 제거할까요?"
    a.informativeText = "로그인 항목과 설정을 지우고 앱을 휴지통으로 옮깁니다. 카카오톡은 그대로입니다."
    a.addButton(withTitle: "제거")
    a.addButton(withTitle: "취소")
    guard a.runModal() == .alertFirstButtonReturn else { return }
    try? SMAppService.mainApp.unregister()
    Settings.removeAll()
    _ = try? Installer.run("/usr/bin/tccutil", ["reset", "Accessibility", Config.bundleID])   // may need admin; harmless if it fails
    do {
      try FileManager.default.trashItem(at: Bundle.main.bundleURL, resultingItemURL: nil)
      alert("제거했습니다", "손쉬운 사용 목록에 KakaoAdHide가 남아 있으면 '-' 버튼으로 지워 주세요.")
    } catch {
      alert("앱을 휴지통으로 옮기지 못했습니다", "\(Bundle.main.bundleURL.path)을 직접 휴지통으로 옮겨 주세요.")
    }
    NSApp.terminate(nil)
  }

  @objc private func quit() { NSApp.terminate(nil) }

  private func alert(_ title: String, _ text: String) {
    NSApp.activate(ignoringOtherApps: true)
    let a = NSAlert()
    a.messageText = title
    a.informativeText = text
    a.runModal()
  }

  // MARK: notifications

  private func postNotification(id: String, title: String, body: String) {
    let center = UNUserNotificationCenter.current()
    center.requestAuthorization(options: [.alert]) { granted, _ in
      guard granted else { return }
      let c = UNMutableNotificationContent()
      c.title = title
      c.body = body
      center.add(UNNotificationRequest(identifier: id, content: c, trigger: nil))
    }
  }

  func userNotificationCenter(_ center: UNUserNotificationCenter, didReceive response: UNNotificationResponse,
                              withCompletionHandler done: @escaping () -> Void) {
    if response.notification.request.identifier == "update" {
      DispatchQueue.main.async { if let f = self.updater.pending { self.offerUpdate(f) } }
    }
    done()
  }

  func userNotificationCenter(_ center: UNUserNotificationCenter, willPresent notification: UNNotification,
                              withCompletionHandler done: @escaping (UNNotificationPresentationOptions) -> Void) {
    done([.banner])
  }
}
