//! Update check and download (the install step is per platform).
//!
//! A minute after start and then once a day the latest release's latest-<platform>.json is read. Nothing is
//! downloaded until the user picks the update. Then the package is downloaded, checked against the signed feed
//! (feed.rs) and installed by the platform: Windows runs the new exe, which installs itself like KakaoAdHide
//! 1.0.0's did; macOS also checks the app's code signature and swaps the bundle (platform::install_update).
//! Requests carry only "User-Agent: KakaoAdHide/<version>".
//!
//! Settings: UpdateCheck ("0" turns the check off), SkippedVersion, NotifiedVersion, LastVersion.

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
        .user_agent(format!("KakaoAdHide/{VERSION}"))
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

pub fn check() -> Result<Check, String> {
    let data = get_limited(&feed::feed_url(platform::NAME), MAX_FEED)?;
    let f = Feed::parse(&data).map_err(|e| e.to_string())?;
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
pub fn verify_command(feed_path: &str, package: &str) -> i32 {
    let Ok(data) = std::fs::read(feed_path) else { return 1 };
    let Ok(f) = Feed::parse(&data) else { return 1 };
    if f.version != VERSION { return 4; }
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
}
