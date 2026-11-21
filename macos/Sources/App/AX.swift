// Small helpers over the Accessibility API (reading KakaoTalk's windows) and the window server.

import AppKit
import ApplicationServices

enum AX {
  static func value(_ e: AXUIElement, _ attribute: String) -> AnyObject? {
    var v: AnyObject?
    return AXUIElementCopyAttributeValue(e, attribute as CFString, &v) == .success ? v : nil
  }
  static func string(_ e: AXUIElement, _ attribute: String) -> String? { value(e, attribute) as? String }
  static func role(_ e: AXUIElement) -> String? { string(e, kAXRoleAttribute) }
  static func bool(_ e: AXUIElement, _ attribute: String) -> Bool { value(e, attribute) as? Bool ?? false }
  static func children(_ e: AXUIElement) -> [AXUIElement] { value(e, kAXChildrenAttribute) as? [AXUIElement] ?? [] }
  static func windows(_ app: AXUIElement) -> [AXUIElement] { value(app, kAXWindowsAttribute) as? [AXUIElement] ?? [] }

  /// Frame in global screen coordinates, origin at the top left of the primary screen (like CGWindowList).
  static func frame(_ e: AXUIElement) -> CGRect? {
    guard let p = value(e, kAXPositionAttribute), let s = value(e, kAXSizeAttribute),
          CFGetTypeID(p) == AXValueGetTypeID(), CFGetTypeID(s) == AXValueGetTypeID() else { return nil }
    var point = CGPoint.zero, size = CGSize.zero
    guard AXValueGetValue(p as! AXValue, .cgPoint, &point), AXValueGetValue(s as! AXValue, .cgSize, &size) else { return nil }
    return CGRect(origin: point, size: size)
  }

  /// True when some element within `depth` levels below `e` has the role.
  static func contains(_ e: AXUIElement, role wanted: String, depth: Int) -> Bool {
    if role(e) == wanted { return true }
    guard depth > 0 else { return false }
    return children(e).contains { contains($0, role: wanted, depth: depth - 1) }
  }

  // The window server id of an AX window. _AXUIElementGetWindow is private but long-standing (used by
  // window managers such as Rectangle); if it ever goes away, windowID returns nil and the cover stays hidden.
  private typealias GetWindowFn = @convention(c) (AXUIElement, UnsafeMutablePointer<CGWindowID>) -> AXError
  private static let getWindow: GetWindowFn? = {
    guard let sym = dlsym(UnsafeMutableRawPointer(bitPattern: -2), "_AXUIElementGetWindow") else { return nil }   // RTLD_DEFAULT
    return unsafeBitCast(sym, to: GetWindowFn.self)
  }()
  static func windowID(_ e: AXUIElement) -> CGWindowID? {
    var id: CGWindowID = 0
    guard let f = getWindow, f(e, &id) == .success, id != 0 else { return nil }
    return id
  }
}

enum WindowServer {
  /// Current bounds and on-screen state of a window, straight from the window server. During a drag this is
  /// up to date every frame, while AX move notifications only arrive about 9 times a second.
  /// Bounds need no Screen Recording permission (only window titles would).
  static func info(_ id: CGWindowID) -> (bounds: CGRect, onScreen: Bool)? {
    guard let list = CGWindowListCopyWindowInfo(.optionIncludingWindow, id) as? [[String: Any]],
          let d = list.first, let b = d[kCGWindowBounds as String] as? NSDictionary,
          let rect = CGRect(dictionaryRepresentation: b) else { return nil }
    return (rect, d[kCGWindowIsOnscreen as String] as? Bool ?? false)
  }

  /// The front-most normal window under `point` (global top-left coordinates), ignoring `excluding`. Windows are
  /// hit-tested `grow` points beyond their bounds (resize handles sit just outside a window's edges).
  static func frontmostWindow(at point: CGPoint, excluding: CGWindowID, grow: CGFloat) -> CGWindowID? {
    guard let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID)
            as? [[String: Any]] else { return nil }
    for w in list {
      guard let id = w[kCGWindowNumber as String] as? CGWindowID, id != excluding,
            (w[kCGWindowLayer as String] as? Int) == 0,
            let b = w[kCGWindowBounds as String] as? NSDictionary, let r = CGRect(dictionaryRepresentation: b) else { continue }
      if r.insetBy(dx: -grow, dy: -grow).contains(point) { return id }
    }
    return nil
  }

  /// Whether any window between the normal and the floating level lies over `rect` in front of `window`
  /// (ignoring `ignoring`, our cover). Windows above the floating level (Dock, menu bar, ...) don't matter.
  static func isCovered(_ rect: CGRect, above window: CGWindowID, ignoring: CGWindowID) -> Bool {
    guard let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID)
            as? [[String: Any]] else { return true }
    let floatingLevel = Int(CGWindowLevelForKey(.floatingWindow))
    for w in list {
      guard let id = w[kCGWindowNumber as String] as? CGWindowID else { continue }
      if id == window { return false }
      if id == ignoring { continue }
      guard let layer = w[kCGWindowLayer as String] as? Int, layer >= 0, layer < floatingLevel,
            (w[kCGWindowAlpha as String] as? Double ?? 1) > 0,
            let b = w[kCGWindowBounds as String] as? NSDictionary, let r = CGRect(dictionaryRepresentation: b) else { continue }
      if r.intersects(rect.insetBy(dx: 0.5, dy: 0.5)) { return true }
    }
    return true   // KakaoTalk's window not found on screen: be safe
  }

  /// Global top-left coordinates -> AppKit screen coordinates (origin bottom left of the primary screen).
  static func toAppKit(_ r: CGRect) -> NSRect {
    let primaryHeight = NSScreen.screens.first?.frame.height ?? 0
    return NSRect(x: r.minX, y: primaryHeight - r.maxY, width: r.width, height: r.height)
  }
}
