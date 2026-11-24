//! Windows: installed as TalkNap (2.7.0 on):
//!   - %LOCALAPPDATA%\TalkNap\TalkNap.exe, a Startup shortcut TalkNap.lnk running it with --run, an "Installed apps"
//!     entry (HKCU\...\Uninstall\TalkNap), settings in HKCU\Software\TalkNap, notifications under the
//!     AppUserModelID pandemoniummm.TalkNap,
//!   - mutex Local\TalkNap (one instance), event Local\TalkNap.Quit (asks a running copy to exit),
//!   - --run, --uninstall, --update, --quiet, --verify-feed (unchanged since 1.0.0: an old version updates by
//!     running the new exe with --quiet),
//!   - KakaoTalk's own auto start is left as the user set it (see `release_kakao_autostart`).
//!
//! 1.0.0 to 2.6.0 installed the same things under the name KakaoAdHide (%LOCALAPPDATA%\KakaoAdHide\KakaoAdHide.exe,
//! KakaoAdHide.lnk, HKCU\Software\KakaoAdHide, ...\Uninstall\KakaoAdHide, pandemoniummm.KakaoAdHide, mutex
//! Local\KakaoAdHide, event Local\KakaoAdHide.Quit, %TEMP%\KakaoAdHide-update). The OLD_* names are kept only to
//! find such an install, stop it, take over its settings and certificate and remove it (`install`, `first_run`).
//!
//! Setting up the ad server needs the hosts file (administrator: the exe runs itself elevated with `--hosts-add`
//! or `--hosts-remove`) and the certificate in the user's trusted roots (Windows shows a confirmation).

use crate::certs::Paths;
use crate::feed::{self, Feed, VerifyError};
use crate::{hosts, keys, setup, VERSION};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, WAIT_ABANDONED, WAIT_OBJECT_0};
use windows_sys::Win32::System::Threading::{
    CreateEventW, CreateMutexW, GetExitCodeProcess, OpenEventW, ReleaseMutex, ResetEvent, SetEvent, WaitForSingleObject,
    EVENT_MODIFY_STATE, INFINITE,
};
use windows_sys::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;

pub const NAME: &str = "windows";
pub const APP: &str = "TalkNap";
/// The name 1.0.0 to 2.6.0 installed under (only to move such an install).
pub const OLD_APP: &str = "KakaoAdHide";
const MUTEX: &str = r"Local\TalkNap";
const OLD_MUTEX: &str = r"Local\KakaoAdHide";
const QUIT_EVENT: &str = r"Local\TalkNap.Quit";
const OLD_QUIT_EVENT: &str = r"Local\KakaoAdHide.Quit";
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const APP_KEY: &str = r"Software\TalkNap";
const OLD_APP_KEY: &str = r"Software\KakaoAdHide";
const UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\TalkNap";
const OLD_UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\KakaoAdHide";
const KAKAO_RUN_VALUE: &str = "KakaoTalk";
/// The AppUserModelID notifications are shown under; registered so they read "TalkNap" (removed on uninstall).
const NOTIFY_APP_ID: &str = "pandemoniummm.TalkNap";
const OLD_NOTIFY_APP_ID: &str = "pandemoniummm.KakaoAdHide";
/// Where an AppUserModelID's DisplayName and icon are registered.
fn app_id_key(id: &str) -> String { format!(r"Software\Classes\AppUserModelId\{id}") }
/// What KakaoAdHide put in its install folder besides its exe and `certs` (which is moved): removed with it.
const OLD_FILES: [&str; 3] = ["server.log", "server.log.old", "icon.png"];

fn wide(s: &str) -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() }
fn env_dir(var: &str) -> PathBuf { PathBuf::from(std::env::var_os(var).unwrap_or_default()) }

pub fn install_dir() -> PathBuf { env_dir("LOCALAPPDATA").join(APP) }
pub fn installed_exe() -> PathBuf { install_dir().join(format!("{APP}.exe")) }
fn startup_dir() -> PathBuf { env_dir("APPDATA").join(r"Microsoft\Windows\Start Menu\Programs\Startup") }
fn startup_lnk() -> PathBuf { startup_dir().join(format!("{APP}.lnk")) }
pub fn update_temp_dir() -> PathBuf { std::env::temp_dir().join(format!("{APP}-update")) }

fn old_install_dir() -> PathBuf { env_dir("LOCALAPPDATA").join(OLD_APP) }
fn old_installed_exe() -> PathBuf { old_install_dir().join(format!("{OLD_APP}.exe")) }
fn old_startup_lnk() -> PathBuf { startup_dir().join(format!("{OLD_APP}.lnk")) }
fn old_update_temp_dir() -> PathBuf { std::env::temp_dir().join(format!("{OLD_APP}-update")) }

/// App data next to the installed exe (removed with it on uninstall).
pub fn data_dir() -> PathBuf { install_dir() }

// ---------------------------------------------------------------- settings (HKCU\Software\KakaoAdHide, as 1.0.0)

