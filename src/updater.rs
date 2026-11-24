//! Update check and download (the install step is per platform).
//!
//! A minute after start and then once a day the latest release's feed-<platform>.json is read. Nothing is
//! downloaded until the user picks the update. Then the package is downloaded, checked against the signed feed
//! (feed.rs) and installed by the platform: Windows runs the new exe, which installs itself like KakaoAdHide
//! 1.0.0's did; macOS also checks the app's code signature and swaps the bundle (platform::install_update).
//! Requests carry only "User-Agent: TalkNap/<version>".
//!
//! The feed may also carry the bottom banner's ad placement ids, signed on their own; verified ones are kept in
//! the BannerUnits setting (banner::is_bottom_banner).
//!
//! Settings: UpdateCheck ("0" turns the check off), SkippedVersion, NotifiedVersion, LastVersion, BannerUnits.

use crate::feed::{self, Feed, Version, VerifyError};
use crate::{platform, VERSION};
use std::fmt;
use std::io::Read;
use std::path::Path;

pub const MAX_FEED: u64 = 64 * 1024;
pub const MAX_PACKAGE: u64 = 50 * 1024 * 1024;

#[derive(Debug)]
pub enum InstallError {
    Network(String),
    Verify(VerifyError),
    NotInApplications,
    Io(String),
}

impl fmt::Display for InstallError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            InstallError::Network(e) => write!(f, "받지 못했습니다: {e}"),
            InstallError::Verify(e) => write!(f, "{e}"),
            InstallError::NotInApplications => write!(f, "응용 프로그램 폴더 밖에 있거나 바꿀 권한이 없어서 자동으로 업데이트할 수 없습니다."),
            InstallError::Io(e) => write!(f, "{e}"),
        }
    }
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(60)))
        .user_agent(format!("TalkNap/{VERSION}"))
        .build()
        .into()
}

fn get_limited(url: &str, limit: u64) -> Result<Vec<u8>, String> {
    let mut resp = agent().get(url).call().map_err(|e| e.to_string())?;
    let mut data = Vec::new();
    resp.body_mut().as_reader().take(limit + 1).read_to_end(&mut data).map_err(|e| e.to_string())?;
    if data.len() as u64 > limit { return Err("too large".into()); }
    Ok(data)
}

pub enum Check { UpToDate, Available(Feed) }

/// Why the check failed: the update information couldn't be fetched, or what came isn't valid (already worded
/// for the user).
#[derive(Debug)]
pub enum CheckError { Network(String), Feed(String) }

impl CheckError {
    pub fn problem(&self) -> crate::notices::Problem {
        match self {
            CheckError::Network(e) => crate::notices::check_failed_offline(e),
            CheckError::Feed(e) => crate::notices::check_failed(e),
        }
    }
}

pub fn check() -> Result<Check, CheckError> {
    let data = get_limited(&feed::feed_url(platform::NAME), MAX_FEED).map_err(CheckError::Network)?;
    let f = Feed::parse(&data).map_err(|e| CheckError::Feed(e.to_string()))?;
    if let Some(units) = feed::verified_units(&f, platform::NAME, crate::keys::PUBLIC_KEYS) {
        platform::set(crate::banner::FEED_UNITS, Some(&units.join(",")));
    }
    let newer = match (Version::parse(&f.version), Version::parse(VERSION)) { (Some(a), Some(b)) => a > b, _ => false };
    Ok(if newer { Check::Available(f) } else { Check::UpToDate })
}

/// Downloads the feed's package to `dest` (size-limited, link checked first).
pub fn download(f: &Feed, dest: &Path) -> Result<(), InstallError> {
    f.check_url().map_err(InstallError::Verify)?;
    let data = get_limited(&f.url, MAX_PACKAGE).map_err(InstallError::Network)?;
    std::fs::write(dest, data).map_err(|e| InstallError::Io(e.to_string()))
}

pub fn enabled() -> bool { platform::get("UpdateCheck").as_deref() != Some("0") }
pub fn set_enabled(on: bool) { platform::set("UpdateCheck", if on { None } else { Some("0") }) }
pub fn skipped() -> Option<String> { platform::get("SkippedVersion") }
pub fn skip(v: &str) { platform::set("SkippedVersion", Some(v)) }
pub fn notified() -> Option<String> { platform::get("NotifiedVersion") }
pub fn set_notified(v: &str) { platform::set("NotifiedVersion", Some(v)) }

/// The version this copy replaced, if it was just updated to a newer one (not on a first install or a downgrade).
pub fn note_version() -> Option<String> {
    let last = platform::get("LastVersion");
    if last.as_deref() != Some(VERSION) { platform::set("LastVersion", Some(VERSION)); }
    last.filter(|l| upgraded_from(l))
}

fn upgraded_from(last: &str) -> bool {
    matches!((Version::parse(last), Version::parse(VERSION)), (Some(a), Some(b)) if a < b)
}

/// KakaoAdHide --verify-feed <feed.json> <package> : for the release workflow. The app being released checks its
/// own feed and package, so this also proves the feed's version is the version built into it.
/// Exit codes: 0 ok, 3 not verified, 4 version mismatch, 1 other error.
pub fn verify_command(feed_path: &str, package: &str) -> i32 { verify_with(feed_path, package, true) }

/// KakaoAdHide --verify-update <feed.json> <package> : the same checks, made by an installed (older) app on a
/// newer release, so the feed's version only has to be this version or newer.
/// Exit codes: 0 ok, 3 not verified, 4 version older (or unreadable), 1 other error.
pub fn verify_update_command(feed_path: &str, package: &str) -> i32 { verify_with(feed_path, package, false) }

/// The feed's version against this app's: equal when `exact`, else equal or newer (both strictly parsed).
fn version_ok(feed: &str, mine: &str, exact: bool) -> bool {
    if exact { return feed == mine; }
    matches!((Version::parse(feed), Version::parse(mine)), (Some(a), Some(b)) if a >= b)
}

fn verify_with(feed_path: &str, package: &str, exact: bool) -> i32 {
    let Ok(data) = std::fs::read(feed_path) else { return 1 };
    let Ok(f) = Feed::parse(&data) else { return 1 };
    if !version_ok(&f.version, VERSION, exact) { return 4; }
    // a list that's there must verify too (an unsigned or mis-signed one would just be ignored by the app)
    if (!f.units.is_empty() || f.units_signature.is_some())
        && feed::verified_units(&f, platform::NAME, crate::keys::PUBLIC_KEYS).is_none() {
        eprintln!("not verified: ad placement ids (units)");
        return 3;
    }
    match platform::verify_package(&f, Path::new(package)) {
        Ok(()) => 0,
        Err(e) => { eprintln!("not verified: {e}"); 3 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upgrade_notice_only_after_an_upgrade() {
        assert!(upgraded_from("0.0.1"));
        assert!(!upgraded_from(VERSION));
        assert!(!upgraded_from("999.0.0"));   // downgrade
        assert!(!upgraded_from("not a version"));
    }

    #[test]
    fn feed_version_rule() {
        assert!(version_ok("2.6.0", "2.6.0", true));
        assert!(!version_ok("2.6.1", "2.6.0", true));
        assert!(version_ok("2.6.0", "2.6.0", false));
        assert!(version_ok("2.6.1", "2.6.0", false));
        assert!(version_ok("3.0.0", "2.6.0", false));
        assert!(!version_ok("2.5.9", "2.6.0", false));
        assert!(!version_ok("2.7.0-beta", "2.6.0", false));
        assert!(!version_ok("", "2.6.0", false));
    }
}
