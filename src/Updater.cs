// KakaoAdHide - update check and one-click update.
//
// Once a day (first time a minute after start) it reads latest-windows.json from the latest GitHub release. When
// there is a newer version it shows a tray icon and a notification; nothing is downloaded until the user
// clicks it. Then the new exe is downloaded and checked before it is run:
//   - the download link must point into this repository's releases,
//   - size and SHA-256 must match latest-windows.json,
//   - latest-windows.json's signature over "KakaoAdHide\n<version>\n<sha256>" must verify with one of the public keys
//     built into this exe (src/UpdateKeys.cs).
// The verified exe is started with --quiet: it installs itself over this copy (stopping it first) and runs.
// If verification fails (or no keys are built in), nothing is run and the user is pointed to the release page.
//
// Turn the check off: HKCU\Software\KakaoAdHide, DWORD UpdateCheck = 0.

using System;
using System.Diagnostics;
using System.Drawing;
using System.IO;
using System.Net;
using System.Runtime.Serialization;
using System.Runtime.Serialization.Json;
using System.Security.Cryptography;
using System.Text;
using System.Threading;
using System.Windows.Forms;
using Microsoft.Win32;

#pragma warning disable 0649   // the fields are filled in by the JSON serializer
[DataContract]
class UpdateFeed {
  [DataMember(Name = "version")] public string Version;
  [DataMember(Name = "url")] public string Url;
  [DataMember(Name = "size")] public long Size;
  [DataMember(Name = "sha256")] public string Sha256;
  [DataMember(Name = "signature")] public string Signature;
  [DataMember(Name = "page")] public string Page;
}
#pragma warning restore 0649

class UpdateVerifyException : Exception {
  public UpdateVerifyException(string message) : base(message) { }
}

static class Updater {
  const string Repo = "pandemoniummm/KakaoAdHide";
  const string AppKey = @"Software\KakaoAdHide";
  public const string ReleasesPage = "https://github.com/" + Repo + "/releases/latest";
  const int MaxExeBytes = 20 * 1024 * 1024;
  const int IntervalMs = 24 * 60 * 60 * 1000;

#if TEST_FEED
  // test builds only (build.ps1 -Define TEST_FEED): local feed and keys from environment variables
  static string FeedUrl { get { return Environment.GetEnvironmentVariable("KAKAOADHIDE_TEST_FEED"); } }
  const string AssetPrefix = "http://127.0.0.1:";
  public const int FirstCheckMs = 3000;
  static string[] Keys {
    get {
      string f = Environment.GetEnvironmentVariable("KAKAOADHIDE_TEST_KEYS");
      return f == null ? new string[0] : File.ReadAllLines(f);
    }
  }
#else
  const string FeedUrl = "https://github.com/" + Repo + "/releases/latest/download/latest-windows.json";
  const string AssetPrefix = "https://github.com/" + Repo + "/releases/download/";
  public const int FirstCheckMs = 60 * 1000;
  static string[] Keys { get { return UpdateKeys.Public; } }
#endif

  public static readonly string TempDir = Path.Combine(Path.GetTempPath(), "KakaoAdHide-update");

  static Updater() {
    try { ServicePointManager.SecurityProtocol |= (SecurityProtocolType)3072; } catch { }   // TLS 1.2
  }

  // ------------------------------------------------------------ settings (HKCU\Software\KakaoAdHide)

  static object GetSetting(string name) {
    try { using (var k = Registry.CurrentUser.OpenSubKey(AppKey)) { return k == null ? null : k.GetValue(name); } }
    catch { return null; }
  }
  static void SetSetting(string name, object value) {
    try { using (var k = Registry.CurrentUser.CreateSubKey(AppKey)) { k.SetValue(name, value); } } catch { }
  }

  public static bool Enabled {
    get { object v = GetSetting("UpdateCheck"); return !(v is int) || (int)v != 0; }
  }

  // The version this copy replaced, if it was just updated (read once at start).
  public static string UpdatedFrom;

