// KakaoAdHide - hides the ad banner of KakaoTalk for Windows.
//
// What it does, only through ordinary window APIs (no file patching, no code injection, no network changes):
//   1) The ad popup (a top-level window owned by the main window, holding the AdFit web view) is made
//      transparent, click-through, given an empty region and hidden - from the moment it is created.
//   2) The empty slot left in the main window (an untitled EVA_ChildWindow, 80-200px tall) is hidden.
//   3) OnlineMainView (friends / chats / more) is stretched down to the bottom of the window.
//
// It is event driven (SetWinEventHook, out of context): it wakes only when KakaoTalk's windows change,
// plus a light 1s safety check.

using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;

static class Hider {
  delegate bool EnumProc(IntPtr h, IntPtr l);
  delegate void WinEventProc(IntPtr hook, uint ev, IntPtr hwnd, int idObject, int idChild, uint thread, uint time);

  [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc f, IntPtr l);
  [DllImport("user32.dll")] static extern bool EnumChildWindows(IntPtr p, EnumProc f, IntPtr l);
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetClassName(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] static extern IntPtr GetParent(IntPtr h);
  [DllImport("user32.dll")] static extern IntPtr GetWindow(IntPtr h, uint cmd);
  [DllImport("user32.dll")] static extern int GetWindowLong(IntPtr h, int i);
  [DllImport("user32.dll")] static extern int SetWindowLong(IntPtr h, int i, int v);
  [DllImport("user32.dll")] static extern bool SetLayeredWindowAttributes(IntPtr h, uint key, byte alpha, uint flags);
  [DllImport("user32.dll")] static extern bool ShowWindowAsync(IntPtr h, int cmd);
  [DllImport("user32.dll")] static extern bool GetClientRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] static extern int MapWindowPoints(IntPtr from, IntPtr to, ref RECT r, int n);
  [DllImport("user32.dll")] static extern bool SetWindowPos(IntPtr h, IntPtr a, int x, int y, int cx, int cy, uint f);
  [DllImport("user32.dll")] static extern int SetWindowRgn(IntPtr h, IntPtr rgn, bool redraw);
  [DllImport("user32.dll")] static extern int GetWindowRgn(IntPtr h, IntPtr rgn);
  [DllImport("user32.dll")] static extern short GetAsyncKeyState(int vk);
  [DllImport("gdi32.dll")] static extern IntPtr CreateRectRgn(int l, int t, int r, int b);
  [DllImport("gdi32.dll")] static extern bool DeleteObject(IntPtr o);

  [DllImport("user32.dll")] static extern IntPtr SetWinEventHook(uint min, uint max, IntPtr hmod, WinEventProc proc, uint pid, uint tid, uint flags);
  [DllImport("user32.dll")] static extern bool UnhookWinEvent(IntPtr h);
  [DllImport("user32.dll")] static extern bool PeekMessage(out MSG m, IntPtr h, uint min, uint max, uint remove);
  [DllImport("user32.dll")] static extern uint MsgWaitForMultipleObjects(uint n, IntPtr[] handles, bool all, uint ms, uint mask);
  [DllImport("user32.dll")] static extern bool TranslateMessage(ref MSG m);
  [DllImport("user32.dll")] static extern IntPtr DispatchMessage(ref MSG m);

  // cover window (our own process)
  delegate IntPtr WndProcD(IntPtr h, uint m, IntPtr w, IntPtr l);
  [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)] struct WNDCLASS {
    public uint style; public WndProcD lpfnWndProc; public int cbClsExtra, cbWndExtra;
    public IntPtr hInstance, hIcon, hCursor, hbrBackground; public string lpszMenuName, lpszClassName; }
  [StructLayout(LayoutKind.Sequential)] struct PAINTSTRUCT {
    public IntPtr hdc; public int fErase; public RECT rcPaint; public int fRestore, fIncUpdate;
    [MarshalAs(UnmanagedType.ByValArray, SizeConst = 32)] public byte[] rgbReserved; }
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern ushort RegisterClassW(ref WNDCLASS c);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern IntPtr CreateWindowExW(int ex, string cls, string name, uint style, int x, int y, int w, int h, IntPtr parent, IntPtr menu, IntPtr inst, IntPtr param);
  [DllImport("user32.dll")] static extern IntPtr DefWindowProcW(IntPtr h, uint m, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] static extern bool IsWindow(IntPtr h);
  [DllImport("user32.dll")] static extern bool InvalidateRect(IntPtr h, IntPtr r, bool erase);
  [DllImport("user32.dll")] static extern IntPtr BeginPaint(IntPtr h, out PAINTSTRUCT p);
  [DllImport("user32.dll")] static extern bool EndPaint(IntPtr h, ref PAINTSTRUCT p);
  [DllImport("user32.dll")] static extern int FillRect(IntPtr dc, ref RECT r, IntPtr br);
  [DllImport("user32.dll")] static extern IntPtr GetDC(IntPtr h);
  [DllImport("user32.dll")] static extern int ReleaseDC(IntPtr h, IntPtr dc);
  [DllImport("gdi32.dll")] static extern uint GetPixel(IntPtr dc, int x, int y);
  [DllImport("gdi32.dll")] static extern IntPtr CreateSolidBrush(uint c);
  [DllImport("kernel32.dll", CharSet = CharSet.Unicode)] static extern IntPtr GetModuleHandleW(string n);

  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  [StructLayout(LayoutKind.Sequential)] public struct MSG { public IntPtr hwnd; public uint message; public IntPtr wParam; public IntPtr lParam; public uint time; public int ptX, ptY; }

  static string Cls(IntPtr h) { var s = new StringBuilder(256); GetClassName(h, s, 256); return s.ToString(); }
  static string Txt(IntPtr h) { var s = new StringBuilder(256); GetWindowText(h, s, 256); return s.ToString(); }
  static List<IntPtr> Kids(IntPtr p) { var l = new List<IntPtr>(); EnumChildWindows(p, (h, x) => { l.Add(h); return true; }, IntPtr.Zero); return l; }

  // SetWindowRgn / SetWindowLong / SetWindowPos on another process's window wait for that window's UI
  // thread. At startup KakaoTalk's UI thread is busy for up to ~1s, and a blocked call would stall the
  // whole loop, so these calls run on short-lived threads; the main loop never waits on KakaoTalk.
  // At most one call per key is in flight; a request arriving meanwhile makes it run once more afterwards
  // (the action reads the current state itself), so nothing ever piles up in KakaoTalk's message queue.
  static HashSet<string> inflight = new HashSet<string>(), again = new HashSet<string>();
  static void Async(string key, ThreadStart a) {
    lock (inflight) { if (!inflight.Add(key)) { again.Add(key); return; } }
    var t = new Thread(() => {
      while (true) {
        try { a(); } catch { }
        lock (inflight) { if (!again.Remove(key)) { inflight.Remove(key); return; } }
      }
    });
    t.IsBackground = true;
    t.Start();
  }

  // Stretch OnlineMainView to the bottom of the main window's client area. Synchronous SetWindowPos (run
  // via Async): a sent message is handled before KakaoTalk's posted messages and input, so it lands within
  // the same resize step. (SWP_ASYNCWINDOWPOS requests queued up during a resize drag and pushed back the
  // mouse input, so the window lagged behind the cursor.)
  static bool StretchNeeded(IntPtr top, IntPtr view, out int w, out int target) {
    RECT c; GetClientRect(top, out c);
    RECT v; GetWindowRect(view, out v); MapWindowPoints(IntPtr.Zero, top, ref v, 2);
    w = v.R - v.L; target = c.B - v.T;
    return target > 0 && v.B - v.T != target;
  }
  static void Stretch(IntPtr top, IntPtr view) {
    int w, target;
    if (StretchNeeded(top, view, out w, out target))
      SetWindowPos(view, IntPtr.Zero, 0, 0, w, target, 0x0016);   // NOMOVE|NOZORDER|NOACTIVATE
  }

  // Give the window an empty region: it can never paint (or take clicks), even if KakaoTalk shows it again
  static void Erase(IntPtr h) {
    IntPtr tmp = CreateRectRgn(0, 0, 0, 0);
    int kind = GetWindowRgn(h, tmp);   // 1 = NULLREGION -> already erased
    DeleteObject(tmp);
    if (kind != 1) Async("rgn" + h, () => SetWindowRgn(h, CreateRectRgn(0, 0, 0, 0), true));   // the system owns the region after this
  }

  const string MainTitle = "카카오톡";   // "KakaoTalk" in Korean
  static HashSet<IntPtr> tentative = new HashSet<IntPtr>(); // slots erased at 0x0 before their final size is known

  static void Hide(IntPtr h) { Erase(h); if (IsWindowVisible(h)) ShowWindowAsync(h, 0); }

  // Top-level ad popup: also make it fully transparent and click-through. KakaoTalk calls SetWindowRgn itself
  // when it sizes the popup (replacing our empty region), but it never touches the extended style.
  static void HideTop(IntPtr h) {
    int ex = GetWindowLong(h, -20);   // GWL_EXSTYLE
    if ((ex & 0x80020) != 0x80020) {  // WS_EX_LAYERED | WS_EX_TRANSPARENT
      Async("ex" + h, () => {
        SetWindowLong(h, -20, GetWindowLong(h, -20) | 0x80020);
        SetLayeredWindowAttributes(h, 0, 0, 2);   // LWA_ALPHA, alpha 0
      });
      Erase(h);
    }
    // Once it is transparent, don't fight KakaoTalk over the region: it resets it whenever it repositions
    // the popup (every step of a window drag), and re-erasing each time made KakaoTalk do extra work.
    if (IsWindowVisible(h)) ShowWindowAsync(h, 0);
    Cover(h);
  }

  // ---- Cover window ----
  // At startup KakaoTalk creates, sizes, shows and paints the ad popup in one stretch without pumping
  // messages, so none of the changes above can land until ~150-200ms later and the built-in default
  // banner is on screen meanwhile. Nothing from outside the process can change that window sooner
  // (even DWM cloaking is refused), so for that short time we put our own click-through window over
  // it, painted with the colours just above the ad (left nav bar + list background), and remove it
  // once KakaoTalk has applied the hide.
  static IntPtr ov, ovTarget;
  static WndProcD ovProc;
  static int ovStart, ovDone, navW;
  static uint colL = 0xFFFFFF, colR = 0xFFFFFF;
  static RECT ovRect;

  static bool Applied(IntPtr h) {
    if (!IsWindowVisible(h) || (GetWindowLong(h, -20) & 0x80000) != 0) return true;
    IntPtr tmp = CreateRectRgn(0, 0, 0, 0); int kind = GetWindowRgn(h, tmp); DeleteObject(tmp);
    return kind == 1;
  }

  static void Cover(IntPtr h) {
    if (ov == IntPtr.Zero || ovTarget != IntPtr.Zero || Applied(h)) return;
    ovTarget = h; ovStart = Environment.TickCount; ovDone = 0; ovRect = new RECT();
    UpdateCover();
  }

  static void CreateCover() {
    ovProc = (h, m, w, l) => {
      if (m == 0x000F) {             // WM_PAINT
        PAINTSTRUCT ps; IntPtr dc = BeginPaint(h, out ps);
        RECT c; GetClientRect(h, out c);
        int split = Math.Min(Math.Max(navW, 0), c.R);
        RECT a = c; a.R = split; RECT b = c; b.L = split;
        IntPtr bl = CreateSolidBrush(colL), br = CreateSolidBrush(colR);
        if (a.R > a.L) FillRect(dc, ref a, bl);
        FillRect(dc, ref b, br);
        DeleteObject(bl); DeleteObject(br);
        EndPaint(h, ref ps);
        return IntPtr.Zero;
      }
      if (m == 0x0014) return (IntPtr)1;   // WM_ERASEBKGND: painted in WM_PAINT
      if (m == 0x0021) return (IntPtr)3;   // WM_MOUSEACTIVATE: MA_NOACTIVATE
      return DefWindowProcW(h, m, w, l);
    };
    var wc = new WNDCLASS { lpfnWndProc = ovProc, hInstance = GetModuleHandleW(null), lpszClassName = "KakaoAdHideCover" };
    RegisterClassW(ref wc);
    // TOPMOST | TOOLWINDOW | LAYERED | TRANSPARENT | NOACTIVATE ; WS_POPUP
    ov = CreateWindowExW(0x08 | 0x80 | 0x80000 | 0x20 | 0x08000000, "KakaoAdHideCover", "", 0x80000000,
                         0, 0, 0, 0, IntPtr.Zero, IntPtr.Zero, wc.hInstance, IntPtr.Zero);
    if (ov != IntPtr.Zero) SetLayeredWindowAttributes(ov, 0, 255, 2);   // opaque, but click-through
  }

  static uint Sample(IntPtr dc, int x, int y, uint fallback) {
    uint c = GetPixel(dc, x, y);
    return c == 0xFFFFFFFF ? fallback : c;   // CLR_INVALID (off screen)
  }

  static void UpdateCover() {
    if (ovTarget == IntPtr.Zero) return;
    int now = Environment.TickCount;
    if (ovDone == 0 && (!IsWindow(ovTarget) || Applied(ovTarget))) ovDone = now | 1;
    // keep it up a little longer after the hide lands, while KakaoTalk repaints the area; never > 3s
    if ((ovDone != 0 && unchecked(now - ovDone) >= 150) || unchecked(now - ovStart) > 3000) {
      SetWindowPos(ov, IntPtr.Zero, 0, 0, 0, 0, 0x0080 | 0x0013);   // HIDEWINDOW | NOSIZE|NOMOVE|NOACTIVATE
      ovTarget = IntPtr.Zero;
      return;
    }
    RECT r; GetWindowRect(ovTarget, out r);
    if (r.R - r.L <= 0 || r.B - r.T <= 0) return;
    IntPtr dc = GetDC(IntPtr.Zero);
    uint l = Sample(dc, r.L + 8, r.T - 3, colL), rr = Sample(dc, r.R - 30, r.T - 3, colR);
    ReleaseDC(IntPtr.Zero, dc);
    bool moved = r.L != ovRect.L || r.T != ovRect.T || r.R != ovRect.R || r.B != ovRect.B;
    if (moved || l != colL || rr != colR) {
      colL = l; colR = rr; ovRect = r;
      SetWindowPos(ov, (IntPtr)(-1), r.L, r.T, r.R - r.L, r.B - r.T, 0x0010 | 0x0040);   // TOPMOST; NOACTIVATE|SHOWWINDOW
      InvalidateRect(ov, IntPtr.Zero, false);
    }
  }

  // 1) Ad popup. Returns true if 'top' is one (and hides it).
  static bool CheckAd(IntPtr top, List<IntPtr> kids) {
    // a) top-level window that contains the AdFit web view
    foreach (var k in kids)
      if (Txt(k).Contains("AdFit")) { HideTop(top); return true; }

    // b) the same popup recognised by shape, from the moment it is created - before the AdFit child
    //    arrives (that one comes from the WebView2 process, so the per-pid hook never sees it).
    //    KakaoTalk paints its built-in default banner here in the meantime: untitled EVA_Window_Dblclk
    //    popup, owned by the main window, holding a Chrome_WidgetWin_0 host, banner-sized or still 0x0.
    if (Txt(top) != "" || Cls(top) != "EVA_Window_Dblclk") return false;
    IntPtr owner = GetWindow(top, 4);   // GW_OWNER
    if (owner == IntPtr.Zero || Txt(owner) != MainTitle) return false;
    RECT tr; GetWindowRect(top, out tr);
    if (tr.B - tr.T > 200) return false;
    foreach (var k in kids)
      if (Cls(k) == "Chrome_WidgetWin_0") { HideTop(top); return true; }
    return false;
  }

  static void Tick(uint pid) {
    var tops = new List<IntPtr>();
    EnumWindows((h, x) => { uint p; GetWindowThreadProcessId(h, out p); if (p == pid) tops.Add(h); return true; }, IntPtr.Zero);
    foreach (var top in tops) {
      var kids = Kids(top);
      if (CheckAd(top, kids)) continue;
      if (Txt(top) == MainTitle) CheckMain(top, kids);
    }
  }

  // Fast path for one event: look only at the window the event is about (and its main window)
  static void Quick(IntPtr h) {
    IntPtr parent = GetParent(h);
    if (GetWindow(h, 4) != IntPtr.Zero && parent == GetWindow(h, 4)) {   // top-level owned popup
      CheckAd(h, Kids(h));
    } else if (parent != IntPtr.Zero && Txt(parent) == MainTitle && Cls(h) == "EVA_ChildWindow") {
      CheckMain(parent, Kids(parent));
    }
  }

  // 2) Main window: hide the empty slot, stretch OnlineMainView
  static void CheckMain(IntPtr top, List<IntPtr> kids) {
    IntPtr view = IntPtr.Zero;
    foreach (var k in kids)
      if (GetParent(k) == top && Txt(k).StartsWith("OnlineMainView")) { view = k; break; }
    if (view == IntPtr.Zero) return;   // only once the real views have their names, so an unnamed one is never mistaken

    foreach (var k in kids) {
      if (k == view || GetParent(k) != top || Cls(k) != "EVA_ChildWindow" || Txt(k) != "") continue;
      RECT r; GetWindowRect(k, out r); int hgt = r.B - r.T;
      if (hgt >= 80 && hgt <= 200) { Hide(k); tentative.Remove(k); }
      else if (hgt == 0 && !IsWindowVisible(k)) {
        // freshly created slot: blank it now so nothing paints before it gets its size
        if (tentative.Add(k)) Erase(k);
      }
      else if (tentative.Remove(k)) { IntPtr w = k; Async("rgn" + w, () => SetWindowRgn(w, IntPtr.Zero, true)); }   // turned out not to be the slot: restore
    }

    // width of the left nav bar (for the cover window): offset of the visible list view inside OnlineMainView
    RECT vr; GetWindowRect(view, out vr);
    foreach (var k in kids)
      if (GetParent(k) == view && IsWindowVisible(k)) {
        RECT kr; GetWindowRect(k, out kr);
        if (kr.R - kr.L > 0 && kr.L > vr.L) { navW = kr.L - vr.L; break; }
      }

    // Not while the user drags/resizes the window: KakaoTalk re-lays out (and shrinks the view) on every
    // step, and stretching it back each time doubled its work so the window trailed the cursor.
    // The view is stretched once when the drag ends (EVENT_SYSTEM_MOVESIZEEND).
    int sw, target;
    if (!dragging && StretchNeeded(top, view, out sw, out target)) { IntPtr t = top, vw = view; Async("pos" + vw, () => Stretch(t, vw)); }
  }

  static WinEventProc cb;      // kept in a static field so the GC never collects it
  static bool pending, dragging;

  // Call once, before KakaoTalk starts, on the thread that will call Run.
  public static void Init() {
    try { CreateCover(); } catch { }
  }

  // Blocks until the KakaoTalk process exits (returns false) or 'quit' is signalled (returns true).
  public static bool Run(uint pid, WaitHandle quit) {
    Process proc;
    try { proc = Process.GetProcessById((int)pid); } catch { return false; }
    IntPtr[] handles = { quit.SafeWaitHandle.DangerousGetHandle() };

    // Events arrive in bursts (KakaoTalk creates dozens of windows at startup). A full scan per event would
    // fall behind the queue, so: handle the event's own window at once (cheap), mark a full scan as
    // pending, and run that scan once after the queue has been drained.
    cb = (hook, ev, hwnd, idObject, idChild, thread, time) => {
      if (idObject != 0 || hwnd == IntPtr.Zero) return;   // OBJID_WINDOW only (ignore caret, scrollbars...)
      if (ev == 0x000A) { dragging = true; return; }      // EVENT_SYSTEM_MOVESIZESTART
      if (ev == 0x000B) dragging = false;                  // EVENT_SYSTEM_MOVESIZEEND -> full scan stretches the view
      if (ev == 0x8000 || ev == 0x8002) { try { Quick(hwnd); } catch { } }   // CREATE / SHOW
      pending = true;
    };

    // EVENT_OBJECT_CREATE .. EVENT_OBJECT_NAMECHANGE, and MOVESIZESTART / MOVESIZEEND; WINEVENT_OUTOFCONTEXT
    IntPtr h1 = SetWinEventHook(0x8000, 0x800C, IntPtr.Zero, cb, pid, 0, 0);
    IntPtr h2 = SetWinEventHook(0x000A, 0x000B, IntPtr.Zero, cb, pid, 0, 0);

    bool quitting = false;
    pending = true;
    int lastFull = Environment.TickCount;
    MSG m;
    while (true) {
      uint wait = MsgWaitForMultipleObjects(1, handles, false, pending ? 0u : (ovTarget != IntPtr.Zero ? 15u : 1000u), 0x04FF);   // QS_ALLINPUT
      if (wait == 0) { quitting = true; break; }                               // quit event
      while (PeekMessage(out m, IntPtr.Zero, 0, 0, 1)) {                       // PM_REMOVE; WinEvent callbacks run in here
        if (m.message == 0x0012) { quitting = true; goto done; }               // WM_QUIT
        TranslateMessage(ref m); DispatchMessage(ref m);
      }
      bool second = unchecked(Environment.TickCount - lastFull) >= 1000;
      if (second) {                                    // light safety check, and exit when KakaoTalk is gone
        proc.Refresh();
        if (proc.HasExited) break;
        if (dragging && (GetAsyncKeyState(0x01) & 0x8000) == 0) { dragging = false; pending = true; }   // missed MOVESIZEEND
      }
      if (pending || second) {
        pending = false;
        try { Tick(pid); } catch { }
        lastFull = Environment.TickCount;
      }
      try { UpdateCover(); } catch { }
    }
    done:
    UnhookWinEvent(h1); UnhookWinEvent(h2);
    return quitting;
  }
}