pub fn get(name: &str) -> Option<String> {
    let k = RegKey::predef(HKEY_CURRENT_USER).open_subkey(APP_KEY).ok()?;
    if let Ok(s) = k.get_value::<String, _>(name) { return Some(s); }
    k.get_value::<u32, _>(name).ok().map(|v| v.to_string())   // UpdateCheck is a DWORD (README documents it)
}

pub fn set(name: &str, value: Option<&str>) {
    let Ok((k, _)) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(APP_KEY) else { return };
    match value {
        None => { let _ = k.delete_value(name); }
        Some(v) if name == "UpdateCheck" => { let _ = k.set_value(name, &v.parse::<u32>().unwrap_or(1)); }
        Some(v) => { let _ = k.set_value(name, &v.to_string()); }
    }
}

// ---------------------------------------------------------------- certificate trust (CurrentUser\Root)

fn certutil(args: &[&OsStr]) -> bool {
    use std::os::windows::process::CommandExt;
    Command::new("certutil").args(args).creation_flags(0x0800_0000).stdout(std::process::Stdio::null())
        .status().map(|s| s.success()).unwrap_or(false)
}

fn thumbprint(p: &Paths) -> Option<String> { std::fs::read(&p.der).ok().map(|d| setup::sha1_hex(&d)) }

pub fn certificate_trusted(p: &Paths) -> bool {
    thumbprint(p).map(|t| certutil(&[OsStr::new("-user"), OsStr::new("-verifystore"), OsStr::new("Root"), OsStr::new(&t)])).unwrap_or(false)
}

/// Adds the certificate to the user's trusted roots. Windows asks the user to confirm.
pub fn trust_certificate(p: &Paths) -> Result<(), String> {
    if certutil(&[OsStr::new("-user"), OsStr::new("-addstore"), OsStr::new("-f"), OsStr::new("Root"), p.der.as_os_str()]) { Ok(()) }
    else { Err("인증서를 신뢰하지 않아 설정을 마치지 못했습니다.".into()) }
}

pub fn untrust_certificate(p: &Paths) {
    if let Some(t) = thumbprint(p) { let _ = certutil(&[OsStr::new("-user"), OsStr::new("-delstore"), OsStr::new("Root"), OsStr::new(&t)]); }
}

/// True when the user's trusted roots hold a certificate whose name is the old CN (exit code 0; not found is nonzero).
pub fn old_certificates_present() -> bool {
    certutil(&[OsStr::new("-user"), OsStr::new("-store"), OsStr::new("Root"), OsStr::new(crate::certs::OLD_COMMON_NAME)])
}

/// Deletes every old-CN certificate (certutil removes one per call, and the same CN can be there several times).
pub fn remove_old_certificates() -> Result<(), String> {
    for _ in 0..10 {
        if !old_certificates_present() { return Ok(()); }
        let _ = certutil(&[OsStr::new("-user"), OsStr::new("-delstore"), OsStr::new("Root"), OsStr::new(crate::certs::OLD_COMMON_NAME)]);
    }
    if old_certificates_present() { Err("옛 인증서를 지우지 못했습니다.".into()) } else { Ok(()) }
}

// ---------------------------------------------------------------- hosts (administrator, via an elevated copy of us)

/// Runs this exe elevated with --hosts-add / --hosts-remove (Windows shows the UAC prompt) and waits for it.
pub fn set_hosts(add: bool) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let (verb, file, params) = (wide("runas"), wide(&exe.display().to_string()), wide(if add { "--hosts-add" } else { "--hosts-remove" }));
    let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = SEE_MASK_NOCLOSEPROCESS;
    info.lpVerb = verb.as_ptr();
    info.lpFile = file.as_ptr();
    info.lpParameters = params.as_ptr();
    info.nShow = 0;
    if unsafe { ShellExecuteExW(&mut info) } == 0 || info.hProcess.is_null() {
        return Err("관리자 권한을 받지 못해 hosts 파일을 바꾸지 못했습니다.".into());
    }
    let mut code = 1u32;
    unsafe {
        WaitForSingleObject(info.hProcess, INFINITE);
        GetExitCodeProcess(info.hProcess, &mut code);
        CloseHandle(info.hProcess);
    }
    if code == 0 { Ok(()) } else { Err("hosts 파일을 바꾸지 못했습니다.".into()) }
}

/// The elevated half of set_hosts. Exit code 0 on success.
pub fn hosts_elevated(add: bool) -> i32 {
    let Ok(current) = hosts::read() else { return 1 };
    let new = if add { hosts::add(&current) } else { hosts::remove(&current) };
    if new != current && std::fs::write(hosts::HOSTS_PATH, new).is_err() { return 1; }
    use std::os::windows::process::CommandExt;
    let _ = Command::new("ipconfig").arg("/flushdns").creation_flags(0x0800_0000).stdout(std::process::Stdio::null()).status();
    0
}

// ---------------------------------------------------------------- single instance, quit event

