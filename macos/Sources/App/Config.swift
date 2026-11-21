import AppKit

enum Config {
  static let bundleID = "io.github.pandemoniummm.KakaoAdHide"
  static let kakaoBundleID = "com.kakao.KakaoTalkMac"
  static let appName = "KakaoAdHide.app"

  /// SHA-1 of the self-signed "KakaoAdHide Code Signing" certificate (tools/new-macos-signing-cert.sh).
  static let signingCertificateSHA1 = "DB044D3EF322C3D1E1961BB3D91063A3E07CF06A"

  /// What a downloaded update must be signed with, checked on top of the feed signature. It is also what the
  /// release builds' designated requirement says, so the Accessibility permission carries over to the update.
  /// Moving to a Developer ID later: first ship a version that also accepts it, e.g.
  ///   ... or (anchor apple generic and certificate leaf[subject.OU] = "<TEAM ID>")
  /// and only then sign with it; copies that never got that version have to be updated by hand.
  static let signingRequirement =
    #"identifier "\#(bundleID)" and certificate leaf = H"\#(signingCertificateSHA1)""#

  static var version: String {
    Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "0.0.0"
  }
}

/// UserDefaults (domain io.github.pandemoniummm.KakaoAdHide). Turn the update check off from Terminal:
///   defaults write io.github.pandemoniummm.KakaoAdHide UpdateCheck -bool false
enum Settings {
  private static let d = UserDefaults.standard

  enum CoverColor: String, CaseIterable { case auto, light, dark }

  static var coverEnabled: Bool {
    get { d.object(forKey: "CoverEnabled") as? Bool ?? true }
    set { d.set(newValue, forKey: "CoverEnabled") }
  }
  static var coverColor: CoverColor {
    get { CoverColor(rawValue: d.string(forKey: "CoverColor") ?? "") ?? .auto }
    set { d.set(newValue.rawValue, forKey: "CoverColor") }
  }
  static var updateCheck: Bool {
    get { d.object(forKey: "UpdateCheck") as? Bool ?? true }
    set { d.set(newValue, forKey: "UpdateCheck") }
  }
  static var skippedVersion: String? {
    get { d.string(forKey: "SkippedVersion") }
    set { d.set(newValue, forKey: "SkippedVersion") }
  }
  static var notifiedVersion: String? {
    get { d.string(forKey: "NotifiedVersion") }
    set { d.set(newValue, forKey: "NotifiedVersion") }
  }
  static var lastVersion: String? {
    get { d.string(forKey: "LastVersion") }
    set { d.set(newValue, forKey: "LastVersion") }
  }
  static var setupDone: Bool {
    get { d.bool(forKey: "SetupDone") }
    set { d.set(newValue, forKey: "SetupDone") }
  }

  static func removeAll() { d.removePersistentDomain(forName: Config.bundleID) }
}
