//! macOS: TalkNap.app in /Applications (or ~/Applications), a menu bar item, a LaunchAgent to start at login,
//! settings in ~/Library/Application Support/TalkNap/settings.json.
//!
//! Up to 2.6.0 the app was KakaoAdHide.app (bundle id io.github.pandemoniummm.KakaoAdHide, data in
//! ~/Library/Application Support/KakaoAdHide). 2.6.0 updates by swapping it for TalkNap.app; `migrate_old` then
//! moves the rest over at each start. The OLD_* names are kept only to find and remove what the old app left.

use crate::certs::Paths;
use crate::feed::{self, Feed, VerifyError};
use crate::{hosts, keys, setup};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const NAME: &str = "macos";
pub const BUNDLE_ID: &str = "io.github.pandemoniummm.TalkNap";
pub const APP_NAME: &str = "TalkNap.app";
pub const UPDATE_ASSET: &str = "TalkNap.dmg";

/// SHA-1 of the self-signed "TalkNap Code Signing" certificate (tools/new-macos-signing-cert.sh --name TalkNap).
/// 2.6.0 accepts TalkNap.app only with this certificate: changing it cuts installed apps off from updates.
pub const SIGNING_CERT_SHA1: &str = "82E7C479987E47FEAA2A457F64A8F09205F0CA77";

/// The app up to 2.6.0: only used to find and remove it, its data and its login item.
pub const OLD_BUNDLE_ID: &str = "io.github.pandemoniummm.KakaoAdHide";
pub const OLD_APP_NAME: &str = "KakaoAdHide.app";
/// The old name as it appears in crash report file names (the old executable was Contents/MacOS/KakaoAdHide).
const OLD_NAME: &str = "KakaoAdHide";

/// What a downloaded update must be signed with, checked on top of the feed signature. Moving to a Developer ID
/// later: first ship a version that also accepts it (... or anchor apple generic and certificate leaf[subject.OU]
/// = "<TEAM>"), and only then sign with it.
fn requirement(id: &str, sha1: &str) -> String {
    format!("identifier \"{id}\" and certificate leaf = H\"{sha1}\"")
}

fn home() -> PathBuf { PathBuf::from(std::env::var_os("HOME").unwrap_or_default()) }

pub fn data_dir() -> PathBuf { home().join("Library/Application Support/TalkNap") }

/// Where KakaoAdHide (2.6.0 and earlier) kept its settings and certificate.
fn old_data_dir() -> PathBuf { home().join("Library/Application Support").join(OLD_NAME) }

// ---------------------------------------------------------------- settings

fn settings_path() -> PathBuf { data_dir().join("settings.json") }

fn load() -> BTreeMap<String, String> {
    std::fs::read(settings_path()).ok().and_then(|d| serde_json::from_slice(&d).ok()).unwrap_or_default()
}

pub fn get(name: &str) -> Option<String> { load().get(name).cloned() }

pub fn set(name: &str, value: Option<&str>) {
    let mut m = load();
    match value { Some(v) => { m.insert(name.into(), v.into()); } None => { m.remove(name); } }
    let _ = std::fs::create_dir_all(data_dir());
    let _ = std::fs::write(settings_path(), serde_json::to_vec_pretty(&m).unwrap_or_default());
}

// ---------------------------------------------------------------- certificate trust

fn login_keychain() -> PathBuf { home().join("Library/Keychains/login.keychain-db") }

pub fn certificate_trusted(p: &Paths) -> bool {
    p.pem.exists()
        && Command::new("/usr/bin/security")
            .args(["verify-cert", "-c"]).arg(&p.pem)
            .args(["-p", "ssl", "-n", hosts::AD_HOSTS[0], "-L", "-q"])
            .stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null())
            .status().map(|s| s.success()).unwrap_or(false)
}

/// Adds the certificate to the login keychain, trusted for SSL only (trustRoot: it is self-signed, so macOS
/// treats it as a root). macOS asks for the user's password.
pub fn trust_certificate(p: &Paths) -> Result<(), String> {
    let ok = Command::new("/usr/bin/security")
        .args(["add-trusted-cert", "-r", "trustRoot", "-p", "ssl", "-k"]).arg(login_keychain()).arg(&p.pem)
        .status().map(|s| s.success()).unwrap_or(false);
    if ok { Ok(()) } else { Err("인증서를 신뢰하지 않아 설정을 마치지 못했습니다.".into()) }
}

pub fn untrust_certificate(p: &Paths) {
    let _ = Command::new("/usr/bin/security").arg("remove-trusted-cert").arg(&p.pem).status();
    if let Ok(der) = std::fs::read(&p.der) {
        let _ = Command::new("/usr/bin/security")
            .args(["delete-certificate", "-Z", &setup::sha1_hex(&der)]).arg(login_keychain()).status();
    }
}

/// SHA-1 hashes of the login keychain's certificates named like the old CN (`security find-certificate -Z`).
fn old_certificate_hashes() -> Vec<String> {
    let Ok(out) = Command::new("/usr/bin/security")
        .args(["find-certificate", "-a", "-c", crate::certs::OLD_COMMON_NAME, "-Z"]).arg(login_keychain())
        .stderr(std::process::Stdio::null()).output() else { return Vec::new() };
    if !out.status.success() { return Vec::new(); }
    String::from_utf8_lossy(&out.stdout).lines()
        .filter_map(|l| l.trim().strip_prefix("SHA-1 hash:")).map(|h| h.trim().to_string()).filter(|h| !h.is_empty()).collect()
}

/// True when the login keychain holds a certificate whose name is the old CN.
pub fn old_certificates_present() -> bool { !old_certificate_hashes().is_empty() }

