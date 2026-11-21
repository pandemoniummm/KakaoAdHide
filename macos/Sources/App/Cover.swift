// Covers the ad banner at the bottom of KakaoTalk's main window with a window of our own.
//
// macOS gives no way to hide or resize a view inside another app's window (the Accessibility API can only move
// and resize whole windows), so unlike the Windows version the banner cannot be removed and the list cannot be
// stretched: the cover paints the banner's area in KakaoTalk's background colours and takes its clicks.
//
// How it follows KakaoTalk:
//   - AXObserver notifications (window moved/resized/minimized, app activated/hidden, ...) and a light 0.5s
//     safety check re-read the layout through the Accessibility API.
//   - While the mouse is held down on KakaoTalk's window, AX notifications arrive only ~9 times a second and
//     AppKit applies window frames only with its display cycle, so DragMover takes over and moves/reshapes the
//     cover directly in the window server (see SkyLight): from each mouse event via an event tap for a mouse,
//     from KakaoTalk's window bounds read about every millisecond for a trackpad (whichever measured best on
//     screen with real input).
//   - The cover window is wide and shaped (rounded bottom corners); a resize only changes its shape, which the
//     window server applies together with the position. Clicks within 5pt of the banner's edges go through to
//     KakaoTalk's resize handles.
//   - When nothing lies over the banner the cover floats above normal windows, so a press on KakaoTalk (which
//     raises its window) can't put the banner in front of it; when another window overlaps the banner it sits
//     at the normal level directly above KakaoTalk's window, under that other window.
//   - Nothing is shown unless the layout looks exactly like the banner (bottom of "Main Window", full width,
//     holding a web view), so a KakaoTalk redesign leaves the cover hidden rather than covering the wrong thing.

import AppKit
import ApplicationServices

/// KAKAOADHIDE_DEBUG=1: log on stderr (layout refreshes, drags).
let debugEnabled = ProcessInfo.processInfo.environment["KAKAOADHIDE_DEBUG"] != nil
func debug(_ s: @autoclosure () -> String) {
  guard debugEnabled else { return }
  // host time (seconds since boot, the same clock as screen capture timestamps)
  FileHandle.standardError.write(Data(String(format: "%.4f %@\n", CACurrentMediaTime(), s()).utf8))
}

final class CoverController {
  enum State: Equatable { case disabled, noPermission, waitingForKakao, covering, notFound }

  private(set) var state: State = .waitingForKakao { didSet { if state != oldValue { onStateChange?(state) } } }
  var onStateChange: ((State) -> Void)?

  private let panel = CoverPanel()
  private let leftWing = WingPanel(sidebarColour: true)
  private let bottomWing = WingPanel(sidebarColour: false)
  private var kakao: NSRunningApplication?
  private var appElement: AXUIElement?
  private var observer: AXObserver?
  private var safetyTimer: Timer?
  private var trackTimer: Timer?
  private var mover: DragMover?
  private var refreshQueued = false
  private var observedWindow: AXUIElement?
  private var mouseMonitor: Any?

  /// Where the banner sits relative to the window, so it can be placed from window bounds alone while tracking.
  private struct Layout: Equatable {
    var windowID: CGWindowID
    var left: CGFloat, right: CGFloat, bottom: CGFloat, height: CGFloat
    var sidebar: CGFloat
    func adRect(in window: CGRect) -> CGRect {
      CGRect(x: window.minX + left, y: window.maxY - bottom - height,
             width: window.width - left - right, height: height)
    }
  }
  private var layout: Layout?

  func start() {
    let nc = NSWorkspace.shared.notificationCenter
    nc.addObserver(forName: NSWorkspace.didLaunchApplicationNotification, object: nil, queue: .main) { [weak self] n in
      if let app = n.userInfo?[NSWorkspace.applicationUserInfoKey] as? NSRunningApplication,
         app.bundleIdentifier == Config.kakaoBundleID { self?.attach(app) }
    }
    nc.addObserver(forName: NSWorkspace.didTerminateApplicationNotification, object: nil, queue: .main) { [weak self] n in
      if let app = n.userInfo?[NSWorkspace.applicationUserInfoKey] as? NSRunningApplication,
         app.processIdentifier == self?.kakao?.processIdentifier { self?.detach() }
    }
    for name in [NSWorkspace.didActivateApplicationNotification, NSWorkspace.activeSpaceDidChangeNotification,
                 NSWorkspace.didHideApplicationNotification, NSWorkspace.didUnhideApplicationNotification] {
      nc.addObserver(forName: name, object: nil, queue: .main) { [weak self] _ in self?.queueRefresh() }
    }
    NotificationCenter.default.addObserver(forName: NSApplication.didChangeScreenParametersNotification, object: nil,
                                           queue: .main) { [weak self] _ in self?.queueRefresh() }

    // Pressing the mouse on KakaoTalk's window may start a move or resize: track from the first frame instead of
    // waiting for the first AX notification (~0.1s). Tracking a plain click costs nothing (bounds don't change).
    mouseMonitor = NSEvent.addGlobalMonitorForEvents(matching: .leftMouseDown) { [weak self] _ in self?.mouseDown() }

    safetyTimer = Timer.scheduledTimer(withTimeInterval: 0.5, repeats: true) { [weak self] _ in self?.safetyCheck() }
    levelTimer = Timer.scheduledTimer(withTimeInterval: 0.1, repeats: true) { [weak self] _ in self?.updateLevel() }
    safetyCheck()
  }

  func settingsChanged() {
    panel.colorsChanged()
    refresh()
  }

  // MARK: attaching to KakaoTalk

  private func safetyCheck() {
    if kakao == nil || kakao?.isTerminated == true || appElement == nil {
      if let app = NSRunningApplication.runningApplications(withBundleIdentifier: Config.kakaoBundleID).first {
        attach(app)
        return
      }
    } else if observer == nil {
      attach(kakao!)   // permission was granted after KakaoTalk started
      return
    }
    refresh()
  }