pub struct Instance { mutex: HANDLE }
impl Drop for Instance { fn drop(&mut self) { unsafe { ReleaseMutex(self.mutex); CloseHandle(self.mutex); } } }

/// Owns Local\TalkNap for the life of the process; None when another copy runs.
pub fn single_instance() -> Option<Instance> {
    let name = wide(MUTEX);
    let m = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
    if m.is_null() { return None; }
    let r = unsafe { WaitForSingleObject(m, 0) };
    if r == WAIT_OBJECT_0 || r == WAIT_ABANDONED { Some(Instance { mutex: m }) } else { unsafe { CloseHandle(m) }; None }
}

/// Makes our hidden menu window the foreground window before a menu opens on it, so the menu takes the keyboard
/// and closes when clicking elsewhere. The click came from KakaoTalk's window, and Windows only lets the
/// foreground thread hand the foreground over, so attach to its input for the call.
/// Returns the window that was in front, for `give_back_foreground`.
pub fn take_foreground(hwnd: isize) -> isize {
    use windows_sys::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId, SetForegroundWindow};
    let hwnd = hwnd as windows_sys::Win32::Foundation::HWND;
    unsafe {
        let fg = GetForegroundWindow();
        let fg_thread = if fg.is_null() { 0 } else { GetWindowThreadProcessId(fg, std::ptr::null_mut()) };
        let me = GetCurrentThreadId();
        let attached = fg_thread != 0 && fg_thread != me && AttachThreadInput(me, fg_thread, 1) != 0;
        SetForegroundWindow(hwnd);
        if attached { AttachThreadInput(me, fg_thread, 0); }
        fg as isize
    }
}

/// After the menu: if KakaoAdHide's (hidden) window is still in front, the window that was before gets it back,
/// so KakaoTalk is active again as it was. Then the button's state follows what's in front, now and once more a
/// moment later (closing a menu by clicking another window activates that window just after).
pub fn give_back_foreground(mine: isize, prev: isize) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, SetForegroundWindow};
    unsafe {
        if prev != 0 && GetForegroundWindow() as isize == mine { SetForegroundWindow(prev as _); }
    }
    recheck_kakao_active();
    std::thread::spawn(|| { std::thread::sleep(std::time::Duration::from_millis(300)); recheck_kakao_active(); });
}

/// Calls `on_quit` (from a background thread) when another copy signals Local\TalkNap.Quit.
pub fn watch_quit_event(on_quit: impl FnOnce() + Send + 'static) {
    let name = wide(QUIT_EVENT);
    let ev = unsafe { CreateEventW(std::ptr::null(), 1, 0, name.as_ptr()) };
    if ev.is_null() { return; }
    unsafe { ResetEvent(ev) };
    let ev = ev as usize;
    std::thread::spawn(move || {
        unsafe { WaitForSingleObject(ev as HANDLE, INFINITE) };
        on_quit();
    });
}

/// Asks a running copy to exit and waits until it has (as 1.0.0's installer did): this version's and a
/// KakaoAdHide (1.0.0 to 2.6.0) one, as both may run while an old install is being replaced.
pub fn stop_running() {
    for q in [QUIT_EVENT, OLD_QUIT_EVENT] {
        let name = wide(q);
        let ev = unsafe { OpenEventW(EVENT_MODIFY_STATE, 0, name.as_ptr()) };
        if !ev.is_null() { unsafe { SetEvent(ev); CloseHandle(ev); } }
    }
    for name in [MUTEX, OLD_MUTEX] {
        let mname = wide(name);
        let m = unsafe { CreateMutexW(std::ptr::null(), 0, mname.as_ptr()) };
        if m.is_null() { continue; }
        let r = unsafe { WaitForSingleObject(m, 5000) };
        unsafe {
            if r == WAIT_OBJECT_0 || r == WAIT_ABANDONED { ReleaseMutex(m); }
            CloseHandle(m);
        }
    }
}

// ---------------------------------------------------------------- install / uninstall (as 1.0.0)

pub fn self_path() -> PathBuf { std::env::current_exe().unwrap_or_default() }

pub fn is_installed_copy() -> bool {
    let a = self_path().canonicalize().ok();
    let b = installed_exe().canonicalize().ok();
    a.is_some() && a == b
}