/// Deletes every old-CN certificate from the login keychain (-t also drops its user trust settings).
/// Whether it worked is decided by looking again, not by the exit codes.
pub fn remove_old_certificates() -> Result<(), String> {
    for hash in old_certificate_hashes() {
        let _ = Command::new("/usr/bin/security").args(["delete-certificate", "-Z", &hash, "-t"]).arg(login_keychain()).status();
    }
    if old_certificates_present() { Err("옛 인증서를 지우지 못했습니다.".into()) } else { Ok(()) }
}

// ---------------------------------------------------------------- hosts (administrator)

/// Writes the hosts file with or without our entries. macOS asks for an administrator password; only a fixed
/// copy of a file we just wrote is run as root.
pub fn set_hosts(add: bool) -> Result<(), String> {
    let current = hosts::read().map_err(|e| e.to_string())?;
    let new = if add { hosts::add(&current) } else { hosts::remove(&current) };
    let tmp = data_dir().join("hosts.new");
    std::fs::create_dir_all(data_dir()).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, new).map_err(|e| e.to_string())?;
    let prompt = if add { "TalkNap이 카카오톡 광고 주소 두 개를 이 Mac으로 돌리려고 합니다." }
                 else { "TalkNap이 hosts 파일에 넣었던 두 줄을 지우려고 합니다." };
    let script = format!(
        // The resolver didn't always pick up the new file after a plain HUP: flush, then restart it (launchd
        // starts it again at once). The flush steps run even if an earlier one fails.
        "on run argv\n do shell script \"/bin/cp \" & quoted form of (item 1 of argv) & \" /etc/hosts && /usr/sbin/chown root:wheel /etc/hosts && /bin/chmod 644 /etc/hosts; r=$?; /usr/bin/dscacheutil -flushcache; /usr/bin/killall -HUP mDNSResponder; /usr/bin/killall mDNSResponder; /usr/bin/killall mDNSResponderHelper; exit $r\" with prompt \"{prompt}\" with administrator privileges\nend run"
    );
    let ok = Command::new("/usr/bin/osascript").arg("-e").arg(script).arg(&tmp).status().map(|s| s.success()).unwrap_or(false);
    let _ = std::fs::remove_file(&tmp);
    if ok { Ok(()) } else { Err("관리자 권한을 받지 못해 hosts 파일을 바꾸지 못했습니다.".into()) }
}

// ---------------------------------------------------------------- app location, login item, single instance

/// …/TalkNap.app when running from a bundle.
pub fn bundle_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?.canonicalize().ok()?;
    let app = exe.parent()?.parent()?.parent()?.to_path_buf();
    (app.extension().map(|e| e == "app").unwrap_or(false)).then_some(app)
}

pub fn in_applications(app: &Path) -> bool {
    let parent = app.parent().map(|p| p.to_path_buf()).unwrap_or_default();
    parent == Path::new("/Applications") || parent == home().join("Applications")
}

/// Run from the disk image or Downloads: offer to copy into Applications (needed for updates and the login item).
/// Returns true when the copy was started and this instance should quit.
pub fn offer_move_to_applications() -> bool {
    let Some(here) = bundle_path() else { return false };
    if in_applications(&here) { return false; }
    if !crate::ui::ask(&crate::notices::move_to_applications()) {
        return false;
    }
    let system = Path::new("/Applications");
    let folder = if is_writable(system) { system.to_path_buf() } else { home().join("Applications") };
    let dest = folder.join(APP_NAME);
    let _ = std::fs::create_dir_all(&folder);
    if dest.exists() { let _ = Command::new("/bin/rm").arg("-rf").arg(&dest).status(); }
    let copied = Command::new("/usr/bin/ditto").arg(&here).arg(&dest).status().map(|s| s.success()).unwrap_or(false);
    if !copied { crate::ui::error(&crate::notices::move_failed(&folder.display().to_string())); return false; }
    Command::new("/usr/bin/open").arg("-n").arg(&dest).spawn().is_ok()
}

fn is_writable(dir: &Path) -> bool {
    let probe = dir.join(format!(".talknap-{}", std::process::id()));
    let ok = std::fs::write(&probe, b"").is_ok();
    let _ = std::fs::remove_file(probe);
    ok
}

/// ~/Library/LaunchAgents/<label>.plist: ours is labelled BUNDLE_ID, KakaoAdHide's was OLD_BUNDLE_ID.
fn launch_agent_for(label: &str) -> PathBuf { home().join(format!("Library/LaunchAgents/{label}.plist")) }

fn launch_agent() -> PathBuf { launch_agent_for(BUNDLE_ID) }

pub fn login_item_enabled() -> bool { launch_agent().exists() }

/// The LaunchAgent that opens `app` at login. AssociatedBundleIdentifiers makes "Allow in the Background"
/// (System Settings > General > Login Items) show the app's name rather than `open` from an unknown developer.
fn login_agent_plist(app: &Path) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>Label</key><string>{BUNDLE_ID}</string>
<key>ProgramArguments</key><array><string>/usr/bin/open</string><string>-g</string><string>-a</string><string>{}</string><string>--args</string><string>--run</string></array>
<key>RunAtLoad</key><true/>
<key>AssociatedBundleIdentifiers</key><array><string>{BUNDLE_ID}</string></array>
</dict></plist>
"#,
        xml_escape(&app.display().to_string())
    )
}

/// Whether a LaunchAgent plist (as login_agent_plist writes it) opens `app`.
fn plist_points_to(plist: &str, app: &Path) -> bool {
    plist.contains(&format!("<string>{}</string>", xml_escape(&app.display().to_string())))
}

