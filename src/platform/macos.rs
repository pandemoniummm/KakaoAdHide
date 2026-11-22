//! macOS: KakaoAdHide.app in /Applications (or ~/Applications), a menu bar item, a LaunchAgent to start at login,
//! settings in ~/Library/Application Support/KakaoAdHide/settings.json.

use crate::certs::Paths;
use crate::feed::{self, Feed, VerifyError};
use crate::{hosts, keys, setup};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const NAME: &str = "macos";
pub const BUNDLE_ID: &str = "io.github.pandemoniummm.KakaoAdHide";
pub const APP_NAME: &str = "KakaoAdHide.app";
pub const UPDATE_ASSET: &str = "KakaoAdHide-macos.dmg";

/// SHA-1 of the self-signed "KakaoAdHide Code Signing" certificate (tools/new-macos-signing-cert.sh).
pub const SIGNING_CERT_SHA1: &str = "DB044D3EF322C3D1E1961BB3D91063A3E07CF06A";

/// What a downloaded update must be signed with, checked on top of the feed signature. Moving to a Developer ID
/// later: first ship a version that also accepts it (... or anchor apple generic and certificate leaf[subject.OU]
/// = "<TEAM>"), and only then sign with it.
pub fn signing_requirement() -> String {
    format!("identifier \"{BUNDLE_ID}\" and certificate leaf = H\"{SIGNING_CERT_SHA1}\"")
}

fn home() -> PathBuf { PathBuf::from(std::env::var_os("HOME").unwrap_or_default()) }

pub fn data_dir() -> PathBuf { home().join("Library/Application Support/KakaoAdHide") }

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

// ---------------------------------------------------------------- hosts (administrator)

/// Writes the hosts file with or without our entries. macOS asks for an administrator password; only a fixed
/// copy of a file we just wrote is run as root.
pub fn set_hosts(add: bool) -> Result<(), String> {
    let current = hosts::read().map_err(|e| e.to_string())?;
    let new = if add { hosts::add(&current) } else { hosts::remove(&current) };
    let tmp = data_dir().join("hosts.new");
    std::fs::create_dir_all(data_dir()).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, new).map_err(|e| e.to_string())?;
    let prompt = if add { "KakaoAdHide가 카카오톡 광고 주소 두 개를 이 Mac으로 돌리려고 합니다." }
                 else { "KakaoAdHide가 hosts 파일에 넣었던 두 줄을 지우려고 합니다." };
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

/// …/KakaoAdHide.app when running from a bundle.
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
    if !crate::ui::confirm("응용 프로그램 폴더로 옮길까요?",
        "KakaoAdHide는 응용 프로그램 폴더에 있어야 로그인할 때 실행되고 업데이트도 됩니다.", "옮기기", "그대로 실행") {
        return false;
    }
    let system = Path::new("/Applications");
    let folder = if is_writable(system) { system.to_path_buf() } else { home().join("Applications") };
    let dest = folder.join(APP_NAME);
    let _ = std::fs::create_dir_all(&folder);
    if dest.exists() { let _ = Command::new("/bin/rm").arg("-rf").arg(&dest).status(); }
    let copied = Command::new("/usr/bin/ditto").arg(&here).arg(&dest).status().map(|s| s.success()).unwrap_or(false);
    if !copied { crate::ui::message("옮기지 못했습니다", &format!("{}에 직접 끌어다 놓아 주세요.", folder.display())); return false; }
    Command::new("/usr/bin/open").arg("-n").arg(&dest).spawn().is_ok()
}

fn is_writable(dir: &Path) -> bool {
    let probe = dir.join(format!(".kakaoadhide-{}", std::process::id()));
    let ok = std::fs::write(&probe, b"").is_ok();
    let _ = std::fs::remove_file(probe);
    ok
}

fn launch_agent() -> PathBuf { home().join(format!("Library/LaunchAgents/{BUNDLE_ID}.plist")) }

pub fn login_item_enabled() -> bool { launch_agent().exists() }

pub fn set_login_item(on: bool) -> Result<(), String> {
    let path = launch_agent();
    if !on { let _ = std::fs::remove_file(&path); return Ok(()); }
    let app = bundle_path().ok_or("앱 위치를 알 수 없습니다.")?;
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>Label</key><string>{BUNDLE_ID}</string>
<key>ProgramArguments</key><array><string>/usr/bin/open</string><string>-g</string><string>-a</string><string>{}</string><string>--args</string><string>--run</string></array>
<key>RunAtLoad</key><true/>
</dict></plist>
"#,
        xml_escape(&app.display().to_string())
    );
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    std::fs::write(&path, plist).map_err(|e| e.to_string())
}

fn xml_escape(s: &str) -> String { s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;") }

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
    let _ = std::fs::remove_dir_all(data_dir());
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

/// Mounts the disk image read-only and invisibly, copies KakaoAdHide.app out and checks it: bundle identifier,
/// version equal to the feed's, and a valid code signature by our certificate. Detaches before returning.
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
            .args(["--verify", "--deep", "--strict"]).arg(format!("-R={}", signing_requirement())).arg(&copy)
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
/// stops any other copy still running from the old files and opens whichever is in place. Arguments: pid, new app, installed app, work folder (removed at the end).
const REPLACE_SCRIPT: &str = r#"#!/bin/bash
pid="$1"; src="$2"; dst="$3"; work="$4"
for _ in $(seq 1 300); do kill -0 "$pid" 2>/dev/null || break; sleep 0.2; done
sleep 0.3
dir="$(dirname "$dst")"; name="$(basename "$dst")"
staged="$dir/.$name.update-$$"; previous="$dir/.$name.previous-$$"
if /usr/bin/ditto "$src" "$staged" && /bin/mv "$dst" "$previous"; then
  if /bin/mv "$staged" "$dst"; then /bin/rm -rf "$previous"; else /bin/mv "$previous" "$dst"; fi
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
    let work = std::env::temp_dir().join(format!("KakaoAdHide-update-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(|e| InstallError::Io(e.to_string()))?;
    let dmg = work.join(UPDATE_ASSET);
    crate::updater::download(feed, &dmg)?;
    feed::verify_file(feed, &dmg, keys::PUBLIC_KEYS).map_err(InstallError::Verify)?;
    let new_app = extract_verified_app(&dmg, &work, &feed.version).map_err(InstallError::Verify)?;
    let script = work.join("replace.sh");
    std::fs::write(&script, REPLACE_SCRIPT).map_err(|e| InstallError::Io(e.to_string()))?;
    Command::new("/bin/bash").arg(&script).arg(std::process::id().to_string()).arg(&new_app).arg(&app).arg(&work)
        .stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null())
        .spawn().map_err(|e| InstallError::Io(e.to_string()))?;
    Ok(())
}

/// For the release workflow: the feed and disk image, checked with the same code users run.
pub fn verify_package(feed: &Feed, dmg: &Path) -> Result<(), VerifyError> {
    feed.check_url()?;
    feed::verify_file(feed, dmg, keys::PUBLIC_KEYS)?;
    let work = std::env::temp_dir().join(format!("KakaoAdHide-verify-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(|e| VerifyError::BadPackage(e.to_string()))?;
    let r = extract_verified_app(dmg, &work, &feed.version).map(|_| ());
    let _ = std::fs::remove_dir_all(&work);
    r
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
        std::fs::write(&script, REPLACE_SCRIPT.replace("/usr/bin/open \"$dst\"", "true")).unwrap();
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