pub fn install(quiet: bool) -> Result<(), String> {
    // Over an existing install (a newer exe run by hand, or one a KakaoAdHide 1.0.0 to 2.6.0 install downloaded)
    // this is an update: no "installed" message, the new copy says it was updated when it starts (ui:
    // updater::note_version, which reads LastVersion taken over from the old settings).
    let existing = installed_exe().exists() || old_installed_exe().exists();
    stop_running();
    std::fs::create_dir_all(install_dir()).map_err(|e| e.to_string())?;
    // The stopped copy (1.0.0 or this version) may take a moment to exit and release its exe: retry for 15s.
    let mut copied = std::fs::copy(self_path(), installed_exe());
    for _ in 0..30 {
        if copied.is_ok() { break; }
        std::thread::sleep(std::time::Duration::from_millis(500));
        copied = std::fs::copy(self_path(), installed_exe());
    }
    copied.map_err(|e| format!("설치하지 못했습니다: {e}"))?;
    // Before anything reads them: the old install's certificate (still trusted, so no new confirmation) and
    // settings (KakaoRun for release_kakao_autostart below, LastVersion for the "updated" message).
    let adopted = adopt_old_install();
    let mut lnk = mslnk::ShellLink::new(installed_exe()).map_err(|e| e.to_string())?;
    lnk.set_arguments(Some("--run".into()));
    lnk.set_working_dir(Some(install_dir().display().to_string()));
    lnk.set_name(Some(APP.into()));
    lnk.create_lnk(startup_lnk()).map_err(|e| e.to_string())?;
    release_kakao_autostart();
    register_notifications();

    let (k, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(UNINSTALL_KEY).map_err(|e| e.to_string())?;
    let exe = installed_exe().display().to_string();
    let _ = k.set_value("DisplayName", &"TalkNap (카카오톡 광고 칸 비우기)".to_string());
    let _ = k.set_value("DisplayVersion", &VERSION.to_string());
    let _ = k.set_value("Publisher", &"TalkNap".to_string());
    let _ = k.set_value("DisplayIcon", &exe);
    let _ = k.set_value("InstallLocation", &install_dir().display().to_string());
    let _ = k.set_value("UninstallString", &format!("\"{exe}\" --uninstall"));
    let _ = k.set_value("QuietUninstallString", &format!("\"{exe}\" --uninstall --quiet"));
    let _ = k.set_value("NoModify", &1u32);   // u32 is written as REG_DWORD
    let _ = k.set_value("NoRepair", &1u32);
    let size_kb = std::fs::metadata(installed_exe()).map(|m| (m.len() / 1024) as u32).unwrap_or(0);
    let _ = k.set_value("EstimatedSize", &size_kb);
    remove_old_install(adopted);

    if !quiet && !existing {
        crate::ui::show(&crate::notices::installed());
    }
    Command::new(installed_exe()).current_dir(install_dir()).spawn().map_err(|e| e.to_string())?;
    Ok(())
}

pub fn uninstall(quiet: bool) -> Result<(), String> {
    stop_running();
    // An old install left behind goes too. First, so that its certificate (moved into certs/) is untrusted with
    // ours and its saved KakaoTalk auto start (taken over) is given back below.
    // Its certs/ goes even when it couldn't be moved: undo removes the old certificate from the trusted roots by name.
    let adopted = adopt_old_install();
    remove_old_install(Adopted { certs_taken: true, ..adopted });
    let undo = setup::undo();
    let _ = std::fs::remove_file(startup_lnk());
    restore_kakao_autostart();
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let _ = hkcu.delete_subkey_all(UNINSTALL_KEY);
    let _ = hkcu.delete_subkey_all(APP_KEY);
    let _ = std::fs::remove_dir_all(update_temp_dir());
    let _ = std::fs::remove_dir_all(setup::cert_dir());
    for f in ["server.log", "server.log.old", "icon.png"] { let _ = std::fs::remove_file(data_dir().join(f)); }
    if !quiet {
        match &undo {
            Ok(()) => crate::ui::show(&crate::notices::removed()),
            Err(e) => crate::ui::error(&crate::notices::uninstall_incomplete(e)),
        }
    }
    let _ = hkcu.delete_subkey_all(app_id_key(NOTIFY_APP_ID));   // after the notification above, which is shown under it
    // The exe can't delete itself while running: cmd does it after we exit (rmdir without /s only removes an
    // empty folder, so nothing else that happens to be in there is touched). Uninstalling from the tray menu runs
    // in the app itself, which can take a while to exit, so cmd keeps retrying for about a minute.
    if installed_exe().exists() {
        use std::os::windows::process::CommandExt;
        let (exe, dir) = (installed_exe(), install_dir());
        let cmd = format!(
            "/c for /l %i in (1,1,30) do @(ping 127.0.0.1 -n 3 >nul & if exist \"{e}\" (del /f /q \"{e}\" 2>nul) & if not exist \"{e}\" (rmdir \"{d}\" 2>nul & exit))",
            e = exe.display(), d = dir.display()
        );
        // Break away from a job the app may run in (one that kills its children on exit); fall back if not allowed.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
        let spawn = |flags| Command::new("cmd.exe").raw_arg(&cmd).creation_flags(flags).spawn();
        if spawn(CREATE_NO_WINDOW | CREATE_BREAKAWAY_FROM_JOB).is_err() { let _ = spawn(CREATE_NO_WINDOW); }
    }
    undo
}

// ---------------------------------------------------------------- the KakaoAdHide install (1.0.0 to 2.6.0)

/// Copies every value of `from` into `to`, each with its own type (REG_SZ, REG_DWORD, ...), when `to` has none yet
/// (it's never overwritten). True when something was copied.
fn migrate_settings(from: &RegKey, to: &RegKey) -> std::io::Result<bool> {
    if to.enum_values().next().is_some() { return Ok(false); }
    let values = from.enum_values().collect::<std::io::Result<Vec<_>>>()?;
    for (name, value) in &values { to.set_raw_value(name, value)?; }
    Ok(!values.is_empty())
}

/// Takes over the old settings (HKCU\Software\KakaoAdHide into HKCU\Software\TalkNap). False when they are there
/// and couldn't be copied, so they must be kept.
fn migrate_old_settings() -> bool {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let Ok(old) = hkcu.open_subkey(OLD_APP_KEY) else { return true };
    let Ok((new, _)) = hkcu.create_subkey(APP_KEY) else { return false };
    migrate_settings(&old, &new).is_ok()
}

/// Moves the old install's certs/ (certificate and key, already trusted) into ours when we have none yet:
/// renamed, or copied when that fails.
fn move_old_certs() {
    let (old, new) = (old_install_dir().join("certs"), setup::cert_dir());
    if !old_install_dir().is_absolute() || !old.is_dir() || new.exists() || !install_dir().is_dir() { return; }
    if std::fs::rename(&old, &new).is_ok() { return; }
    if copy_dir(&old, &new).is_ok() { let _ = std::fs::remove_dir_all(&old); } else { let _ = std::fs::remove_dir_all(&new); }
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)? {
        let e = e?;
        if e.file_type()?.is_dir() { copy_dir(&e.path(), &to.join(e.file_name()))?; }
        else { std::fs::copy(e.path(), to.join(e.file_name()))?; }
    }
    Ok(())
}

