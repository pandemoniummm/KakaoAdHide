//! Windows: installed like KakaoAdHide 1.0.0 (these must not change, or 1.0.0 copies can't update into this one):
//!   - %LOCALAPPDATA%\KakaoAdHide\KakaoAdHide.exe, a Startup shortcut running it with --run, an "Installed apps"
//!     entry (HKCU\...\Uninstall\KakaoAdHide), settings in HKCU\Software\KakaoAdHide,
//!   - mutex Local\KakaoAdHide (one instance), event Local\KakaoAdHide.Quit (asks a running copy to exit),
//!   - --run, --uninstall, --update, --quiet, --verify-feed,
//!   - KakaoTalk's own auto start is left as the user set it (see `release_kakao_autostart`).
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
pub const APP: &str = "KakaoAdHide";
const MUTEX: &str = r"Local\KakaoAdHide";
const QUIT_EVENT: &str = r"Local\KakaoAdHide.Quit";
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const APP_KEY: &str = r"Software\KakaoAdHide";
const UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\KakaoAdHide";
const KAKAO_RUN_VALUE: &str = "KakaoTalk";

fn wide(s: &str) -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() }
fn env_dir(var: &str) -> PathBuf { PathBuf::from(std::env::var_os(var).unwrap_or_default()) }

pub fn install_dir() -> PathBuf { env_dir("LOCALAPPDATA").join(APP) }
pub fn installed_exe() -> PathBuf { install_dir().join(format!("{APP}.exe")) }
fn startup_lnk() -> PathBuf { env_dir("APPDATA").join(r"Microsoft\Windows\Start Menu\Programs\Startup").join(format!("{APP}.lnk")) }
pub fn update_temp_dir() -> PathBuf { std::env::temp_dir().join("KakaoAdHide-update") }

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

/// Owns Local\KakaoAdHide for the life of the process; None when another copy runs.
pub fn single_instance() -> Option<Instance> {
    let name = wide(MUTEX);
    let m = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
    if m.is_null() { return None; }
    let r = unsafe { WaitForSingleObject(m, 0) };
    if r == WAIT_OBJECT_0 || r == WAIT_ABANDONED { Some(Instance { mutex: m }) } else { unsafe { CloseHandle(m) }; None }
}

/// Calls `on_quit` (from a background thread) when another copy signals Local\KakaoAdHide.Quit.
/// Makes our hidden menu window the foreground window before a menu opens on it, so the menu takes the keyboard
/// and closes when clicking elsewhere. The click came from KakaoTalk's window, and Windows only lets the
/// foreground thread hand the foreground over, so attach to its input for the call.
pub fn take_foreground(hwnd: isize) {
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
    }
}

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