  private func attach(_ app: NSRunningApplication) {
    detach()
    kakao = app
    let element = AXUIElementCreateApplication(app.processIdentifier)
    AXUIElementSetMessagingTimeout(element, 0.25)   // a busy KakaoTalk (e.g. at startup) must not stall us
    appElement = element
    if AXIsProcessTrusted() {
      var obs: AXObserver?
      let callback: AXObserverCallback = { _, _, notification, refcon in
        guard let refcon else { return }
        let me = Unmanaged<CoverController>.fromOpaque(refcon).takeUnretainedValue()
        me.axNotification(notification as String)
      }
      if AXObserverCreate(app.processIdentifier, callback, &obs) == .success, let obs {
        let refcon = Unmanaged.passUnretained(self).toOpaque()
        for n in [kAXWindowCreatedNotification, kAXUIElementDestroyedNotification, kAXFocusedWindowChangedNotification,
                  kAXMainWindowChangedNotification, kAXWindowMovedNotification, kAXWindowResizedNotification,
                  kAXWindowMiniaturizedNotification, kAXWindowDeminiaturizedNotification,
                  kAXApplicationActivatedNotification, kAXApplicationDeactivatedNotification,
                  kAXApplicationHiddenNotification, kAXApplicationShownNotification] {
          AXObserverAddNotification(obs, element, n as CFString, refcon)
        }
        CFRunLoopAddSource(CFRunLoopGetMain(), AXObserverGetRunLoopSource(obs), .defaultMode)
        observer = obs
      }
    }
    refresh()
  }

  private func detach() {
    stopTracking()
    observedWindow = nil
    if let observer { CFRunLoopRemoveSource(CFRunLoopGetMain(), AXObserverGetRunLoopSource(observer), .defaultMode) }
    observer = nil
    appElement = nil
    kakao = nil
    hide()
    if Settings.coverEnabled && AXIsProcessTrusted() { state = .waitingForKakao }
  }

  private func axNotification(_ name: String) {
    debug("ax \(name) mouse=\(NSEvent.pressedMouseButtons) layout=\(layout != nil) tracking=\(mover != nil)")
    // Clicking KakaoTalk raises its window above the cover: while tracking, put the cover back right away.
    if [kAXApplicationActivatedNotification, kAXMainWindowChangedNotification, kAXFocusedWindowChangedNotification,
        kAXWindowCreatedNotification].contains(name) {
      if mover == nil { queueRefresh() }   // during a drag the cover floats above (see startTracking)
    }
    let moving = [kAXWindowMovedNotification, kAXWindowResizedNotification, kAXMovedNotification,
                  kAXResizedNotification].contains(name)
    if moving && NSEvent.pressedMouseButtons & 1 != 0 && layout != nil {
      startTracking()
      return
    }
    queueRefresh()
  }

  /// Several notifications usually arrive together: re-read the layout once.
  private func queueRefresh() {
    guard !refreshQueued else { return }
    refreshQueued = true
    DispatchQueue.main.async { [weak self] in
      self?.refreshQueued = false
      self?.refresh()
    }
  }

  // MARK: layout

  private func refresh() {
    guard mover == nil else { return }   // the drag mover owns the position
    let t0 = Date()
    defer { debug(String(format: "refresh %.1fms state=%@", Date().timeIntervalSince(t0) * 1000, "\(state)")) }
    guard Settings.coverEnabled else { state = .disabled; hide(); return }
    guard AXIsProcessTrusted() else { state = .noPermission; hide(); return }
    guard let app = kakao, !app.isTerminated, let element = appElement else { state = .waitingForKakao; hide(); return }
    guard !app.isHidden else { hide(); return }
    switch locate(element) {
    case .found(let l, _, let win):
      absentSince = nil
      state = .covering
      observe(win)
      layout = l
      guard let ws = WindowServer.info(l.windowID), ws.onScreen else { hide(); return }   // other Space
      show(l, window: ws.bounds, reorder: true)   // the window server's bounds are the freshest
      updateLevel()
    case .unavailable:
      // KakaoTalk is busy: keep the last layout, placed from the window server's bounds.
      guard let l = layout, let ws = WindowServer.info(l.windowID) else { return }
      if ws.onScreen { show(l, window: ws.bounds, reorder: true) } else { hide() }
    case .absent:
      // Right after a drag (and while KakaoTalk relays out) the banner can briefly fail the checks: if the window
      // is still on screen, keep covering from the last layout and hide only when it stays absent for a second.
      if let l = layout, let ws = WindowServer.info(l.windowID), ws.onScreen {
        if absentSince == nil { absentSince = Date() }
        if Date().timeIntervalSince(absentSince!) < 1 {
          show(l, window: ws.bounds, reorder: true)
          DispatchQueue.main.asyncAfter(deadline: .now() + 0.25) { [weak self] in self?.refresh() }
          return
        }
      }
      absentSince = nil
      state = .notFound
      layout = nil
      hide()
    }
  }

  /// Move/resize notifications registered on the application element alone are not delivered for windows created
  /// after the observer (KakaoTalk opening its main window later): register them on the main window itself.
  private func observe(_ window: AXUIElement) {
    guard let observer else { return }
    if let old = observedWindow {
      if CFEqual(old, window) { return }
      for n in [kAXMovedNotification, kAXResizedNotification] {
        AXObserverRemoveNotification(observer, old, n as CFString)
      }
    }
    let refcon = Unmanaged.passUnretained(self).toOpaque()
    for n in [kAXMovedNotification, kAXResizedNotification] {
      AXObserverAddNotification(observer, window, n as CFString, refcon)
    }
    observedWindow = window
  }

  private enum Located { case found(Layout, CGRect, AXUIElement), absent, unavailable }