pub fn set_login_item(on: bool) -> Result<(), String> {
    let path = launch_agent();
    if !on { let _ = std::fs::remove_file(&path); return Ok(()); }
    let app = bundle_path().ok_or("앱 위치를 알 수 없습니다.")?;
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    std::fs::write(&path, login_agent_plist(&app)).map_err(|e| e.to_string())
}

fn xml_escape(s: &str) -> String { s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;") }

// ---------------------------------------------------------------- moving over from KakaoAdHide (2.6.0 and earlier)

/// True only for KakaoAdHide's bundle identifier: nothing else is ever removed as the old app.
fn is_old_app(bundle_id: &str) -> bool { bundle_id == OLD_BUNDLE_ID }

/// The app's CFBundleIdentifier ("" when it can't be read).
fn bundle_id_of(app: &Path) -> String {
    Command::new("/usr/bin/plutil").args(["-extract", "CFBundleIdentifier", "raw", "-o", "-"]).arg(app.join("Contents/Info.plist"))
        .stderr(std::process::Stdio::null()).output().ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default()
}

/// Crash reports of the old app: ~/Library/Logs/DiagnosticReports/KakaoAdHide-….ips (or KakaoAdHide_…).
fn is_old_crash_report(name: &str) -> bool {
    name.strip_prefix(OLD_NAME).is_some_and(|rest| rest.starts_with('-') || rest.starts_with('_'))
}

/// ~/Library/Application Support/CrashReporter/KakaoAdHide_<uuid>.plist
fn is_old_crash_reporter_plist(name: &str) -> bool {
    name.strip_prefix(OLD_NAME).is_some_and(|rest| rest.starts_with('_') && rest.ends_with(".plist"))
}

/// Moves `from` (a file or folder) to `to`, which must not exist: a rename (same disk), else a copy that is
/// removed again if it fails. True when `from` is now at `to`; `from` is left as it was otherwise.
fn move_item(from: &Path, to: &Path) -> bool {
    if std::fs::rename(from, to).is_ok() { return true; }
    let copied = Command::new("/usr/bin/ditto").arg(from).arg(to)
        .stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null())
        .status().map(|s| s.success()).unwrap_or(false);
    if !copied {
        let _ = if to.is_dir() { std::fs::remove_dir_all(to) } else { std::fs::remove_file(to) };
        return false;
    }
    let _ = if from.is_dir() { std::fs::remove_dir_all(from) } else { std::fs::remove_file(from) };
    true
}

/// Moves settings.json and certs/ from the old data folder into ours, then removes the old folder (its server.log,
/// lock and hosts.new are dropped). What we already have wins: a settings.json or certs/ of ours is kept and the
/// old one dropped. If a move fails the old folder is kept, with the old certificate in it, and the move is tried
/// again at the next start.
fn move_old_data() {
    let (old, new) = (old_data_dir(), data_dir());
    if !old.is_absolute() || !old.is_dir() || old == new { return; }
    if std::fs::create_dir_all(&new).is_err() { return; }
    for name in ["certs", "settings.json"] {
        let (from, to) = (old.join(name), new.join(name));
        if std::fs::symlink_metadata(&from).is_err() || std::fs::symlink_metadata(&to).is_ok() { continue; }
        if !move_item(&from, &to) { return; }
    }
    let _ = std::fs::remove_dir_all(&old);
}

/// Stops and removes KakaoAdHide.app in /Applications and ~/Applications, if its bundle identifier is the old one.
fn remove_old_apps(running: &Path) {
    for folder in [PathBuf::from("/Applications"), home().join("Applications")] {
        let app = folder.join(OLD_APP_NAME);
        if !app.is_absolute() || app.as_path() == running || !app.is_dir() || !is_old_app(&bundle_id_of(&app)) { continue; }
        let stopped = Command::new("/usr/bin/pkill").arg("-f").arg(format!("^{}/Contents/MacOS/", app.display()))
            .stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null())
            .status().map(|s| s.success()).unwrap_or(false);
        // let it exit (and free port 443) before our server starts
        if stopped { std::thread::sleep(std::time::Duration::from_secs(1)); }
        let _ = std::fs::remove_dir_all(&app);
    }
}

/// The login item (issue #15): KakaoAdHide's LaunchAgent opens the old app, which is gone after the update, so
/// it is replaced by ours. Ours is rewritten when it opens another path (the app was moved). With neither, the
/// user turned it off: left off. Login Items the user added in System Settings are not touched.
fn move_login_item(app: &Path) {
    let old = launch_agent_for(OLD_BUNDLE_ID);
    if old.exists() {
        let _ = std::fs::remove_file(&old);
        let _ = set_login_item(true);
    } else if let Ok(plist) = std::fs::read_to_string(launch_agent()) {
        if !plist_points_to(&plist, app) { let _ = set_login_item(true); }
    }
}

/// What macOS kept for the old bundle identifier and name: preferences, saved window state, caches, crash reports.
fn remove_old_traces() {
    let lib = home().join("Library");
    if !lib.is_absolute() { return; }
    let _ = std::fs::remove_file(lib.join(format!("Preferences/{OLD_BUNDLE_ID}.plist")));
    let _ = std::fs::remove_dir_all(lib.join(format!("Saved Application State/{OLD_BUNDLE_ID}.savedState")));
    let _ = std::fs::remove_dir_all(lib.join(format!("Caches/{OLD_BUNDLE_ID}")));
    let remove_matching = |dir: PathBuf, matches: fn(&str) -> bool| {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for e in entries.flatten() {
            if e.file_name().to_str().is_some_and(matches) { let _ = std::fs::remove_file(e.path()); }
        }
    };
    remove_matching(lib.join("Logs/DiagnosticReports"), is_old_crash_report);
    remove_matching(lib.join("Application Support/CrashReporter"), is_old_crash_reporter_plist);
}