/// What `adopt_old_install` took over, so `remove_old_install` knows what it may delete.
#[derive(Clone, Copy)]
struct Adopted {
    /// The old settings are in ours (or there were none): the old key may go.
    settings_taken: bool,
    /// We have our own certs/ (moved from the old install, or made earlier): the old certs/ may go. When the move
    /// failed it's kept: setup keeps serving with that still-trusted certificate if the user declines a new one,
    /// and the next start tries the move again.
    certs_taken: bool,
}

/// What install and setup need from an old install, before they run: its certificate and settings. Only ever
/// called synchronously, before setup and the version note (never next to them on another thread).
fn adopt_old_install() -> Adopted {
    move_old_certs();
    Adopted { certs_taken: setup::cert_dir().is_dir(), settings_taken: migrate_old_settings() }
}

/// Removes a KakaoAdHide install after `adopt_old_install` took over what it could: its "Installed apps" entry,
/// settings key, Startup shortcut, notification name, update leftovers, and its folder's known files. Only
/// deletes (it may run on a background thread next to setup). Its exe may still be exiting (it was just asked to
/// quit): retried for 15 s, and when it's still there `first_run` tries again at the next start. The folder itself
/// is removed only when nothing else is left in it.
fn remove_old_install(adopted: Adopted) {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let _ = hkcu.delete_subkey_all(OLD_UNINSTALL_KEY);
    if adopted.settings_taken { let _ = hkcu.delete_subkey_all(OLD_APP_KEY); }
    let _ = hkcu.delete_subkey_all(app_id_key(OLD_NOTIFY_APP_ID));
    if startup_dir().is_absolute() { let _ = std::fs::remove_file(old_startup_lnk()); }
    let _ = std::fs::remove_dir_all(old_update_temp_dir());   // fails on the installer, if it's us: cleanup_update_temp
    let dir = old_install_dir();
    if !dir.is_absolute() || !dir.is_dir() { return; }
    for f in OLD_FILES { let _ = std::fs::remove_file(dir.join(f)); }
    if adopted.certs_taken { let _ = std::fs::remove_dir_all(dir.join("certs")); }
    let exe = old_installed_exe();
    let me = self_path().canonicalize().ok();
    if me.is_some() && me == exe.canonicalize().ok() { return; }   // never this running exe
    for _ in 0..30 {
        if !exe.exists() || std::fs::remove_file(&exe).is_ok() { break; }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    let _ = std::fs::remove_dir(&dir);   // not recursive: anything else in there keeps the folder
}

/// Whether anything of a KakaoAdHide install is left (it couldn't all be removed when this one was installed).
fn old_install_left() -> bool {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    (old_install_dir().is_absolute() && old_install_dir().exists())
        || hkcu.open_subkey(OLD_APP_KEY).is_ok()
        || hkcu.open_subkey(OLD_UNINSTALL_KEY).is_ok()
        || (startup_dir().is_absolute() && old_startup_lnk().exists())
}

// ---------------------------------------------------------------- KakaoTalk's own auto start
//
// Left as the user set it. Up to 2.1.0 KakaoAdHide turned it off (keeping the original in KakaoRun) and started
// KakaoTalk itself, so KakaoTalk's first ad request found the server running. That isn't needed: when KakaoTalk
// starts first, its request fails, it shows its built-in banner and asks again a few minutes later (measured:
// about 3 minutes), by which time the server is up.

/// Gives back KakaoTalk's auto start if an older version turned it off, and forgets the saved copy.
pub fn release_kakao_autostart() {
    restore_kakao_autostart();
    if let Ok(app) = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(APP_KEY, winreg::enums::KEY_WRITE) {
        let _ = app.delete_value("KakaoRun");
    }
}

fn restore_kakao_autostart() {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let Some(v) = hkcu.open_subkey(APP_KEY).ok().and_then(|k| k.get_value::<String, _>("KakaoRun").ok()) else { return };
    if let Ok((run, _)) = hkcu.create_subkey(RUN_KEY) {
        if run.get_value::<String, _>(KAKAO_RUN_VALUE).is_err() { let _ = run.set_value(KAKAO_RUN_VALUE, &v); }
    }
}

fn find_kakao() -> Option<PathBuf> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let mut c: Vec<PathBuf> = Vec::new();
    if let Ok(run) = hkcu.open_subkey(RUN_KEY) {
        if let Some(p) = run.get_value::<String, _>(KAKAO_RUN_VALUE).ok().and_then(|v| exe_from_command_line(&v)) { c.push(p.into()); }
    }
    for base in ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"] { c.push(env_dir(base).join(r"Kakao\KakaoTalk\KakaoTalk.exe")); }
    c.into_iter().find(|p| !p.as_os_str().is_empty() && p.exists())
}