  /// The banner in KakaoTalk's main window, if it is there and looks like the banner. `unavailable` when
  /// KakaoTalk didn't answer (busy, e.g. relaying out during a resize): the last layout is kept then.
  private func locate(_ app: AXUIElement) -> Located {
    var windowsRef: AnyObject?
    let err = AXUIElementCopyAttributeValue(app, kAXWindowsAttribute as CFString, &windowsRef)
    if err == .cannotComplete { return .unavailable }
    guard let win = (windowsRef as? [AXUIElement] ?? []).first(where: { AX.string($0, "AXIdentifier") == "Main Window" }),
          !AX.bool(win, kAXMinimizedAttribute), let wid = AX.windowID(win) else { return .absent }
    guard let wr = AX.frame(win) else { return .unavailable }
    let kids = AX.children(win)
    if kids.isEmpty { return .unavailable }
    guard let ad = kids.first(where: { AX.role($0) == kAXGroupRole && AX.contains($0, role: "AXWebArea", depth: 5) }),
          let ar = AX.frame(ad) else { return .absent }
    // Must sit at the bottom edge, span (nearly) the whole width and be banner-sized.
    guard (30...250).contains(ar.height), ar.width >= wr.width * 0.8,
          abs(ar.maxY - wr.maxY) <= 4, ar.minX >= wr.minX - 1, ar.maxX <= wr.maxX + 1 else { return .absent }
    // The list (tallest scroll area) starts right of the sidebar; below it the cover uses the list colour.
    let list = kids.filter { AX.role($0) == kAXScrollAreaRole }.compactMap(AX.frame).max { $0.height < $1.height }
    let sidebar = list.map { min(max($0.minX - ar.minX, 0), ar.width) } ?? 0
    let l = Layout(windowID: wid, left: ar.minX - wr.minX, right: wr.maxX - ar.maxX, bottom: wr.maxY - ar.maxY,
                   height: ar.height, sidebar: sidebar)
    return .found(l, wr, win)
  }

  private func show(_ l: Layout, window: CGRect, reorder: Bool) {
    let frame = WindowServer.toAppKit(l.adRect(in: window))
    let appearing = !panel.isVisible
    panel.place(frame, sidebar: l.sidebar)
    if reorder || appearing { panel.order(.above, relativeTo: Int(l.windowID)) }
    if appearing { panel.place(frame, sidebar: l.sidebar) }   // AppKit may reset the shape when the window appears
  }

  private func hide() {
    if panel.isVisible { panel.orderOut(nil) }
    if floating && mover == nil { floating = false; panel.level = .normal }
  }

  // MARK: drag tracking

  private func startTracking() {
    guard mover == nil, let l = layout, panel.isVisible else { return }
    debug("track start")
    let coverID = CGWindowID(panel.windowNumber)
    mover = DragMover(kakao: l.windowID, cover: coverID, place: { l.adRect(in: $0) },
                      onWidth: { [weak self] w in self?.panel.setVisibleWidth(w) },
                      onResize: { [weak self] bounds in
                        self?.show(l, window: bounds, reorder: false)
                        CATransaction.flush()   // hand the new frame to the window server now, not at the next cycle
                      },
                      onEnd: { [weak self] in self?.endTracking() })
    if SkyLight.shapesAvailable {
      mover?.leftWing = leftWing.showEmpty()
      mover?.bottomWing = bottomWing.showEmpty()
    }
    // Re-ordering the cover through AppKit while the window moves would re-apply AppKit's own, then stale, idea of
    // the frame (dragging the cover back to where the drag started), so it floats until the mouse is released.
    // Tracking only starts when KakaoTalk's window is the front-most one under the mouse, and the press makes it
    // the front window, so nothing else gets covered.
    setFloating(true)
  }

  // MARK: level

  // A press on KakaoTalk makes the window server raise KakaoTalk's window above the cover before we can react
  // (one frame of banner). So whenever nothing else lies over the banner the cover already floats above normal
  // windows: it looks the same, and the raise can't put KakaoTalk over it. As soon as another window (another
  // app's, or a KakaoTalk chat window) overlaps the banner, the cover goes back to the normal level directly above
  // KakaoTalk's window. Checked 10 times a second, on every mouse press and on every layout refresh.
  private var floating = false
  private var absentSince: Date?
  private var levelTimer: Timer?

  private func setFloating(_ on: Bool) {
    guard on != floating, let l = layout else { return }
    floating = on
    panel.level = on ? .floating : .normal
    guard let ws = WindowServer.info(l.windowID) else { return }
    show(l, window: ws.bounds, reorder: !on)   // back above KakaoTalk; and reshape in case AppKit reset the shape
  }

  private func updateLevel() {
    guard mover == nil, panel.isVisible, let l = layout, let ws = WindowServer.info(l.windowID) else { return }
    let banner = l.adRect(in: ws.bounds)
    setFloating(!WindowServer.isCovered(banner, above: l.windowID, ignoring: CGWindowID(panel.windowNumber)))
  }

  private func endTracking() {
    debug("track end")
    let l = layout
    stopTracking()
    if let l, let ws = WindowServer.info(l.windowID) {
      panel.sync(WindowServer.toAppKit(l.adRect(in: ws.bounds)), sidebar: l.sidebar)
    }
    refresh()   // re-reads the layout and decides the level again
    // AX can still report the pre-drop layout for a moment: look again shortly after.
    for delay in [0.15, 0.5] { DispatchQueue.main.asyncAfter(deadline: .now() + delay) { [weak self] in self?.refresh() } }
  }

  private func mouseDown() {
    updateLevel()   // a press elsewhere may bring another window over the banner
    guard let l = layout, panel.isVisible, let ws = WindowServer.info(l.windowID) else { return }
    let m = NSEvent.mouseLocation   // AppKit coordinates
    let primaryHeight = NSScreen.screens.first?.frame.height ?? 0
    let p = CGPoint(x: m.x, y: primaryHeight - m.y)
    guard ws.bounds.insetBy(dx: -8, dy: -8).contains(p) else { return }   // the resize handles sit just outside
    // Only when the press lands on KakaoTalk itself, not on another app's window lying over it.
    if WindowServer.frontmostWindow(at: p, excluding: CGWindowID(panel.windowNumber), grow: 8) == l.windowID { startTracking() }
  }