/// Moves over from KakaoAdHide, at every start (each step first checks, cheaply, whether there is anything to do;
/// failures are silent and tried again next time): (1) the old app, (2) its data, (3) the login item, (4) what
/// macOS kept under the old names. Only for the app installed in Applications: a copy run from the disk image or a
/// bare binary (cargo run) changes nothing.
pub fn migrate_old() {
    let Some(app) = bundle_path().filter(|a| in_applications(a)) else { return };
    remove_old_apps(&app);
    move_old_data();
    move_login_item(&app);
    remove_old_traces();
}

/// Held for the life of the process: a second instance finds it locked and quits.
pub fn single_instance() -> Option<std::fs::File> {
    use std::os::fd::AsRawFd;
    let _ = std::fs::create_dir_all(data_dir());
    let f = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(data_dir().join("lock")).ok()?;
    extern "C" { fn flock(fd: i32, op: i32) -> i32; }
    const LOCK_EX: i32 = 2; const LOCK_NB: i32 = 4;
    (unsafe { flock(f.as_raw_fd(), LOCK_EX | LOCK_NB) } == 0).then_some(f)
}

pub fn open_url(url: &str) { let _ = Command::new("/usr/bin/open").arg(url).spawn(); }

// ---------------------------------------------------------------- first run, uninstall

pub fn first_run() {
    if get("SetupDone").is_none() {
        set("SetupDone", Some("1"));
        if bundle_path().map(|a| in_applications(&a)).unwrap_or(false) { let _ = set_login_item(true); }
    }
}

pub fn uninstall(_quiet: bool) -> Result<(), String> {
    let r = setup::undo();
    let _ = set_login_item(false);
    let _ = std::fs::remove_file(launch_agent_for(OLD_BUNDLE_ID));
    let _ = std::fs::remove_dir_all(data_dir());
    let _ = std::fs::remove_dir_all(old_data_dir());
    if let Some(app) = bundle_path() {
        let _ = Command::new("/usr/bin/osascript")
            .arg("-e").arg("on run argv\n tell application \"Finder\" to delete (POSIX file (item 1 of argv) as alias)\nend run")
            .arg(&app).status();
    }
    r
}

// ---------------------------------------------------------------- updates

pub fn can_update_in_place() -> bool {
    bundle_path().map(|a| in_applications(&a) && is_writable(a.parent().unwrap())).unwrap_or(false)
}

/// Mounts the disk image read-only and invisibly, copies TalkNap.app out and checks it: our bundle identifier and
/// certificate, version equal to the feed's, and a valid code signature. Detaches before returning.
pub fn extract_verified_app(dmg: &Path, work: &Path, version: &str) -> Result<PathBuf, VerifyError> {
    let bad = |why: &str| VerifyError::BadPackage(why.to_string());
    let mount = work.join("mount");
    std::fs::create_dir_all(&mount).map_err(|e| bad(&e.to_string()))?;
    if !attach_image(dmg, &mount) { return Err(bad("디스크 이미지를 열지 못했습니다")); }
    let result = (|| {
        let source = mount.join(APP_NAME);
        let meta = std::fs::symlink_metadata(&source).map_err(|_| bad("앱이 없습니다"))?;
        if !meta.is_dir() { return Err(bad("앱이 없습니다")); }
        let copy = work.join(APP_NAME);
        let ok = Command::new("/usr/bin/ditto").arg("--noqtn").arg(&source).arg(&copy).status().map(|s| s.success()).unwrap_or(false);
        if !ok { return Err(bad("앱을 복사하지 못했습니다")); }
        let plist = copy.join("Contents/Info.plist");
        let read = |key: &str| Command::new("/usr/bin/plutil").args(["-extract", key, "raw", "-o", "-"]).arg(&plist)
            .output().ok().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
        if read("CFBundleIdentifier") != BUNDLE_ID { return Err(bad("번들 ID가 다릅니다")); }
        if read("CFBundleShortVersionString") != version { return Err(bad("버전이 다릅니다")); }
        let signed = Command::new("/usr/bin/codesign")
            .args(["--verify", "--deep", "--strict"]).arg(format!("-R={}", requirement(BUNDLE_ID, SIGNING_CERT_SHA1))).arg(&copy)
            .stderr(std::process::Stdio::null()).status().map(|s| s.success()).unwrap_or(false);
        if !signed { return Err(bad("코드 서명이 맞지 않습니다")); }
        Ok(copy)
    })();
    detach_image(&mount);
    result
}

fn quietly(cmd: &mut Command) -> bool {
    cmd.stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status().map(|s| s.success()).unwrap_or(false)
}

/// Mounts read-only and hidden from Finder. `diskutil image` where macOS has it (hdiutil attach is deprecated
/// there), hdiutil on older systems.
fn attach_image(dmg: &Path, mount: &Path) -> bool {
    quietly(Command::new("/usr/sbin/diskutil").args(["image", "attach", "--readOnly", "--nobrowse", "--mountPoint"]).arg(mount).arg(dmg))
        || quietly(Command::new("/usr/bin/hdiutil")
            .args(["attach", "-nobrowse", "-readonly", "-noautoopen", "-noverify", "-mountpoint"]).arg(mount).arg(dmg))
}