pub fn exe_from_command_line(cmd: &str) -> Option<String> {
    let cmd = cmd.trim();
    if let Some(rest) = cmd.strip_prefix('"') { return rest.find('"').filter(|&e| e > 0).map(|e| rest[..e].to_string()); }
    let i = cmd.to_ascii_lowercase().find(".exe")?;
    (i > 0).then(|| cmd[..i + 4].to_string())
}

// ---------------------------------------------------------------- KakaoTalk active or not (the menu button dims)

/// Follows the foreground window (a system event, nothing polled) and tells the server whether KakaoTalk is the
/// active app. Call on the UI thread: the hook's callback runs from its message loop. KakaoAdHide's own menu
/// counts as another app: KakaoTalk dims its icons while the menu is open, so the button dims with them.
pub fn watch_kakao_active() {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::Accessibility::{SetWinEventHook, HWINEVENTHOOK};
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, EVENT_SYSTEM_FOREGROUND, WINEVENT_OUTOFCONTEXT};
    unsafe extern "system" fn on_foreground(_: HWINEVENTHOOK, _: u32, hwnd: HWND, _: i32, _: i32, _: u32, _: u32) {
        note_foreground(hwnd);
    }
    unsafe {
        SetWinEventHook(EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND, std::ptr::null_mut(), Some(on_foreground), 0, 0, WINEVENT_OUTOFCONTEXT);
        note_foreground(GetForegroundWindow());
    }
}

/// Checks the foreground window again now (after KakaoAdHide's own menu closes).
pub fn recheck_kakao_active() {
    note_foreground(unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow() });
}

fn note_foreground(hwnd: windows_sys::Win32::Foundation::HWND) {
    if hwnd.is_null() { return; }
    crate::server::set_kakao_active(is_kakao_main(hwnd));
}

/// Whether `hwnd` is KakaoTalk's main window itself (class EVA_Window_Dblclk, titled "카카오톡"). Not its other
/// windows: KakaoTalk's sidebar dims while a chat room is in front, and also while the ad slot (a window of its
/// own, owned by the main one) has the focus, after a click on it.
fn is_kakao_main(hwnd: windows_sys::Win32::Foundation::HWND) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetAncestor, GetClassNameW, GetWindowTextW, GA_ROOT};
    let root = unsafe { GetAncestor(hwnd, GA_ROOT) };
    let root = if root.is_null() { hwnd } else { root };
    let mut buf = [0u16; 64];
    let n = unsafe { GetClassNameW(root, buf.as_mut_ptr(), buf.len() as i32) };
    if n <= 0 || String::from_utf16_lossy(&buf[..n as usize]) != "EVA_Window_Dblclk" { return false; }
    let n = unsafe { GetWindowTextW(root, buf.as_mut_ptr(), buf.len() as i32) };
    n > 0 && String::from_utf16_lossy(&buf[..n as usize]) == "카카오톡"
}

/// Whether a KakaoTalk.exe process runs.
fn kakao_running() -> bool { !kakao_pids().is_empty() }

/// The KakaoTalk.exe processes (a process snapshot, no external program).
fn kakao_pids() -> Vec<u32> {
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
    };
    let mut pids = Vec::new();
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE { return pids; }
        let mut e: PROCESSENTRY32W = std::mem::zeroed();
        e.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut ok = Process32FirstW(snap, &mut e) != 0;
        while ok {
            let len = e.szExeFile.iter().position(|&c| c == 0).unwrap_or(e.szExeFile.len());
            if String::from_utf16_lossy(&e.szExeFile[..len]).eq_ignore_ascii_case("KakaoTalk.exe") { pids.push(e.th32ProcessID); }
            ok = Process32NextW(snap, &mut e) != 0;
        }
        CloseHandle(snap);
    }
    pids
}