  private func stopTracking() {
    mover?.stop()
    mover = nil
    leftWing.orderOut(nil)
    bottomWing.orderOut(nil)
    trackTimer?.invalidate()
    trackTimer = nil
  }
}

/// The cover: a borderless, non-activating panel (clicking it doesn't take focus away from KakaoTalk),
/// left out of Mission Control and the window cycle.
final class CoverPanel: NSPanel {
  private let view = CoverView()

  init() {
    super.init(contentRect: .zero, styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: true)
    isOpaque = false            // transparent outside the rounded bottom corners
    backgroundColor = .clear
    hasShadow = false
    isReleasedWhenClosed = false
    animationBehavior = .none
    hidesOnDeactivate = false
    level = .normal
    collectionBehavior = [.transient, .ignoresCycle, .fullScreenAuxiliary]
    contentView = view
    NotificationCenter.default.addObserver(forName: NSWindow.didChangeScreenNotification, object: self, queue: .main) {
      [weak self] _ in self?.view.applyColors()
    }
  }
  override var canBecomeKey: Bool { false }
  override var canBecomeMain: Bool { false }

  /// How far inside the banner's edges the cover takes clicks: KakaoTalk's resize handles are under its edges.
  static let clickEdge: CGFloat = 5
  /// With window shapes the window itself is this wide and only the banner's width is shown, so a resize needs no
  /// new window size (which AppKit applies only with its display cycle).
  static var wideWidth: CGFloat { (NSScreen.screens.map(\.frame.width).max() ?? 2000) + 400 }
  /// Largest overhang during fast drags (see DragMover.overhang).
  static let maxOverhang: CGFloat = 140

  func place(_ frame: NSRect, sidebar: CGFloat) {
    view.sidebar = sidebar
    view.visibleWidth = frame.width
    if SkyLight.shapesAvailable {
      let wide = NSRect(x: frame.minX, y: frame.minY, width: Self.wideWidth, height: frame.height)
      if self.frame != wide { setFrame(wide, display: false) }
      if windowNumber > 0 {   // no window server window before it is first shown (show() shapes it right after)
        let primaryHeight = NSScreen.screens.first?.frame.height ?? 0
        SkyLight.cover(CGWindowID(windowNumber),
                       ad: CGRect(x: frame.minX, y: primaryHeight - frame.maxY, width: frame.width, height: frame.height))
      }
    } else if self.frame != frame {
      setFrame(frame, display: false)   // layers resize without redrawing anything
    }
  }

  func colorsChanged() { view.applyColors() }

  /// During a resize drag the mover reshapes the window directly; the rounded right corner follows here.
  func setVisibleWidth(_ w: CGFloat) {
    view.visibleWidth = w
    CATransaction.flush()
  }

  /// After the drag mover moved the window behind AppKit's back: bring both to the final position.
  func sync(_ frame: NSRect, sidebar: CGFloat) {
    place(frame, sidebar: sidebar)   // AppKit's idea of the frame, and the real position and shape (no overhang)
  }
}

/// A solid, click-through window that widens the cover during fast drags (see DragMover.applyOverhang).
final class WingPanel: NSPanel {
  private let sidebarColour: Bool
  init(sidebarColour: Bool) {
    self.sidebarColour = sidebarColour
    super.init(contentRect: NSRect(x: -20000, y: -20000, width: 400, height: 140), styleMask: [.borderless, .nonactivatingPanel],
               backing: .buffered, defer: true)
    isOpaque = true
    hasShadow = false
    ignoresMouseEvents = true
    isReleasedWhenClosed = false
    animationBehavior = .none
    hidesOnDeactivate = false
    level = .floating
    collectionBehavior = [.transient, .ignoresCycle, .fullScreenAuxiliary]
    applyColors()
  }
  override var canBecomeKey: Bool { false }
  override var canBecomeMain: Bool { false }
  func applyColors() {
    let g = CoverView.greys(for: NSApp.effectiveAppearance)
    let v = (sidebarColour ? g.sidebar : g.list) / 255
    backgroundColor = NSColor(srgbRed: v, green: v, blue: v, alpha: 1)
  }
  /// Shown (but empty, see SkyLight.showWing) while a drag is followed; the window server id for the mover.
  func showEmpty() -> CGWindowID {
    applyColors()
    // As big as any overhang can get, so the window server shape never needs more than the window has.
    setFrame(NSRect(x: -20000, y: -20000, width: (NSScreen.screens.map(\.frame.width).max() ?? 2000) + 400,
                    height: CoverPanel.maxOverhang + 140), display: false)
    orderFront(nil)
    let id = CGWindowID(max(windowNumber, 0))
    if id != 0 { SkyLight.showWing(id, .zero) }
    return id
  }
}

/// Paints KakaoTalk's background with layers (nothing is drawn per frame): the sidebar colour on the left, the
/// list colour on the rest, so the list looks like it simply ends above an empty strip. The bottom corners are
/// rounded like KakaoTalk's window.
final class CoverView: NSView {
  /// KakaoTalk's window corner radius (measured ~16pt on macOS 26/27).
  static let cornerRadius: CGFloat = 16

  var sidebar: CGFloat = 0 { didSet { if sidebar != oldValue { needsLayout = true } } }
  /// Width of the banner actually shown (the window itself is wider when it is shaped): the right corner goes here.
  var visibleWidth: CGFloat = 0 { didSet { if visibleWidth != oldValue { updateMask() } } }
  private let sidebarLayer = CALayer()
  private let maskLayer = CAShapeLayer()
  /// Where the banner starts inside the view.
  private let originX: CGFloat = 0