fn detach_image(mount: &Path) {
    let _ = quietly(Command::new("/usr/sbin/diskutil").arg("eject").arg(mount))
        || quietly(Command::new("/usr/bin/hdiutil").arg("detach").arg(mount))
        || quietly(Command::new("/usr/bin/hdiutil").args(["detach", "-force"]).arg(mount));
}

/// Swaps the app once the running copy (pid) has exited, putting the old one back if the swap fails, then
/// stops any other copy still running from the old files and opens whichever is in place. Arguments: pid, new app,
/// where it goes, work folder (removed at the end).
const REPLACE_SCRIPT: &str = r#"#!/bin/bash
pid="$1"; src="$2"; dst="$3"; work="$4"
for _ in $(seq 1 300); do kill -0 "$pid" 2>/dev/null || break; sleep 0.2; done
sleep 0.3
dir="$(dirname "$dst")"; name="$(basename "$dst")"
staged="$dir/.$name.update-$$"; previous="$dir/.$name.previous-$$"
ok=0
if /usr/bin/ditto "$src" "$staged"; then
  if [ ! -e "$dst" ] || /bin/mv "$dst" "$previous"; then
    if /bin/mv "$staged" "$dst"; then ok=1; /bin/rm -rf "$previous"
    elif [ -e "$previous" ]; then /bin/mv "$previous" "$dst"; fi
  fi
fi
/bin/rm -rf "$staged"
# a copy still running from the old files (update started from the command line) would keep the old version
/usr/bin/pkill -f "^$dst/Contents/MacOS/" 2>/dev/null && sleep 1
/usr/bin/open "$dst"
/bin/rm -rf "$work"
"#;