/// The menu's "restart KakaoTalk": ends it (closing its window only hides it in the tray) and starts it again once
/// it has exited, so it fetches the blank ad anew. In the background: waits up to 10 s for it to exit.
pub fn restart_kakao() {
    use windows_sys::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE};
    std::thread::spawn(|| {
        for pid in kakao_pids() {
            unsafe {
                let p = OpenProcess(PROCESS_TERMINATE | PROCESS_SYNCHRONIZE, 0, pid);
                if p.is_null() { continue; }
                if TerminateProcess(p, 0) != 0 { WaitForSingleObject(p, 10_000); }
                CloseHandle(p);
            }
        }
        if let Some(k) = find_kakao() { let _ = Command::new(k).spawn(); }
    });
}

/// When started by hand (after installing, or from the Start menu), also starts KakaoTalk unless it already runs.
/// At sign-in KakaoTalk starts (or not) by its own setting.
pub fn start_kakao(at_sign_in: bool) {
    release_kakao_autostart();
    if at_sign_in || kakao_running() { return; }
    if let Some(k) = find_kakao() { let _ = Command::new(k).spawn(); }
}

// ---------------------------------------------------------------- notifications (toasts)

/// Registers the notification sender name (an unpackaged app's AppUserModelID needs a DisplayName to show as
/// "TalkNap" rather than an id). Cheap, so it's also done at each start (older installs, repaired settings).
pub fn register_notifications() {
    // the app icon for the notification header: a PNG next to the exe (removed on uninstall)
    let png = install_dir().join("icon.png");
    let icon_ok = std::fs::write(&png, include_bytes!("../../assets/icon-256.png")).is_ok();
    if let Ok((k, _)) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(app_id_key(NOTIFY_APP_ID)) {
        let _ = k.set_value("DisplayName", &"TalkNap".to_string());
        if icon_ok { let _ = k.set_value("IconUri", &png.display().to_string()); }
    }
}

/// Shows a Windows notification: the title, then `text` as up to two lines (split at the first newline, so a
/// sentence isn't broken mid-way). `on_click` runs (on a system thread) when it's clicked while this app runs.
/// False if it couldn't be shown (notifications off for the app still count as shown: Windows keeps them quiet).
/// `buttons`: (label, id) pairs. `on_click` gets the clicked button's id, or None for the notification itself.
pub fn notify(title: &str, text: &str, buttons: &[(&str, &str)], on_click: Option<Box<dyn Fn(Option<String>) + Send>>) -> bool {
    use tauri_winrt_notification::Toast;
    let (line1, line2) = text.split_once('\n').unwrap_or((text, ""));
    let mut t = Toast::new(NOTIFY_APP_ID).title(title).text1(line1);
    if !line2.is_empty() { t = t.text2(line2); }
    for (label, id) in buttons { t = t.add_button(label, id); }
    if let Some(f) = on_click { t = t.on_activated(move |id| { f(id); Ok(()) }); }
    t.show().is_ok()
}

pub fn open_url(url: &str) {
    use std::os::windows::process::CommandExt;
    let _ = Command::new("rundll32").args(["url.dll,FileProtocolHandler", url]).creation_flags(0x0800_0000).spawn();
}

/// At each start: what's left of a KakaoAdHide install (its exe was still running when this one was installed, or
/// removing failed) is taken over now, before setup and the version note read it, and removed in the background
/// (its exe may take up to 15 s). An old folder kept by files we don't know is checked again at every start.
pub fn first_run() {
    if !old_install_left() { return; }
    let adopted = adopt_old_install();
    std::thread::spawn(move || remove_old_install(adopted));
}
pub fn login_item_enabled() -> bool { startup_lnk().exists() }
pub fn set_login_item(_on: bool) -> Result<(), String> { Ok(()) }   // the Startup shortcut is part of the install

// ---------------------------------------------------------------- updates

pub fn can_update_in_place() -> bool { true }

/// Downloads and verifies the new exe, then runs it with --quiet: it installs itself over this copy (stopping
/// this one through the quit event) and starts. The caller quits on Ok.
pub fn install_update(f: &Feed) -> Result<(), crate::updater::InstallError> {
    use crate::updater::InstallError;
    let dir = update_temp_dir();
    std::fs::create_dir_all(&dir).map_err(|e| InstallError::Io(e.to_string()))?;
    let exe = dir.join(format!("{APP}.exe"));
    crate::updater::download(f, &exe)?;
    feed::verify_file(f, &exe, keys::PUBLIC_KEYS).map_err(InstallError::Verify)?;
    Command::new(&exe).arg("--quiet").current_dir(&dir).spawn().map_err(|e| InstallError::Io(e.to_string()))?;
    Ok(())
}