  // KakaoTalk 26.10's background greys, as stored in a screenshot's pixels (raw values, not colour-converted).
  private struct Palette { let sidebar: CGFloat; let list: CGFloat }   // grey levels 0...255
  private static let dark = Palette(sidebar: 0x2D, list: 0x22)
  private static let light = Palette(sidebar: 0xF2, list: 0xFF)

  override init(frame: NSRect) {
    super.init(frame: frame)
    wantsLayer = true
    let root = CALayer()
    root.masksToBounds = true
    root.addSublayer(sidebarLayer)
    // The window shape only clips to rectangles here, so the rounded bottom corners are cut out of the content:
    // a mask that is the whole layer minus the two corner pieces outside a 16pt arc (even-odd fill).
    maskLayer.fillRule = .evenOdd
    maskLayer.actions = ["path": NSNull(), "bounds": NSNull(), "position": NSNull()]
    if ProcessInfo.processInfo.environment["KAKAOADHIDE_NOMASK"] == nil { root.mask = maskLayer }
    root.actions = ["bounds": NSNull(), "position": NSNull(), "backgroundColor": NSNull()]
    sidebarLayer.actions = ["bounds": NSNull(), "position": NSNull(), "frame": NSNull(), "backgroundColor": NSNull()]
    layer = root
    applyColors()
  }
  required init?(coder: NSCoder) { fatalError() }

  override func layout() {
    super.layout()
    CATransaction.begin()
    CATransaction.setDisableActions(true)
    sidebarLayer.frame = CGRect(x: 0, y: 0, width: min(originX + sidebar, bounds.width), height: bounds.height)
    CATransaction.commit()
    updateMask()
  }

  private func updateMask() {
    let x0 = originX
    let w = visibleWidth > 0 ? min(visibleWidth, bounds.width - x0) : bounds.width - x0
    let r = min(Self.cornerRadius, w / 2, bounds.height)
    let path = CGMutablePath()
    path.addRect(bounds)
    // bottom left of the banner: the piece between the corner and the arc (AppKit layers: y up, bottom at 0)
    path.move(to: CGPoint(x: x0, y: 0))
    path.addLine(to: CGPoint(x: x0 + r, y: 0))
    path.addArc(center: CGPoint(x: x0 + r, y: r), radius: r, startAngle: -.pi / 2, endAngle: .pi, clockwise: true)
    path.closeSubpath()
    // bottom right, at the visible width
    path.move(to: CGPoint(x: x0 + w, y: 0))
    path.addLine(to: CGPoint(x: x0 + w - r, y: 0))
    path.addArc(center: CGPoint(x: x0 + w - r, y: r), radius: r, startAngle: -.pi / 2, endAngle: 0, clockwise: false)
    path.closeSubpath()
    CATransaction.begin()
    CATransaction.setDisableActions(true)
    maskLayer.frame = bounds
    maskLayer.path = path
    CATransaction.commit()
  }

  override func viewDidChangeEffectiveAppearance() { applyColors() }

  /// Sidebar and list colours for the current setting and appearance, as grey levels.
  static func greys(for appearance: NSAppearance) -> (sidebar: CGFloat, list: CGFloat) {
    let dark: Bool
    switch Settings.coverColor {
    case .light: dark = false
    case .dark: dark = true
    case .auto: dark = appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
    }
    let p = dark ? Self.dark : Self.light
    return (p.sidebar, p.list)
  }

  func applyColors() {
    let g = Self.greys(for: effectiveAppearance)
    let p = Palette(sidebar: g.sidebar, list: g.list)
    let space = window?.screen?.colorSpace?.cgColorSpace ?? NSScreen.main?.colorSpace?.cgColorSpace
      ?? CGColorSpace(name: CGColorSpace.sRGB)!
    func grey(_ v: CGFloat) -> CGColor {
      let c = v / 255
      return CGColor(colorSpace: space, components: [c, c, c, 1]) ?? CGColor(gray: c, alpha: 1)
    }
    CATransaction.begin()
    CATransaction.setDisableActions(true)
    layer?.backgroundColor = grey(p.list)
    sidebarLayer.backgroundColor = grey(p.sidebar)
    CATransaction.commit()
  }

  // Swallow clicks so the ad underneath can't be opened by accident.
  override func mouseDown(with event: NSEvent) {}
  override func rightMouseDown(with event: NSEvent) {}
  override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }
}

/// Follows a window drag. With window shapes (the usual case) every change, move or resize, is a window server
/// move plus, when the width changes, a new shape. Without them:
/// AX move notifications arrive only ~9 times a second and AppKit applies window frames
/// with its display cycle, so neither is used for moves while the mouse is down: a background thread asks the
/// window server for KakaoTalk's bounds about every millisecond and moves the cover directly in the window
/// server (SLSMoveWindow), within a millisecond or two of KakaoTalk's window. Once the size changes (a resize),
/// the rest of the drag goes through AppKit only, latest bounds only: mixing the two paths applied position and
/// size in different frames. Stops when the left mouse button is released.
final class DragMover {
  private let lock = NSLock()
  private var running = true
  // Event tap (see startTap): window origin - mouse location while the window moves with the mouse.
  private var anchored = false
  private var anchorOffset = CGPoint.zero
  private var anchorSize = CGSize.zero
  private var coverID: CGWindowID = 0
  private var placeFn: ((CGRect) -> CGRect)?
  private var tap: CFMachPort?
  private var tapLoop: CFRunLoop?
  private var fromTrackpad = false      // drag events come from a trackpad (see mouseDragged)
  private var loggedSubtype = false
  private var pendingResize: CGRect?   // latest bounds waiting for the main thread (never queued up)

