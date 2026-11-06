// KakaoAdHide - entry point: install / uninstall / run.
//
//   KakaoAdHide.exe              first run (from anywhere): install, then start KakaoTalk with the ads hidden
//                                 (when started from the install folder: just run)
//   KakaoAdHide.exe --run        what the Startup shortcut runs at sign-in (starts KakaoTalk to the tray)
//   KakaoAdHide.exe --uninstall  remove everything and give KakaoTalk its own auto start back
//   --quiet                      no message boxes (with install / uninstall)
//
// Install = copy to %LOCALAPPDATA%\KakaoAdHide, a shortcut in the user's Startup folder, an entry in
// "Installed apps", and KakaoTalk's own auto start switched off (KakaoAdHide starts KakaoTalk instead, so the
// hook is in place before KakaoTalk opens its first window). No admin rights needed; nothing outside the
// current user's profile is touched.

using System;
using System.Diagnostics;
using System.IO;
using System.Reflection;
using System.Runtime.InteropServices;
using System.Threading;
using Microsoft.Win32;

[assembly: AssemblyTitle("KakaoAdHide")]
[assembly: AssemblyProduct("KakaoAdHide")]
[assembly: AssemblyDescription("Hides the ad banner of KakaoTalk for Windows")]
[assembly: AssemblyVersion("1.0.0.0")]
[assembly: AssemblyFileVersion("1.0.0.0")]
[assembly: AssemblyInformationalVersion("1.0.0")]

static class Program {
  const string AppName = "KakaoAdHide";
  const string Version = "1.0.0";
  const string MutexName = @"Local\KakaoAdHide";
  const string QuitEventName = @"Local\KakaoAdHide.Quit";
  const string RunKey = @"Software\Microsoft\Windows\CurrentVersion\Run";
  const string AppKey = @"Software\KakaoAdHide";
  const string UninstallKey = @"Software\Microsoft\Windows\CurrentVersion\Uninstall\KakaoAdHide";
  const string KakaoRunValue = "KakaoTalk";                // KakaoTalk's own auto start entry