/// Removes the installer an update left behind (it may still be exiting: retry a little). A KakaoAdHide (2.6.0 or
/// older) updating to this one downloaded it to its own folder, %TEMP%\KakaoAdHide-update.
pub fn cleanup_update_temp() {
    std::thread::spawn(|| {
        for _ in 0..10 {
            let left: Vec<PathBuf> = [update_temp_dir(), old_update_temp_dir()].into_iter().filter(|d| d.exists()).collect();
            if left.iter().all(|d| std::fs::remove_dir_all(d).is_ok()) { return; }
            std::thread::sleep(std::time::Duration::from_secs(2));
        }
    });
}

pub fn verify_package(f: &Feed, exe: &Path) -> Result<(), VerifyError> {
    feed::verify_file(f, exe, keys::PUBLIC_KEYS)
}

#[cfg(test)]
mod tests {
    use winreg::enums::{RegType, HKEY_CURRENT_USER};
    use winreg::RegKey;

    /// A throwaway key HKCU\Software\TalkNapTest-<pid>-<name> with subkeys old and new, deleted when dropped
    /// (also when the test fails). Never the real Software\KakaoAdHide or Software\TalkNap.
    struct TestKey(String);
    impl TestKey {
        fn new(name: &str) -> (TestKey, RegKey, RegKey) {
            let path = format!(r"Software\TalkNapTest-{}-{name}", std::process::id());
            let hkcu = RegKey::predef(HKEY_CURRENT_USER);
            let (old, _) = hkcu.create_subkey(format!(r"{path}\old")).unwrap();
            let (new, _) = hkcu.create_subkey(format!(r"{path}\new")).unwrap();
            (TestKey(path), old, new)
        }
    }
    impl Drop for TestKey {
        fn drop(&mut self) { let _ = RegKey::predef(HKEY_CURRENT_USER).delete_subkey_all(&self.0); }
    }

    #[test]
    fn migrate_settings_copies_all_kinds() {
        let (_key, old, new) = TestKey::new("copy");
        old.set_value("Consented", &"1".to_string()).unwrap();
        old.set_value("LastVersion", &"2.6.0".to_string()).unwrap();
        old.set_value("UpdateCheck", &0u32).unwrap();
        assert!(super::migrate_settings(&old, &new).unwrap());
        assert_eq!(new.get_value::<String, _>("Consented").unwrap(), "1");
        assert_eq!(new.get_value::<String, _>("LastVersion").unwrap(), "2.6.0");
        assert_eq!(new.get_value::<u32, _>("UpdateCheck").unwrap(), 0);
        assert_eq!(new.get_raw_value("Consented").unwrap().vtype, RegType::REG_SZ);
        assert_eq!(new.get_raw_value("UpdateCheck").unwrap().vtype, RegType::REG_DWORD);
        assert_eq!(new.enum_values().count(), 3);
    }

    #[test]
    fn migrate_settings_is_idempotent() {
        let (_key, old, new) = TestKey::new("keep");
        old.set_value("LastVersion", &"2.6.0".to_string()).unwrap();
        old.set_value("UpdateCheck", &0u32).unwrap();
        new.set_value("LastVersion", &"2.7.0".to_string()).unwrap();
        assert!(!super::migrate_settings(&old, &new).unwrap());
        assert_eq!(new.get_value::<String, _>("LastVersion").unwrap(), "2.7.0");
        assert!(new.get_value::<u32, _>("UpdateCheck").is_err());
    }

    #[test]
    fn migrate_settings_from_empty_key() {
        // 1.0.0 kept no settings: nothing to copy, nothing fails
        let (_key, old, new) = TestKey::new("empty");
        assert!(!super::migrate_settings(&old, &new).unwrap());
        assert_eq!(new.enum_values().count(), 0);
    }

    #[test]
    fn old_and_new_names_differ() {
        // removing the old install must never touch the new one
        assert_ne!(super::old_install_dir(), super::install_dir());
        assert_ne!(super::old_startup_lnk(), super::startup_lnk());
        assert_ne!(super::old_update_temp_dir(), super::update_temp_dir());
        assert!(super::old_install_dir().ends_with("KakaoAdHide") && super::install_dir().ends_with("TalkNap"));
        assert_ne!(super::OLD_APP_KEY, super::APP_KEY);
        assert_ne!(super::OLD_UNINSTALL_KEY, super::UNINSTALL_KEY);
        assert_ne!(super::app_id_key(super::OLD_NOTIFY_APP_ID), super::app_id_key(super::NOTIFY_APP_ID));
    }

    /// Touches the real certificate store: run by hand (`cargo test -- --ignored old_certificates`) only on a PC
    /// with a throwaway old-CN test certificate in CurrentUser\Root, never where the real install is.
    #[test]
    #[ignore]
    fn old_certificates() {
        assert!(super::old_certificates_present());
        super::remove_old_certificates().unwrap();
        assert!(!super::old_certificates_present());
    }
}