  init(kakao: CGWindowID, cover: CGWindowID, place: @escaping (CGRect) -> CGRect,
       onWidth: @escaping (CGFloat) -> Void,
       onResize: @escaping (CGRect) -> Void, onEnd: @escaping () -> Void) {
    let direct = SkyLight.available
    coverID = cover
    placeFn = place
    DragMover.dragCount += 1
    let modes = DragMover.abModes
    mode = DragMover.abTest ? modes[(DragMover.dragCount - 1) % modes.count] : DragMover.defaultMode
    debug("drag \(DragMover.dragCount) mode \(mode)")
    var isPoll = false
    if case .poll = mode { isPoll = true }
    let useTap = direct && DragMover.tapEnabled && !isPoll
    if useTap { startTap() }
    let thread = Thread { [weak self] in
      var last = CGRect.null
      var lastSize = CGSize.zero
      var resizing = false
      let shaped = SkyLight.shapesAvailable
      var prev: (bounds: CGRect, time: Double)?
      var lastOffset: CGPoint?
      var mismatchSince: Double?
      var lastBoundsSize = CGSize.zero
      var prev2: (bounds: CGRect, time: Double)?
      var speed: CGFloat = 0        // horizontal pt/s, smoothed
      var overhang: CGFloat = 0
      let lead = DragMover.leadSeconds
      while let me = self, me.isRunning {
        if !CGEventSource.buttonState(.combinedSessionState, button: .left) {
          DispatchQueue.main.async(execute: onEnd)
          return
        }
        if let b = WindowServer.info(kakao)?.bounds, b != last {
          var r = place(b)
          // Optional lead along the window's velocity. Measured on screen it only made things worse (the cover
          // already lands with KakaoTalk's move), so it is off unless KAKAOADHIDE_LEAD_MS is set.
          let now = CACurrentMediaTime()
          if lead > 0, let p = prev, p.bounds.size == b.size, now - p.time > 0.0005, now - p.time < 0.05 {
            let k = lead / (now - p.time)
            let dx = (b.minX - p.bounds.minX) * k, dy = (b.minY - p.bounds.minY) * k
            if abs(dx) < 60 && abs(dy) < 60 { r = r.offsetBy(dx: dx, dy: dy) }
          }
          prev = (b, now)
          if useTap {
            // A title bar drag keeps a fixed offset between the window and the mouse: once seen, the event tap
            // moves the cover from each mouse event, before the window server moves KakaoTalk's window by it.
            let mouse = CGEvent(source: nil)?.location ?? .zero
            let off = CGPoint(x: b.minX - mouse.x, y: b.minY - mouse.y)
            let same = lastOffset.map { abs($0.x - off.x) < 1 && abs($0.y - off.y) < 1 } ?? false
            lastOffset = off
            if !last.isNull && b.size != lastBoundsSize { me.clearAnchor() }   // resizing: the poller moves it
            else if same { mismatchSince = nil; me.setAnchor(off, b.size) }
            else if me.isAnchored {
              // mouse and window disagree (an event in flight, or the window stopped at the menu bar):
              // give it a moment before taking the moves back
              if mismatchSince == nil { mismatchSince = now }
              if now - mismatchSince! > 0.03 { me.clearAnchor() }
            }
            if me.isAnchored {
              last = b; lastSize = r.size; lastBoundsSize = b.size
              usleep(1000)
              continue
            }
          }
          lastBoundsSize = b.size
          if shaped && r.height == lastSize.height {
            // The cover window is wide and shaped: a resize is just a new position and shape, applied by the
            // window server at once, like a plain move. While the window moves fast the cover is widened by
            // about one frame of movement on both sides, so a frame of lag or lead shows no banner.
            if !last.isNull && b.size == last.size, let p = prev2 {
              let dt = now - p.time
              if dt > 0.0005 { speed = 0.7 * speed + 0.3 * abs(b.minX - p.bounds.minX) / dt }
            }
            prev2 = (b, now)
            overhang = DragMover.overhang(forStep: speed / 120, previous: overhang)
            me.applyOverhang(ad: r, overhang)
            if r.width != lastSize.width { me.requestWidth(r.width, onWidth) }
          } else {
            if !last.isNull && r.size != lastSize { resizing = true }
            if direct && !resizing && !last.isNull { _ = SkyLight.move(cover, topLeft: r.origin) }
            else { me.requestResize(b, onResize) }
          }
          last = b
          lastSize = r.size
        }
        usleep(1000)
      }
    }
    thread.qualityOfService = .userInteractive
    thread.name = "KakaoAdHide.DragMover"
    thread.start()
  }

  /// How far ahead to place a moving cover (KAKAOADHIDE_LEAD_MS overrides, for tuning).
  static let leadSeconds: Double = (ProcessInfo.processInfo.environment["KAKAOADHIDE_LEAD_MS"].flatMap(Double.init) ?? 0) / 1000

  /// The event tap path (KAKAOADHIDE_TAP=0 turns it off, for comparison).
  static let tapEnabled = ProcessInfo.processInfo.environment["KAKAOADHIDE_TAP"] != "0"
  /// KAKAOADHIDE_ABTEST=1: each drag uses the next placement mode (logged), for measuring with real input.
  static let abTest = ProcessInfo.processInfo.environment["KAKAOADHIDE_ABTEST"] != nil
  static var dragCount = 0
  /// Where a mouse drag event puts the cover: a fraction of the way from the previous event's window position to
  /// this event's (tap(1) = this event's), or nowhere (poll: the poller follows the window). Measured on screen with
  /// real and hardware-like mouse drags, tap(1) was best (0.9/0.8/0.7 and "previous event" trailed more, 1.1 led more).
  /// KAKAOADHIDE_ABTEST=1 with KAKAOADHIDE_ABMODES cycles modes per drag, for measuring.
  enum Mode: CustomStringConvertible {
    case tap(CGFloat)   // fraction of the way from the previous event's window position to this event's
    case poll
    var description: String { if case .tap(let k) = self { return "tap\(k)" }; return "poll" }
  }
  static let defaultMode: Mode = .tap(1)
  /// KAKAOADHIDE_ABMODES="1,0.85,0.7,poll"
  static let abModes: [Mode] = (ProcessInfo.processInfo.environment["KAKAOADHIDE_ABMODES"] ?? "1,poll")
    .split(separator: ",").map { $0 == "poll" ? .poll : .tap(CGFloat(Double($0) ?? 1)) }
  private var mode: Mode = .tap(1)
  private var previousWindow: CGRect?
  private var tapOverhang: CGFloat = 0
  /// Solid windows that widen the cover to the left and below during fast drags (the cover itself can only
  /// extend to the right, see SkyLight.cover). Created and shown by the controller.
  var leftWing: CGWindowID = 0
  var bottomWing: CGWindowID = 0