  static readonly string InstallDir = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), AppName);
  static readonly string InstalledExe = Path.Combine(InstallDir, AppName + ".exe");
  static readonly string StartupLnk = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.Startup), AppName + ".lnk");

  [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int MessageBoxW(IntPtr h, string text, string caption, uint type);
  [DllImport("user32.dll")] static extern IntPtr SetProcessDpiAwarenessContext(IntPtr v);

  static bool quiet;

  [STAThread]
  static int Main(string[] args) {
    try { SetProcessDpiAwarenessContext((IntPtr)(-4)); } catch { }   // per-monitor v2: real pixels for rects and GetPixel

    bool run = false, uninstall = false;
    foreach (var a in args) {
      string s = a.ToLowerInvariant();
      if (s == "--run") run = true;
      else if (s == "--uninstall") uninstall = true;
      else if (s == "--quiet") quiet = true;
    }

    try {
      if (uninstall) { Uninstall(); return 0; }
      if (run) { RunHider(true); return 0; }
      if (SamePath(SelfPath(), InstalledExe)) { RunHider(false); return 0; }
      Install();
      return 0;
    } catch (Exception e) {
      Message("오류가 발생했습니다.\n\n" + e.Message, 0x10);
      return 1;
    }
  }

  // ---------------------------------------------------------------- run

  static void RunHider(bool atSignIn) {
    bool created;
    using (var mutex = new Mutex(false, MutexName, out created)) {
      bool owned;
      try { owned = mutex.WaitOne(0); } catch (AbandonedMutexException) { owned = true; }
      if (!owned) return;   // already running

      using (var quit = new EventWaitHandle(false, EventResetMode.ManualReset, QuitEventName)) {
        quit.Reset();
        DisableKakaoAutoStart();   // KakaoTalk re-adds it after updates or when its setting is ticked again
        Hider.Init();

        Process first = null;
        if (Process.GetProcessesByName("KakaoTalk").Length == 0) {
          string kakao = FindKakao();
          if (kakao != null) {
            try { first = Process.Start(new ProcessStartInfo(kakao, atSignIn ? "-bystartup" : "") { UseShellExecute = false }); } catch { }
          }
        }

        while (true) {
          // wait up to 30s for KakaoTalk (startup, or restart after an update)
          Process p = first; first = null;
          for (int i = 0; i < 30 && p == null; i++) {
            var list = Process.GetProcessesByName("KakaoTalk");
            if (list.Length > 0) p = list[0];
            else if (quit.WaitOne(1000)) return;
          }
          if (p == null) return;   // KakaoTalk is gone -> exit

          if (Hider.Run((uint)p.Id, quit)) return;   // returns when KakaoTalk exits (false) or on quit (true)
          if (quit.WaitOne(3000)) return;
        }
      }
    }
  }

  // Ask a running instance to stop and wait until it has.
  static void StopRunning() {
    try {
      EventWaitHandle ev;
      if (EventWaitHandle.TryOpenExisting(QuitEventName, out ev)) { ev.Set(); ev.Dispose(); }
    } catch { }
    try {
      using (var m = new Mutex(false, MutexName)) {
        bool got;
        try { got = m.WaitOne(5000); } catch (AbandonedMutexException) { got = true; }
        if (got) m.ReleaseMutex();
      }
    } catch { }
  }

  // ---------------------------------------------------------------- install / uninstall

  static void Install() {
    StopRunning();

    Directory.CreateDirectory(InstallDir);
    File.Copy(SelfPath(), InstalledExe, true);

    CreateShortcut(StartupLnk, InstalledExe, "--run", InstallDir, "KakaoTalk with the ad banner hidden");
    DisableKakaoAutoStart();

    using (var k = Registry.CurrentUser.CreateSubKey(UninstallKey)) {
      k.SetValue("DisplayName", "KakaoAdHide (카카오톡 PC 광고 숨김)");
      k.SetValue("DisplayVersion", Version);
      k.SetValue("Publisher", "KakaoAdHide");
      k.SetValue("DisplayIcon", InstalledExe);
      k.SetValue("InstallLocation", InstallDir);
      k.SetValue("UninstallString", "\"" + InstalledExe + "\" --uninstall");
      k.SetValue("QuietUninstallString", "\"" + InstalledExe + "\" --uninstall --quiet");
      k.SetValue("NoModify", 1, RegistryValueKind.DWord);
      k.SetValue("NoRepair", 1, RegistryValueKind.DWord);
      k.SetValue("EstimatedSize", (int)(new FileInfo(InstalledExe).Length / 1024), RegistryValueKind.DWord);
    }

    Message("설치했습니다.\n\n" +
            "이제 로그인할 때 KakaoAdHide가 카카오톡을 켜고 광고를 숨깁니다.\n" +
            "(카카오톡 자체 자동 실행은 꺼 두었습니다)\n\n" +
            "제거: 설정 > 앱 > 설치된 앱 > KakaoAdHide", 0x40);

    Process.Start(new ProcessStartInfo(InstalledExe) { UseShellExecute = false, WorkingDirectory = InstallDir });
  }

  static void Uninstall() {
    StopRunning();

    TryDelete(StartupLnk);
    RestoreKakaoAutoStart();
    try { Registry.CurrentUser.DeleteSubKeyTree(UninstallKey, false); } catch { }
    try { Registry.CurrentUser.DeleteSubKeyTree(AppKey, false); } catch { }

    Message("제거했습니다.\n카카오톡 자동 실행은 원래대로 돌려 두었습니다.", 0x40);

    // The exe can't delete itself while running: let cmd do it after we exit. rmdir without /s only
    // removes the folder if it is empty, so nothing else that happens to be in there is touched.
    if (File.Exists(InstalledExe)) {
      string cmd = "/c ping 127.0.0.1 -n 3 >nul & del /f /q \"" + InstalledExe + "\" & rmdir \"" + InstallDir + "\"";
      Process.Start(new ProcessStartInfo("cmd.exe", cmd) { CreateNoWindow = true, UseShellExecute = false, WindowStyle = ProcessWindowStyle.Hidden });
    }
  }

  // ---------------------------------------------------------------- KakaoTalk's own auto start

  static void DisableKakaoAutoStart() {
    try {
      using (var run = Registry.CurrentUser.OpenSubKey(RunKey, true)) {
        if (run == null) return;
        var v = run.GetValue(KakaoRunValue) as string;
        if (v == null) return;
        using (var app = Registry.CurrentUser.CreateSubKey(AppKey)) {
          if (app.GetValue("KakaoRun") == null) app.SetValue("KakaoRun", v);   // keep the original for uninstall
          string exe = ExeFromCommandLine(v);
          if (exe != null && File.Exists(exe)) app.SetValue("KakaoPath", exe);
        }
        run.DeleteValue(KakaoRunValue, false);
      }
    } catch { }
  }

  static void RestoreKakaoAutoStart() {
    try {
      string v;
      using (var app = Registry.CurrentUser.OpenSubKey(AppKey)) { v = app == null ? null : app.GetValue("KakaoRun") as string; }
      if (v == null) return;
      using (var run = Registry.CurrentUser.CreateSubKey(RunKey)) {
        if (run.GetValue(KakaoRunValue) == null) run.SetValue(KakaoRunValue, v);
      }
    } catch { }
  }

  static string FindKakao() {
    var candidates = new System.Collections.Generic.List<string>();
    try {
      using (var app = Registry.CurrentUser.OpenSubKey(AppKey)) {
        if (app != null) candidates.Add(app.GetValue("KakaoPath") as string);
      }
    } catch { }
    try {
      using (var run = Registry.CurrentUser.OpenSubKey(RunKey)) {
        if (run != null) candidates.Add(ExeFromCommandLine(run.GetValue(KakaoRunValue) as string));
      }
    } catch { }
    candidates.Add(Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ProgramFiles), @"Kakao\KakaoTalk\KakaoTalk.exe"));
    candidates.Add(Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ProgramFilesX86), @"Kakao\KakaoTalk\KakaoTalk.exe"));
    candidates.Add(Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), @"Kakao\KakaoTalk\KakaoTalk.exe"));
    foreach (var c in candidates)
      if (!string.IsNullOrEmpty(c) && File.Exists(c)) return c;
    return null;
  }

  static string ExeFromCommandLine(string cmd) {
    if (string.IsNullOrEmpty(cmd)) return null;
    cmd = cmd.Trim();
    if (cmd.StartsWith("\"")) {
      int end = cmd.IndexOf('"', 1);
      return end > 1 ? cmd.Substring(1, end - 1) : null;
    }
    int exe = cmd.IndexOf(".exe", StringComparison.OrdinalIgnoreCase);
    return exe > 0 ? cmd.Substring(0, exe + 4) : null;
  }

  // ---------------------------------------------------------------- shortcuts

  static object Shell() { return Activator.CreateInstance(Type.GetTypeFromProgID("WScript.Shell")); }
  static void Set(object o, string name, object v) { o.GetType().InvokeMember(name, BindingFlags.SetProperty, null, o, new[] { v }); }
  static object Call(object o, string name, params object[] a) { return o.GetType().InvokeMember(name, BindingFlags.InvokeMethod, null, o, a); }

  static void CreateShortcut(string lnk, string target, string arguments, string workDir, string description) {
    object sh = Shell();
    object s = Call(sh, "CreateShortcut", lnk);
    Set(s, "TargetPath", target);
    Set(s, "Arguments", arguments);
    Set(s, "WorkingDirectory", workDir);
    Set(s, "Description", description);
    Call(s, "Save");
  }

  // ---------------------------------------------------------------- helpers

  static string SelfPath() { return Process.GetCurrentProcess().MainModule.FileName; }

  static bool SamePath(string a, string b) {
    try { return string.Equals(Path.GetFullPath(a), Path.GetFullPath(b), StringComparison.OrdinalIgnoreCase); }
    catch { return false; }
  }

  static void TryDelete(string f) { try { if (File.Exists(f)) File.Delete(f); } catch { } }

  static void Message(string text, uint icon) {
    if (!quiet) MessageBoxW(IntPtr.Zero, text, "KakaoAdHide", icon);
  }
}
