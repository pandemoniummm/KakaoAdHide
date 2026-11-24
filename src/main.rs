// KakaoAdHide - empties the ad banner of KakaoTalk (Windows, macOS) by answering its ad requests locally.
//
//   KakaoAdHide                 Windows, first run from anywhere: install (as 1.0.0 did), then run;
//                               from the install folder, or on macOS: run (tray / menu bar)
//   KakaoAdHide --run           what the login item runs (Windows: also starts KakaoTalk to the tray)
//   KakaoAdHide --uninstall     remove everything (Windows; on macOS use the menu)
//   KakaoAdHide --update        check for an update now and install it after verifying it
//   --quiet                     no message boxes (install / uninstall)
//   KakaoAdHide --verify-feed <feed-*.json> <package>     used by the release workflow (feed version = this one)
//   KakaoAdHide --verify-update <feed-*.json> <package>   the same checks for a release this version or newer
//   KakaoAdHide --version
#![cfg_attr(windows, windows_subsystem = "windows")]

use talknap::*;

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
    if let Some(i) = args.iter().position(|a| a == "--verify-update") {
        let code = match (args.get(i + 1), args.get(i + 2)) {
            (Some(f), Some(p)) => updater::verify_update_command(f, p),
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
                if !quiet { ui::error(&notices::failed(&e)); }
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
        platform::migrate_old();
        let Some(_lock) = platform::single_instance() else { return };
        ui::run(has("--run"));
    }
}

/// --update: 0 = update started, 2 = up to date, 3 = not verified, 1 = error.
fn update_now(quiet: bool) -> i32 {
    match updater::check() {
        Err(e) => { if !quiet { ui::error(&notices::check_failed(&e)); } 1 }
        Ok(updater::Check::UpToDate) => { if !quiet { ui::show(&notices::up_to_date()); } 2 }
        Ok(updater::Check::Available(f)) => match platform::install_update(&f) {
            Ok(()) => 0,
            Err(updater::InstallError::Verify(e)) => {
                eprintln!("update {} not verified: {e}", f.version);
                if !quiet && ui::ask(&notices::update_unverified()) { platform::open_url(&f.page_url()); }
                3
            }
            Err(e) => { if !quiet { ui::error(&notices::download_failed(&e.to_string())); } 1 }
        },
    }
}