  /// Places the cover over `ad` widened by `m` points: to the right by the cover's own shape, to the left and
  /// below by the wings.
  fileprivate func applyOverhang(ad: CGRect, _ m: CGFloat) {
    SkyLight.cover(coverID, ad: ad, right: m)
    if leftWing != 0 { SkyLight.showWing(leftWing, m >= 1 ? CGRect(x: ad.minX - m, y: ad.minY, width: m, height: ad.height) : .zero) }
    if bottomWing != 0 {
      SkyLight.showWing(bottomWing, m >= 1 ? CGRect(x: ad.minX - m, y: ad.maxY, width: ad.width + 2 * m, height: min(m, 40)) : .zero)
    }
  }

  /// How far to widen the cover on each side for a window moving `step` points per event/frame: a bit over one step
  /// (the cover can be up to a frame ahead or behind), nothing for slow moves, at most CoverPanel.maxOverhang.
  /// Shrinks gradually so it doesn't flicker between events.
  static func overhang(forStep step: CGFloat, previous: CGFloat) -> CGFloat {
    let want = step < 2 ? 0 : min(step * 1.3 + 2, CoverPanel.maxOverhang)
    return max(want, previous * 0.8 < 1 ? 0 : previous * 0.8)
  }

  private var isRunning: Bool { lock.lock(); defer { lock.unlock() }; return running }
  private var isAnchored: Bool { lock.lock(); defer { lock.unlock() }; return anchored }
  private func setAnchor(_ off: CGPoint, _ size: CGSize) {
    lock.lock(); if !fromTrackpad { anchorOffset = off; anchorSize = size; anchored = true }; lock.unlock()
  }
  private func clearAnchor() { lock.lock(); anchored = false; lock.unlock() }

  func stop() {
    lock.lock(); running = false; lock.unlock()
    if let tap { CGEvent.tapEnable(tap: tap, enable: false) }
    if let tapLoop { CFRunLoopStop(tapLoop) }
  }

  /// Called on the tap thread for each mouse drag event, before the window server acts on it.
  /// Measured on screen with real input: for a mouse (large steps ~125 times a second) moving the cover from the
  /// event did best (0.9% of frames with a visible strip of the banner, vs 1.2% reading the window's position
  /// afterwards, and 3.2% when the event's position was applied at the next display refresh instead); for a
  /// trackpad the window server moves the window differently and the cover ended up ahead of it, so trackpad
  /// drags are left to the poller (0.2% vs 1.2%).
  fileprivate func mouseDragged(_ event: CGEvent) {
    let location = event.location
    let subtype = event.getIntegerValueField(.mouseEventSubtype)
    lock.lock()
    if !loggedSubtype { loggedSubtype = true; debug("drag event subtype \(subtype)") }
    if subtype != 0 && !fromTrackpad { fromTrackpad = true; anchored = false }
    let go = anchored && running && !fromTrackpad
    var window = CGRect(origin: CGPoint(x: location.x + anchorOffset.x, y: location.y + anchorOffset.y), size: anchorSize)
    if go, let p = previousWindow, p.size == window.size {
      let current = window
      tapOverhang = DragMover.overhang(forStep: abs(current.minX - p.minX), previous: tapOverhang)
      if case .tap(let k) = mode, k != 1 {
        window.origin = CGPoint(x: p.minX + (current.minX - p.minX) * k, y: p.minY + (current.minY - p.minY) * k)
      }
      previousWindow = current
    } else if go {
      previousWindow = window
    }
    let place = placeFn
    let m = tapOverhang
    lock.unlock()
    if go, let place {
      if SkyLight.shapesAvailable { applyOverhang(ad: place(window), m) }
      else { _ = SkyLight.move(coverID, topLeft: place(window).origin) }
    }
  }

  private func startTap() {
    let mask = CGEventMask(1 << CGEventType.leftMouseDragged.rawValue)
    let refcon = Unmanaged.passUnretained(self).toOpaque()
    // A default (not listen-only) tap runs synchronously, before the event moves KakaoTalk's window. It only reads
    // the event and returns it unchanged, and exists only while the mouse is down.
    guard let tap = CGEvent.tapCreate(tap: .cghidEventTap, place: .headInsertEventTap, options: .defaultTap,
                                      eventsOfInterest: mask, callback: { _, type, event, refcon in
      if type == .leftMouseDragged, let refcon {
        Unmanaged<DragMover>.fromOpaque(refcon).takeUnretainedValue().mouseDragged(event)
      }
      return Unmanaged.passUnretained(event)
    }, userInfo: refcon) else { return }
    self.tap = tap
    let source = CFMachPortCreateRunLoopSource(nil, tap, 0)
    let thread = Thread { [weak self] in
      guard let self else { return }
      self.lock.lock(); self.tapLoop = CFRunLoopGetCurrent(); self.lock.unlock()
      CFRunLoopAddSource(CFRunLoopGetCurrent(), source, .commonModes)
      CGEvent.tapEnable(tap: tap, enable: true)
      while self.isRunning { _ = CFRunLoopRunInMode(.defaultMode, 0.25, false) }
      CFMachPortInvalidate(tap)
    }
    thread.qualityOfService = .userInteractive
    thread.name = "KakaoAdHide.DragTap"
    thread.start()
  }