/// Downloads, verifies and hands over to a script that swaps the app once this process has exited (restoring
/// the old one if the swap fails) and opens it. The caller quits on Ok.
pub fn install_update(feed: &Feed) -> Result<(), crate::updater::InstallError> {
    use crate::updater::InstallError;
    let app = bundle_path().filter(|_| can_update_in_place()).ok_or(InstallError::NotInApplications)?;
    let work = std::env::temp_dir().join(format!("TalkNap-update-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(|e| InstallError::Io(e.to_string()))?;
    let dmg = work.join(UPDATE_ASSET);
    crate::updater::download(feed, &dmg)?;
    feed::verify_file(feed, &dmg, keys::PUBLIC_KEYS).map_err(InstallError::Verify)?;
    hand_over(&dmg, &work, &feed.version, &app, std::process::id(), REPLACE_SCRIPT)
}

/// The part of install_update after the download and feed check: checks the app in the disk image
/// (extract_verified_app), writes `script` (REPLACE_SCRIPT, or for a test a copy that opens nothing) to the work
/// folder and starts it to swap the installed `app` once process `wait_pid` has exited. The script removes `work`.
fn hand_over(dmg: &Path, work: &Path, version: &str, app: &Path, wait_pid: u32, script: &str) -> Result<(), crate::updater::InstallError> {
    use crate::updater::InstallError;
    let new_app = extract_verified_app(dmg, work, version).map_err(InstallError::Verify)?;
    let script_path = work.join("replace.sh");
    std::fs::write(&script_path, script).map_err(|e| InstallError::Io(e.to_string()))?;
    Command::new("/bin/bash")
        .arg(&script_path).arg(wait_pid.to_string()).arg(&new_app).arg(app).arg(work)
        .stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null())
        .spawn().map_err(|e| InstallError::Io(e.to_string()))?;
    Ok(())
}

/// For the release workflow: the feed and disk image, checked with the same code users run.
pub fn verify_package(feed: &Feed, dmg: &Path) -> Result<(), VerifyError> {
    feed.check_url()?;
    feed::verify_file(feed, dmg, keys::PUBLIC_KEYS)?;
    let work = std::env::temp_dir().join(format!("TalkNap-verify-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(|e| VerifyError::BadPackage(e.to_string()))?;
    let r = extract_verified_app(dmg, &work, &feed.version).map(|_| ());
    let _ = std::fs::remove_dir_all(&work);
    r
}

// ---------------------------------------------------------------- KakaoTalk active or not (the menu button dims)

const KAKAO_BUNDLE_ID: &str = "com.kakao.KakaoTalkMac";

/// When started by hand (after installing, or from Applications), also starts KakaoTalk unless it already runs,
/// so it fetches the blank ad right away. At login KakaoTalk starts (or not) by its own setting.
pub fn start_kakao(at_sign_in: bool) {
    use objc2_app_kit::NSRunningApplication;
    use objc2_foundation::NSString;
    if at_sign_in { return; }
    let running = NSRunningApplication::runningApplicationsWithBundleIdentifier(&NSString::from_str(KAKAO_BUNDLE_ID));
    if running.count() > 0 { return; }
    // nothing happens if KakaoTalk isn't installed
    let _ = Command::new("/usr/bin/open").args(["-b", KAKAO_BUNDLE_ID]).spawn();
}

/// The menu's "restart KakaoTalk": quits it as Cmd-Q would and starts it again once it has exited (or just starts
/// it), so it fetches the blank ad anew. In the background: KakaoTalk may take a moment to quit.
pub fn restart_kakao() {
    use objc2_app_kit::NSRunningApplication;
    use objc2_foundation::NSString;
    let running = NSRunningApplication::runningApplicationsWithBundleIdentifier(&NSString::from_str(KAKAO_BUNDLE_ID));
    let pids: Vec<i32> = running.iter().map(|a| { a.terminate(); a.processIdentifier() }).collect();
    std::thread::spawn(move || {
        extern "C" { fn kill(pid: i32, sig: i32) -> i32; }
        // up to 15 s for it to exit (it may ask something first); then start it whatever happened
        for _ in 0..60 {
            if pids.iter().all(|&p| unsafe { kill(p, 0) } != 0) { break; }
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
        let _ = Command::new("/usr/bin/open").args(["-b", KAKAO_BUNDLE_ID]).spawn();
    });
}

/// KakaoTalk's process id while it is the active app, 0 otherwise.
static KAKAO_PID: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

/// Tells the server whether KakaoTalk's main window is in front, as KakaoTalk dims its sidebar icons otherwise:
/// another app in front (NSWorkspace's activation notification), or one of KakaoTalk's other windows (a chat
/// room, settings). Those are told apart by the window list, checked every 250 ms but only while KakaoTalk is the
/// active app: its main window is the one as wide as the blank ad (`server::ad_width`). Window titles would say
/// it directly but need the Screen Recording permission. Call on the main thread.
pub fn watch_kakao_active() {
    use block2::RcBlock;
    use objc2_app_kit::{NSWorkspace, NSWorkspaceDidActivateApplicationNotification};
    use objc2_foundation::NSNotification;
    use std::ptr::NonNull;
    use std::sync::atomic::Ordering;
    fn note() {
        let front = NSWorkspace::sharedWorkspace().frontmostApplication();
        let pid = front.filter(|a| a.bundleIdentifier().is_some_and(|s| s.to_string() == KAKAO_BUNDLE_ID))
            .map(|a| a.processIdentifier()).unwrap_or(0);
        KAKAO_PID.store(pid, Ordering::SeqCst);
        crate::server::set_kakao_active(pid != 0 && main_window_in_front(pid));
    }
    let block = RcBlock::new(|_: NonNull<NSNotification>| note());
    let center = NSWorkspace::sharedWorkspace().notificationCenter();
    let observer = unsafe {
        center.addObserverForName_object_queue_usingBlock(Some(NSWorkspaceDidActivateApplicationNotification), None, None, &block)
    };
    std::mem::forget(observer);   // kept for the app's lifetime
    note();
    std::thread::spawn(|| loop {
        std::thread::sleep(std::time::Duration::from_millis(250));
        let pid = KAKAO_PID.load(Ordering::SeqCst);
        if pid != 0 { crate::server::set_kakao_active(main_window_in_front(pid)); }
    });
}

/// Whether KakaoTalk's frontmost window is its main window. True when that can't be told (the blank ad hasn't
/// said its width yet, or KakaoTalk shows no window), so the button then just follows the active app.
fn main_window_in_front(pid: i32) -> bool {
    let width = crate::server::ad_width() as f64;
    if width == 0.0 { return true; }
    let windows = app_windows(pid);
    let Some(&(front, _)) = windows.first() else { return true };
    // the lowest window number if several are as wide (the main window is usually KakaoTalk's first)
    let main = windows.iter().filter(|w| (w.1 - width).abs() < 1.5).map(|w| w.0).min();
    main == Some(front)
}

/// The on-screen normal windows (layer 0) of process `pid`, front to back: (window number, width in points).
fn app_windows(pid: i32) -> Vec<(i64, f64)> {
    use objc2::rc::{autoreleasepool, Retained};
    use objc2::runtime::AnyObject;
    use objc2_foundation::{NSArray, NSDictionary, NSNumber, NSString};
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" { fn CGWindowListCopyWindowInfo(option: u32, relative_to: u32) -> *mut AnyObject; }
    const ON_SCREEN_ONLY: u32 = 1 << 0;
    const EXCLUDE_DESKTOP: u32 = 1 << 4;
    autoreleasepool(|_| {
        let ptr = unsafe { CGWindowListCopyWindowInfo(ON_SCREEN_ONLY | EXCLUDE_DESKTOP, 0) };
        // a CFArray of CFDictionary (toll-free bridged), owned by us (a Copy function)
        let Some(list) = (unsafe { Retained::<NSArray<NSDictionary<NSString, AnyObject>>>::from_raw(ptr.cast()) }) else { return Vec::new() };
        let number = |d: &NSDictionary<NSString, AnyObject>, key: &str| {
            d.objectForKey(&NSString::from_str(key)).and_then(|v| v.downcast::<NSNumber>().ok())
        };
        list.iter().filter_map(|d| {
            if number(&d, "kCGWindowOwnerPID")?.intValue() != pid || number(&d, "kCGWindowLayer")?.intValue() != 0 { return None; }
            let bounds = d.objectForKey(&NSString::from_str("kCGWindowBounds"))?.downcast::<NSDictionary>().ok()?;
            let width = bounds.objectForKey(&NSString::from_str("Width"))?.downcast::<NSNumber>().ok()?.doubleValue();
            Some((number(&d, "kCGWindowNumber")?.longLongValue(), width))
        }).collect()
    })
}

// ---------------------------------------------------------------- notifications (Notification Center)

mod notify {
    use objc2::rc::Retained;
    use objc2::runtime::{Bool, NSObject, NSObjectProtocol, ProtocolObject};
    use objc2::{define_class, msg_send, AllocAnyThread};
    use objc2_foundation::{NSArray, NSBundle, NSError, NSSet, NSString};
    use objc2_user_notifications::*;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Duration;
    use std::sync::Mutex;

    pub type OnClick = Box<dyn Fn(Option<String>) + Send>;
    static NEXT: AtomicU64 = AtomicU64::new(0);
    /// What to do when a shown notification (by request id) or one of its buttons is clicked.
    static CLICKS: Mutex<Option<HashMap<String, OnClick>>> = Mutex::new(None);

    define_class!(
        #[unsafe(super(NSObject))]
        #[name = "TalkNapNotificationDelegate"]
        struct Delegate;

        unsafe impl NSObjectProtocol for Delegate {}

        unsafe impl UNUserNotificationCenterDelegate for Delegate {
            // shown as a banner even while TalkNap counts as the active app (its menu is open)
            #[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]
            fn will_present(&self, _c: &UNUserNotificationCenter, _n: &UNNotification,
                            done: &block2::DynBlock<dyn Fn(UNNotificationPresentationOptions)>) {
                done.call((UNNotificationPresentationOptions::Banner | UNNotificationPresentationOptions::List,));
            }

            #[unsafe(method(userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:))]
            fn did_receive(&self, _c: &UNUserNotificationCenter, response: &UNNotificationResponse, done: &block2::DynBlock<dyn Fn()>) {
                let id = response.notification().request().identifier().to_string();
                let action = response.actionIdentifier().to_string();
                let f = CLICKS.lock().ok().and_then(|mut g| g.as_mut().and_then(|m| m.remove(&id)));
                if let Some(f) = f {
                    let default = unsafe { UNNotificationDefaultActionIdentifier }.to_string();
                    if action == default { f(None) } else if !action.contains("Dismiss") { f(Some(action)) }
                }
                done.call(());
            }
        }
    );

    /// Notification Center needs an app bundle: false when run from a bare binary (cargo run, tests).
    fn in_bundle() -> bool { NSBundle::mainBundle().bundleIdentifier().is_some() }

    /// Asks for permission (macOS asks the user once) and registers `buttons` as the one set of notification
    /// buttons this app uses. Call once at start, on the main thread.
    pub fn register(buttons: &[(&str, &str)]) {
        if !in_bundle() { return; }
        let center = UNUserNotificationCenter::currentNotificationCenter();
        let delegate: Retained<Delegate> = unsafe { msg_send![Delegate::alloc(), init] };
        center.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
        std::mem::forget(delegate);   // the center holds its delegate weakly: kept for the app's lifetime
        let actions: Vec<Retained<UNNotificationAction>> = buttons.iter().map(|(label, id)| {
            UNNotificationAction::actionWithIdentifier_title_options(&NSString::from_str(id), &NSString::from_str(label), UNNotificationActionOptions::empty())
        }).collect();
        let category = UNNotificationCategory::categoryWithIdentifier_actions_intentIdentifiers_options(
            &NSString::from_str(&category_id(buttons)), &NSArray::from_retained_slice(&actions), &NSArray::new(), UNNotificationCategoryOptions::empty());
        center.setNotificationCategories(&NSSet::from_retained_slice(&[category]));
        let answered = block2::RcBlock::new(|_granted: Bool, _e: *mut NSError| {});
        center.requestAuthorizationWithOptions_completionHandler(UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound, &answered);
    }

    /// Whether a notification would show now: allowed, with alerts on. Asked each time (the user can turn them
    /// off in System Settings at any moment); false if there's no answer within a second.
    fn alerts_on() -> bool {
        let (tx, rx) = std::sync::mpsc::channel();
        let got = block2::RcBlock::new(move |s: std::ptr::NonNull<UNNotificationSettings>| {
            let s = unsafe { s.as_ref() };
            let allowed = matches!(s.authorizationStatus(), UNAuthorizationStatus::Authorized | UNAuthorizationStatus::Provisional);
            let _ = tx.send(allowed && s.alertSetting() == UNNotificationSetting::Enabled);
        });
        UNUserNotificationCenter::currentNotificationCenter().getNotificationSettingsWithCompletionHandler(&got);
        rx.recv_timeout(Duration::from_secs(1)).unwrap_or(false)
    }

    fn category_id(buttons: &[(&str, &str)]) -> String {
        buttons.iter().map(|(_, id)| *id).collect::<Vec<_>>().join("-")
    }

    /// Shows a notification; false when it wouldn't show (not allowed, not answered yet, or alerts turned off),
    /// so the caller shows a dialog instead. `buttons` must be the set given to `register` (or empty).
    pub fn show(title: &str, text: &str, buttons: &[(&str, &str)], on_click: Option<OnClick>) -> bool {
        if !in_bundle() || !alerts_on() { return false; }
        let content = UNMutableNotificationContent::new();
        content.setTitle(&NSString::from_str(title));
        content.setBody(&NSString::from_str(text));
        if !buttons.is_empty() { content.setCategoryIdentifier(&NSString::from_str(&category_id(buttons))); }
        let id = format!("talknap-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::SeqCst));
        if let Some(f) = on_click {
            if let Ok(mut g) = CLICKS.lock() { g.get_or_insert_with(HashMap::new).insert(id.clone(), f); }
        }
        let request = UNNotificationRequest::requestWithIdentifier_content_trigger(&NSString::from_str(&id), &content, None);
        UNUserNotificationCenter::currentNotificationCenter().addNotificationRequest_withCompletionHandler(&request, None);
        true
    }
}

/// Registers the app for notifications (macOS asks the user once) with the update notification's buttons.
pub fn register_notifications() {
    notify::register(&[(crate::notices::UPDATE, "update"), (crate::notices::LATER, "later")]);
}

/// Shows a notification in Notification Center: `title`, then `text`. `on_click` gets the clicked button's id,
/// or None for the notification itself. False if it can't be shown (not allowed): show a dialog then.
pub fn notify(title: &str, text: &str, buttons: &[(&str, &str)], on_click: Option<Box<dyn Fn(Option<String>) + Send>>) -> bool {
    notify::show(title, text, buttons, on_click)
}

#[cfg(test)]
mod tests {
    use super::*;

    // The swap script, against throwaway folders: replaces the app after the given process has exited.
    #[test]
    fn replace_script_swaps_the_app() {
        let dir = std::env::temp_dir().join(format!("kah-replace-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (new, dst, work) = (dir.join("new/X.app"), dir.join("Applications/X.app"), dir.join("work"));
        for d in [&new, &dst, &work] { std::fs::create_dir_all(d).unwrap(); }
        std::fs::write(new.join("v"), "2").unwrap();
        std::fs::write(dst.join("v"), "1").unwrap();
        let script = work.join("replace.sh");
        std::fs::write(&script, REPLACE_SCRIPT.replace("/usr/bin/open", "true")).unwrap();
        let mut sleeper = Command::new("/bin/sleep").arg("1").spawn().unwrap();
        let pid = sleeper.id().to_string();
        let reaper = std::thread::spawn(move || sleeper.wait());   // a zombie would still answer kill -0
        let status = Command::new("/bin/bash").arg(&script).arg(&pid).arg(&new).arg(&dst).arg(&work).status().unwrap();
        let _ = reaper.join();
        assert!(status.success());
        assert_eq!(std::fs::read_to_string(dst.join("v")).unwrap(), "2");
        assert!(!work.exists());
        let leftovers: Vec<_> = std::fs::read_dir(dir.join("Applications")).unwrap().flatten().map(|e| e.file_name()).collect();
        assert_eq!(leftovers.len(), 1, "{leftovers:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn login_agent_plist_has_label_and_app() {
        let plist = login_agent_plist(Path::new("/Applications/TalkNap.app"));
        assert_eq!(plist.matches("<string>io.github.pandemoniummm.TalkNap</string>").count(), 2, "{plist}");   // Label, AssociatedBundleIdentifiers
        assert!(plist.contains("<string>/Applications/TalkNap.app</string>"));
        assert!(plist.contains("<string>--args</string><string>--run</string>"));
        assert!(plist.contains("<key>RunAtLoad</key><true/>"));
    }

    #[test]
    fn login_agent_points_elsewhere() {
        let new = Path::new("/Applications/TalkNap.app");
        assert!(!plist_points_to(&login_agent_plist(Path::new("/Applications/KakaoAdHide.app")), new));
        assert!(plist_points_to(&login_agent_plist(new), new));
        let odd = Path::new("/Users/a/Applications/R&D <x>/TalkNap.app");
        assert!(login_agent_plist(odd).contains("R&amp;D &lt;x&gt;"));
        assert!(plist_points_to(&login_agent_plist(odd), odd));
    }

    #[test]
    fn old_app_only_with_old_bundle_id() {
        assert!(is_old_app(OLD_BUNDLE_ID));
        assert!(!is_old_app(BUNDLE_ID));
        assert!(!is_old_app(""));
        assert!(!is_old_app("com.kakao.KakaoTalkMac"));
    }

    #[test]
    fn accepted_pair_is_talknap() {
        assert_eq!(requirement(BUNDLE_ID, SIGNING_CERT_SHA1),
            "identifier \"io.github.pandemoniummm.TalkNap\" and certificate leaf = H\"82E7C479987E47FEAA2A457F64A8F09205F0CA77\"");
        assert_ne!(old_data_dir(), data_dir());
        assert!(data_dir().ends_with("TalkNap") && old_data_dir().ends_with("KakaoAdHide"));
        assert_ne!(launch_agent_for(OLD_BUNDLE_ID), launch_agent());
    }

    #[test]
    fn old_traces_by_name() {
        assert!(is_old_crash_report("KakaoAdHide-2026-11-25-101010.ips"));
        assert!(is_old_crash_report("KakaoAdHide_2026-11-25-101010_Mac.crash"));
        assert!(!is_old_crash_report("TalkNap-2026-11-25-101010.ips"));
        assert!(!is_old_crash_report("KakaoAdHideX-1.ips"));
        assert!(!is_old_crash_report("KakaoTalk-2026-11-25-101010.ips"));
        assert!(is_old_crash_reporter_plist("KakaoAdHide_0A1B2C.plist"));
        assert!(!is_old_crash_reporter_plist("KakaoAdHide_0A1B2C.txt"));
        assert!(!is_old_crash_reporter_plist("TalkNap_0A1B2C.plist"));
    }

    // Run by hand against disk images made by packaging/macos/build.sh --dmg:
    //   KAH_SIGNED_DMG=<signed dmg> KAH_UNSIGNED_DMG=<ad-hoc dmg> KAH_DMG_VERSION=<version> \
    //     cargo test -- --ignored disk_images
    #[test]
    #[ignore]
    fn disk_images() {
        let version = std::env::var("KAH_DMG_VERSION").unwrap();
        let check = |var: &str| {
            let work = std::env::temp_dir().join(format!("kah-dmg-{var}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&work);
            std::fs::create_dir_all(&work).unwrap();
            let r = extract_verified_app(Path::new(&std::env::var(var).unwrap()), &work, &version).map(|_| ());
            let _ = std::fs::remove_dir_all(&work);
            r
        };
        check("KAH_SIGNED_DMG").expect("signed image");
        assert!(matches!(check("KAH_UNSIGNED_DMG"), Err(VerifyError::BadPackage(ref w)) if w.contains("서명")));
        let work = std::env::temp_dir().join(format!("kah-dmg-ver-{}", std::process::id()));
        std::fs::create_dir_all(&work).unwrap();
        let r = extract_verified_app(Path::new(&std::env::var("KAH_SIGNED_DMG").unwrap()), &work, "0.0.1");
        let _ = std::fs::remove_dir_all(&work);
        assert!(matches!(r, Err(VerifyError::BadPackage(ref w)) if w.contains("버전")));
    }
}