  public static void NoteVersion() {
    string last = GetSetting("LastVersion") as string;
    if (last != null && last != BuildInfo.Version) UpdatedFrom = last;
    if (last != BuildInfo.Version) SetSetting("LastVersion", BuildInfo.Version);
  }

  // Remove the downloaded installer once it has done its job (it may still be exiting: retry a little).
  public static void CleanupTemp() {
    var t = new Thread(() => {
      for (int i = 0; i < 10 && Directory.Exists(TempDir); i++) {
        try { Directory.Delete(TempDir, true); } catch { Thread.Sleep(2000); }
      }
    });
    t.IsBackground = true;
    t.Start();
  }

  // ------------------------------------------------------------ check / download / verify

  static byte[] Download(string url, int limit) {
    var req = (HttpWebRequest)WebRequest.Create(url);
    req.UserAgent = "KakaoAdHide/" + BuildInfo.Version;
    req.Timeout = 30000;
    req.ReadWriteTimeout = 30000;
    req.AllowAutoRedirect = true;
    using (var resp = (HttpWebResponse)req.GetResponse())
    using (var s = resp.GetResponseStream())
    using (var ms = new MemoryStream()) {
      var buf = new byte[81920];
      int n;
      while ((n = s.Read(buf, 0, buf.Length)) > 0) {
        ms.Write(buf, 0, n);
        if (ms.Length > limit) throw new IOException("download too large");
      }
      return ms.ToArray();
    }
  }

  public static UpdateFeed ParseFeed(byte[] json) {
    int skip = json.Length >= 3 && json[0] == 0xEF && json[1] == 0xBB && json[2] == 0xBF ? 3 : 0;   // UTF-8 BOM
    using (var ms = new MemoryStream(json, skip, json.Length - skip)) {
      var f = (UpdateFeed)new DataContractJsonSerializer(typeof(UpdateFeed)).ReadObject(ms);
      Version v;
      if (f == null || f.Url == null || f.Sha256 == null || f.Signature == null || !System.Version.TryParse(f.Version ?? "", out v))
        throw new InvalidDataException("bad update feed");
      return f;
    }
  }

  public static UpdateFeed FetchFeed() {
    if (string.IsNullOrEmpty(FeedUrl)) throw new InvalidOperationException("no update feed");
    return ParseFeed(Download(FeedUrl, 64 * 1024));
  }

  public static bool IsNewer(UpdateFeed f) {
    return System.Version.Parse(f.Version) > System.Version.Parse(BuildInfo.Version);
  }

  static string Hex(byte[] b) {
    var sb = new StringBuilder(b.Length * 2);
    foreach (var x in b) sb.Append(x.ToString("x2"));
    return sb.ToString();
  }

  // Checks that 'exe' is exactly what the signed feed describes. Throws UpdateVerifyException otherwise.
  public static void Verify(UpdateFeed f, byte[] exe) {
    if (Keys.Length == 0) throw new UpdateVerifyException("no update keys built in");
    if (f.Size != 0 && f.Size != exe.Length) throw new UpdateVerifyException("size mismatch");
    string sha;
    using (var h = SHA256.Create()) sha = Hex(h.ComputeHash(exe));
    if (!string.Equals(sha, f.Sha256, StringComparison.OrdinalIgnoreCase)) throw new UpdateVerifyException("sha256 mismatch");

    byte[] message = Encoding.UTF8.GetBytes("KakaoAdHide\n" + f.Version + "\n" + sha);
    byte[] sig;
    try { sig = Convert.FromBase64String(f.Signature); } catch { throw new UpdateVerifyException("bad signature encoding"); }
    foreach (var key in Keys) {
      if (string.IsNullOrEmpty(key)) continue;
      try {
        using (var rsa = new RSACryptoServiceProvider()) {
          rsa.PersistKeyInCsp = false;
          rsa.FromXmlString(key);
          if (rsa.VerifyData(message, "SHA256", sig)) return;
        }
      } catch { }
    }
    throw new UpdateVerifyException("signature does not match");
  }

