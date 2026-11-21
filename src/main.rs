// KakaoAdHide - empties the ad banner of KakaoTalk (Windows, macOS) by answering its ad requests locally.
//
//   KakaoAdHide                 Windows, first run from anywhere: install (as 1.0.0 did), then run;
//                               from the install folder, or on macOS: run (tray / menu bar)
//   KakaoAdHide --run           what the login item runs (Windows: also starts KakaoTalk to the tray)
//   KakaoAdHide --uninstall     remove everything (Windows; on macOS use the menu)
//   KakaoAdHide --update        check for an update now and install it after verifying it
//   --quiet                     no message boxes (install / uninstall)
//   KakaoAdHide --verify-feed <latest-*.json> <package>   used by the release workflow
//   KakaoAdHide --version
#![cfg_attr(windows, windows_subsystem = "windows")]

use kakaoadhide::*;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let has = |f: &str| args.iter().any(|a| a.eq_ignore_ascii_case(f));
    let quiet = has("--quiet");

    if has("--version") { println!("{VERSION}"); return; }
    if let Some(i) = args.iter().position(|a| a == "--verify-feed") {
        let code = match (args.get(i + 1), args.get(i + 2)) {
            (Some(f), Some(p)) => updater::verify_command(f, p),
            _ => 1,
        };
        std::process::exit(code);
    }
    #[cfg(windows)]
    {
        if has("--hosts-add") { std::process::exit(platform::hosts_elevated(true)); }
        if has("--hosts-remove") { std::process::exit(platform::hosts_elevated(false)); }
        if has("--uninstall") {
            let r = platform::uninstall(quiet);
            std::process::exit(if r.is_ok() { 0 } else { 1 });
        }
    }
    if has("--update") { std::process::exit(update_now(quiet)); }

    #[cfg(windows)]
    {
        if !has("--run") && !platform::is_installed_copy() {
            if let Err(e) = platform::install(quiet) {
                if !quiet { ui::message("KakaoAdHide", &format!("오류가 발생했습니다.\n\n{e}")); }
                std::process::exit(1);
            }
            return;
        }
        let Some(_instance) = platform::single_instance() else { return };
        platform::cleanup_update_temp();
        ui::run(has("--run"));
    }
    #[cfg(target_os = "macos")]
    {
        let _ = quiet;
        if platform::offer_move_to_applications() { return; }
        let Some(_lock) = platform::single_instance() else { return };
        ui::run(has("--run"));
    }
}

/// --update: 0 = update started, 2 = up to date, 3 = not verified, 1 = error.
fn update_now(quiet: bool) -> i32 {
    let say = |t: &str, m: &str| if !quiet { ui::message(t, m) };
    match updater::check() {
        Err(e) => { say("업데이트를 확인하지 못했습니다", &e); 1 }
        Ok(updater::Check::UpToDate) => { say("최신 버전입니다", &format!("KakaoAdHide {VERSION}")); 2 }
        Ok(updater::Check::Available(f)) => match platform::install_update(&f) {
            Ok(()) => 0,
            Err(updater::InstallError::Verify(e)) => {
                say("업데이트를 설치하지 않았습니다", &format!("새 버전({})을 검증하지 못했습니다.\n{e}\n\n릴리스 페이지에서 직접 받아 주세요.", f.version));
                if !quiet { platform::open_url(&f.page_url()); }
                3
            }
            Err(e) => { say("업데이트를 받지 못했습니다", &e.to_string()); 1 }
        },
    }
}