/// Asks a running copy to exit and waits until it has (as 1.0.0's installer did).
pub fn stop_running() {
    let name = wide(QUIT_EVENT);
    let ev = unsafe { OpenEventW(EVENT_MODIFY_STATE, 0, name.as_ptr()) };
    if !ev.is_null() { unsafe { SetEvent(ev); CloseHandle(ev); } }
    let mname = wide(MUTEX);
    let m = unsafe { CreateMutexW(std::ptr::null(), 0, mname.as_ptr()) };
    if !m.is_null() {
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
    // Over an existing install (a newer exe run by hand) this is an update: no "installed" message, the new copy
    // says it was updated when it starts (ui: updater::note_version).
    let existing = installed_exe().exists();
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
    let mut lnk = mslnk::ShellLink::new(installed_exe()).map_err(|e| e.to_string())?;
    lnk.set_arguments(Some("--run".into()));
    lnk.set_working_dir(Some(install_dir().display().to_string()));
    lnk.set_name(Some("KakaoTalk with the ad banner emptied".into()));
    lnk.create_lnk(startup_lnk()).map_err(|e| e.to_string())?;
    release_kakao_autostart();

    let (k, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(UNINSTALL_KEY).map_err(|e| e.to_string())?;
    let exe = installed_exe().display().to_string();
    let _ = k.set_value("DisplayName", &"KakaoAdHide (카카오톡 PC 광고 숨김)".to_string());
    let _ = k.set_value("DisplayVersion", &VERSION.to_string());
    let _ = k.set_value("Publisher", &"KakaoAdHide".to_string());
    let _ = k.set_value("DisplayIcon", &exe);
    let _ = k.set_value("InstallLocation", &install_dir().display().to_string());
    let _ = k.set_value("UninstallString", &format!("\"{exe}\" --uninstall"));
    let _ = k.set_value("QuietUninstallString", &format!("\"{exe}\" --uninstall --quiet"));
    let _ = k.set_value("NoModify", &1u32);   // u32 is written as REG_DWORD
    let _ = k.set_value("NoRepair", &1u32);
    let size_kb = std::fs::metadata(installed_exe()).map(|m| (m.len() / 1024) as u32).unwrap_or(0);
    let _ = k.set_value("EstimatedSize", &size_kb);

    if !quiet && !existing {
        crate::ui::message("KakaoAdHide", "설치했습니다.\n\n이제 로그인할 때 KakaoAdHide도 함께 켜져 카카오톡 광고 칸을 비웁니다.\n카카오톡 빈 칸 왼쪽(사이드바 아래)의 KakaoAdHide 버튼으로 메뉴를 열 수 있습니다.\n\n제거: 설정 > 앱 > 설치된 앱 > KakaoAdHide");
    }
    Command::new(installed_exe()).current_dir(install_dir()).spawn().map_err(|e| e.to_string())?;
    Ok(())
}

pub fn uninstall(quiet: bool) -> Result<(), String> {
    stop_running();
    let undo = setup::undo();
    let _ = std::fs::remove_file(startup_lnk());
    restore_kakao_autostart();
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let _ = hkcu.delete_subkey_all(UNINSTALL_KEY);
    let _ = hkcu.delete_subkey_all(APP_KEY);
    let _ = std::fs::remove_dir_all(update_temp_dir());
    let _ = std::fs::remove_dir_all(setup::cert_dir());
    for log in ["server.log", "server.log.old"] { let _ = std::fs::remove_file(data_dir().join(log)); }
    if !quiet {
        let extra = match &undo { Ok(()) => String::new(), Err(e) => format!("\n\n일부를 정리하지 못했습니다:\n{e}") };
        crate::ui::message("KakaoAdHide", &format!("제거했습니다.{extra}"));
    }
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

/// Whether a KakaoTalk.exe process runs (a process snapshot, no external program).
fn kakao_running() -> bool {
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
    };
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE { return false; }
        let mut e: PROCESSENTRY32W = std::mem::zeroed();
        e.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut found = false;
        let mut ok = Process32FirstW(snap, &mut e) != 0;
        while ok && !found {
            let len = e.szExeFile.iter().position(|&c| c == 0).unwrap_or(e.szExeFile.len());
            found = String::from_utf16_lossy(&e.szExeFile[..len]).eq_ignore_ascii_case("KakaoTalk.exe");
            ok = Process32NextW(snap, &mut e) != 0;
        }
        CloseHandle(snap);
        found
    }
}

/// When started by hand (after installing, or from the Start menu), also starts KakaoTalk unless it already runs.
/// At sign-in KakaoTalk starts (or not) by its own setting.
pub fn start_kakao(at_sign_in: bool) {
    release_kakao_autostart();
    if at_sign_in || kakao_running() { return; }
    if let Some(k) = find_kakao() { let _ = Command::new(k).spawn(); }
}

pub fn open_url(url: &str) {
    use std::os::windows::process::CommandExt;
    let _ = Command::new("rundll32").args(["url.dll,FileProtocolHandler", url]).creation_flags(0x0800_0000).spawn();
}

pub fn first_run() {}
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

/// Removes the installer an update left behind (it may still be exiting: retry a little).
pub fn cleanup_update_temp() {
    std::thread::spawn(|| {
        for _ in 0..10 {
            if !update_temp_dir().exists() || std::fs::remove_dir_all(update_temp_dir()).is_ok() { return; }
            std::thread::sleep(std::time::Duration::from_secs(2));
        }
    });
}

pub fn verify_package(f: &Feed, exe: &Path) -> Result<(), VerifyError> {
    feed::verify_file(f, exe, keys::PUBLIC_KEYS)
}