  // Downloads the update, verifies it and saves it to TempDir. Returns the saved path.
  public static string DownloadVerified(UpdateFeed f) {
    if (!f.Url.StartsWith(AssetPrefix, StringComparison.Ordinal)) throw new UpdateVerifyException("download link outside this repository");
    byte[] exe = Download(f.Url, MaxExeBytes);
    Verify(f, exe);
    Directory.CreateDirectory(TempDir);
    string path = Path.Combine(TempDir, "KakaoAdHide.exe");
    File.WriteAllBytes(path, exe);
    return path;
  }

  // The new exe installs itself over this copy (it stops the running one first) and starts it.
  public static void Launch(string path) {
    Process.Start(new ProcessStartInfo(path, "--quiet") { UseShellExecute = false, WorkingDirectory = TempDir });
  }

  public static void OpenPage(string url) {
    try { Process.Start(new ProcessStartInfo(url) { UseShellExecute = true }); } catch { }
  }

  // ------------------------------------------------------------ command line

  // KakaoAdHide.exe --update : check now; exit codes 0 = update started, 2 = up to date, 3 = not verified, 1 = error
  public static int UpdateNow(Action<string, uint> message, bool interactive) {
    UpdateFeed f;
    try { f = FetchFeed(); }
    catch (Exception e) { message("업데이트를 확인하지 못했습니다.\n\n" + e.Message, 0x30); return 1; }
    if (!IsNewer(f)) { message("최신 버전입니다. (" + BuildInfo.Version + ")", 0x40); return 2; }
    try {
      Launch(DownloadVerified(f));
      return 0;
    } catch (UpdateVerifyException e) {
      message("새 버전(" + f.Version + ")을 검증하지 못해 설치하지 않았습니다.\n" + e.Message +
              "\n\n릴리스 페이지에서 직접 받아 주세요.", 0x30);
      if (interactive) OpenPage(f.Page ?? ReleasesPage);
      return 3;
    } catch (Exception e) {
      message("업데이트를 받지 못했습니다.\n\n" + e.Message, 0x30);
      return 1;
    }
  }

  // KakaoAdHide.exe --verify-feed latest-windows.json KakaoAdHide-windows.exe : for the release workflow. The exe being
  // checked runs this itself, so it also proves the feed's version is the version built into the exe.
  public static int VerifyFiles(string feedPath, string exePath) {
    try {
      var f = ParseFeed(File.ReadAllBytes(feedPath));
      if (f.Version != BuildInfo.Version) return 4;
      Verify(f, File.ReadAllBytes(exePath));
      return 0;
    } catch (UpdateVerifyException) { return 3; }
    catch { return 1; }
  }

  // ------------------------------------------------------------ tray UI (own STA thread)

  static Control sync;
  static UpdateUi ui;

  public static void StartBackground() {
    var t = new Thread(() => {
      try {
        sync = new Control();
        sync.CreateControl();
        ui = new UpdateUi(Enabled);
        Application.Run(ui);
      } catch { }
    });
    t.IsBackground = true;
    t.SetApartmentState(ApartmentState.STA);
    t.Start();
  }

  // Take the tray icon away before the process exits (otherwise it lingers until the mouse passes over it).
  public static void Shutdown() {
    try {
      if (sync != null && sync.IsHandleCreated && ui != null)
        sync.Invoke((MethodInvoker)(() => ui.HideIcon()));
    } catch { }
  }

  public static string SkippedVersion {
    get { return GetSetting("SkippedVersion") as string; }
    set { SetSetting("SkippedVersion", value); }
  }
  public static string NotifiedVersion {
    get { return GetSetting("NotifiedVersion") as string; }
    set { SetSetting("NotifiedVersion", value); }
  }
}

class UpdateUi : ApplicationContext {
  enum Mode { None, Updated, Available, Downloading, Failed, Unverified }

  readonly System.Windows.Forms.Timer timer = new System.Windows.Forms.Timer();
  readonly bool checks;
  NotifyIcon icon;
  UpdateFeed pending;
  Mode mode = Mode.None;