  private var pendingWidth: CGFloat?
  private func requestWidth(_ w: CGFloat, _ apply: @escaping (CGFloat) -> Void) {
    lock.lock()
    let queued = pendingWidth != nil
    pendingWidth = w
    lock.unlock()
    guard !queued else { return }
    DispatchQueue.main.async { [weak self] in
      guard let self else { return }
      self.lock.lock(); let w = self.pendingWidth; self.pendingWidth = nil; self.lock.unlock()
      if let w { apply(w) }
    }
  }

  private func requestResize(_ bounds: CGRect, _ apply: @escaping (CGRect) -> Void) {
    lock.lock()
    let alreadyQueued = pendingResize != nil
    pendingResize = bounds
    lock.unlock()
    guard !alreadyQueued else { return }
    DispatchQueue.main.async { [weak self] in
      guard let self else { return }
      self.lock.lock()
      let b = self.pendingResize
      self.pendingResize = nil
      self.lock.unlock()
      if let b { apply(b) }
    }
  }
}

/// The window server calls made directly, all on our own cover window: moving it and setting its shape (the visible
/// part, with rounded bottom corners) and event shape (where it takes clicks). Private (SkyLight) but
/// long-standing; if any is missing, the cover falls back to plain AppKit frames.
enum SkyLight {
  private typealias MainConnectionFn = @convention(c) () -> Int32
  private typealias MoveWindowFn = @convention(c) (Int32, UInt32, UnsafePointer<CGPoint>) -> Int32
  private typealias NewRegionFn = @convention(c) (UnsafePointer<CGRect>, UInt32, UnsafeMutablePointer<OpaquePointer?>) -> Int32
  private typealias ReleaseRegionFn = @convention(c) (OpaquePointer) -> Int32
  private typealias SetShapeFn = @convention(c) (Int32, UInt32, Float, Float, OpaquePointer) -> Int32   // x, y: window origin
  private typealias SetEventShapeFn = @convention(c) (Int32, UInt32, OpaquePointer) -> Int32

  private static let sky = dlopen("/System/Library/PrivateFrameworks/SkyLight.framework/SkyLight", RTLD_NOW)
  private static let cg = dlopen("/System/Library/Frameworks/CoreGraphics.framework/CoreGraphics", RTLD_NOW)
  private static func fn<T>(_ lib: UnsafeMutableRawPointer?, _ name: String, _: T.Type) -> T? {
    guard let lib, let f = dlsym(lib, name) else { return nil }
    return unsafeBitCast(f, to: T.self)
  }
  private static let connection: Int32? = fn(sky, "SLSMainConnectionID", MainConnectionFn.self).map { $0() }
  private static let moveWindow = fn(sky, "SLSMoveWindow", MoveWindowFn.self)
  private static let newRegion = fn(cg, "CGSNewRegionWithRectList", NewRegionFn.self)
  private static let releaseRegion = fn(cg, "CGSReleaseRegion", ReleaseRegionFn.self)
  private static let setShape = fn(sky, "SLSSetWindowShape", SetShapeFn.self)
  private static let setEventShape = fn(sky, "SLSSetWindowEventShape", SetEventShapeFn.self)

  static var available: Bool { connection != nil && moveWindow != nil }
  static let shapesAvailable: Bool = available && newRegion != nil && releaseRegion != nil && setShape != nil && setEventShape != nil

  /// `topLeft` in global coordinates with the origin at the top left of the primary screen (like CGWindowList).
  static func move(_ window: CGWindowID, topLeft: CGPoint) -> Bool {
    guard let c = connection, let f = moveWindow else { return false }
    var p = topLeft
    return f(c, window, &p) == 0
  }

  /// Shows the cover over `ad` (global top-left coordinates), extended `right` points to the right, in one window
  /// server call (position and shape together). The window server anchors the window's content at the shape's
  /// left edge, so the shape always starts at the banner's left edge (overhang to the left and below is drawn by
  /// separate wing windows, see DragMover). Clicks are taken only over the banner itself, CoverPanel.clickEdge
  /// points inside its edges (KakaoTalk's resize handles).
  @discardableResult
  static func cover(_ window: CGWindowID, ad: CGRect, right: CGFloat = 0) -> Bool {
    guard let c = connection, let setShape, let setEventShape else { return false }
    let edge = CoverPanel.clickEdge
    var rects = [CGRect(x: 0, y: 0, width: ad.width + max(0, right), height: ad.height)]
    var events = [CGRect(x: edge, y: edge, width: max(0, ad.width - 2 * edge), height: max(0, ad.height - edge))]
    guard let visible = region(&rects), let clicks = region(&events) else { return false }
    defer { _ = releaseRegion?(visible); _ = releaseRegion?(clicks) }
    return setShape(c, window, Float(ad.minX), Float(ad.minY), visible) == 0 && setEventShape(c, window, clicks) == 0
  }

  /// A plain solid window shown as `rect` (global top-left coordinates), or hidden when `rect` is empty.
  @discardableResult
  static func showWing(_ window: CGWindowID, _ rect: CGRect) -> Bool {
    guard let c = connection, let setShape else { return false }
    let r = rect.width >= 1 && rect.height >= 1 ? rect : CGRect(x: -20000, y: -20000, width: 1, height: 1)
    var rects = [CGRect(x: 0, y: 0, width: r.width, height: r.height)]
    guard let visible = region(&rects) else { return false }
    defer { _ = releaseRegion?(visible) }
    return setShape(c, window, Float(r.minX), Float(r.minY), visible) == 0
  }

  private static func region(_ rects: inout [CGRect]) -> OpaquePointer? {
    guard let newRegion else { return nil }
    var r: OpaquePointer?
    return newRegion(&rects, UInt32(rects.count), &r) == 0 ? r : nil
  }
}