  public UpdateUi(bool checks) {
    this.checks = checks;
    if (Updater.UpdatedFrom != null) {
      mode = Mode.Updated;
      Balloon("KakaoAdHide " + BuildInfo.Version, Updater.UpdatedFrom + "에서 " + BuildInfo.Version + "(으)로 업데이트했습니다.", ToolTipIcon.Info);
    }
    if (checks) {
      timer.Interval = Updater.FirstCheckMs;
      timer.Tick += (s, e) => { timer.Interval = 24 * 60 * 60 * 1000; Check(); };
      timer.Start();
    }
  }

  NotifyIcon Tray() {
    if (icon != null) return icon;
    icon = new NotifyIcon();
    try { icon.Icon = System.Drawing.Icon.ExtractAssociatedIcon(Application.ExecutablePath); } catch { icon.Icon = SystemIcons.Application; }
    icon.Text = "KakaoAdHide";
    icon.BalloonTipClicked += (s, e) => Clicked();
    icon.BalloonTipClosed += (s, e) => { if (mode == Mode.Updated) HideIcon(); };
    icon.MouseClick += (s, e) => { if (e.Button == MouseButtons.Left) Clicked(); };
    return icon;
  }

  public void HideIcon() {
    if (icon == null) return;
    icon.Visible = false;
    icon.Dispose();
    icon = null;
  }

  void Balloon(string title, string text, ToolTipIcon kind) {
    var i = Tray();
    i.Visible = true;
    i.ShowBalloonTip(10000, title, text, kind);
  }

  void Check() {
    if (mode == Mode.Downloading) return;
    UpdateFeed f;
    try { f = Updater.FetchFeed(); } catch { return; }   // offline etc.: try again tomorrow
    if (!Updater.IsNewer(f) || f.Version == Updater.SkippedVersion) return;
    pending = f;
    mode = Mode.Available;
    var i = Tray();
    i.Text = "KakaoAdHide " + f.Version + " 업데이트 있음";
    i.ContextMenuStrip = BuildMenu(f);
    i.Visible = true;
    if (Updater.NotifiedVersion != f.Version) {   // the notification once per version; the tray icon stays
      Updater.NotifiedVersion = f.Version;
      i.ShowBalloonTip(15000, "KakaoAdHide " + f.Version, "새 버전이 있습니다. 클릭하면 업데이트합니다.", ToolTipIcon.Info);
    }
  }

  ContextMenuStrip BuildMenu(UpdateFeed f) {
    var m = new ContextMenuStrip();
    m.Items.Add(f.Version + "(으)로 업데이트", null, (s, e) => Install());
    m.Items.Add("변경 내역 보기", null, (s, e) => Updater.OpenPage(f.Page ?? Updater.ReleasesPage));
    m.Items.Add("이 버전 건너뛰기", null, (s, e) => { Updater.SkippedVersion = f.Version; mode = Mode.None; HideIcon(); });
    m.Items.Add(new ToolStripSeparator());
    m.Items.Add("나중에", null, (s, e) => { mode = Mode.None; HideIcon(); });
    return m;
  }

  void Clicked() {
    switch (mode) {
      case Mode.Available:
      case Mode.Failed: Install(); break;
      case Mode.Unverified: Updater.OpenPage(pending != null && pending.Page != null ? pending.Page : Updater.ReleasesPage); break;
      case Mode.Updated: HideIcon(); mode = Mode.None; break;
    }
  }

  void Install() {
    if (pending == null || mode == Mode.Downloading) return;
    mode = Mode.Downloading;
    Tray().Text = "KakaoAdHide " + pending.Version + " 받는 중...";
    try {
      Updater.Launch(Updater.DownloadVerified(pending));   // the installer stops this process
    } catch (UpdateVerifyException) {
      mode = Mode.Unverified;
      Balloon("업데이트를 설치하지 않았습니다",
              "새 버전을 검증하지 못했습니다. 클릭하면 릴리스 페이지를 엽니다. 직접 받아서 실행해 주세요.", ToolTipIcon.Warning);
    } catch {
      mode = Mode.Failed;
      Tray().Text = "KakaoAdHide " + pending.Version + " 업데이트 있음";
      Balloon("업데이트를 받지 못했습니다", "인터넷 연결을 확인하고 다시 클릭해 주세요.", ToolTipIcon.Warning);
    }
  }
}
