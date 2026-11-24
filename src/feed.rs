//! The update feed (feed-windows.json / feed-macos.json, from 2.6.0) and its verification.
//!
//! Copies before 2.6.0 read update-<platform>.json (2.5.2) or latest-<platform>.json (before 2.5.2) instead. Those
//! files stay pinned to the bridge releases (packaging/bridge/), so those copies first update to the bridge and
//! then follow this feed.
//!
//! See docs/update-feed.md:
//!   - the download link must point into this repository's releases,
//!   - size and SHA-256 must match the feed,
//!   - the feed's Ed25519 signature (`signature_ed25519`) over "TalkNap\n<version>\n<sha256 hex>" (UTF-8)
//!     must verify with one of the public keys in keys.rs.
//!

use base64::Engine;
use serde::Deserialize;
use std::cmp::Ordering;
use std::fmt;
use std::io::Read;
use std::path::Path;

pub const REPO: &str = "pandemoniummm/TalkNap";
/// Where release feeds point their downloads (tools/feed-sign): the repository under its name TalkNap.
pub const ASSET_PREFIX: &str = "https://github.com/pandemoniummm/TalkNap/releases/download/";
/// The repository's old name. The bridge feeds (update-*.json, latest-*.json) keep pointing here because the copies
/// before 2.6.0 accept only this prefix (GitHub redirects it), and this copy still accepts it.
pub const OLD_ASSET_PREFIX: &str = "https://github.com/pandemoniummm/KakaoAdHide/releases/download/";
/// Download prefixes this copy accepts: the repository under its new name and under its old name.
pub const ASSET_PREFIXES: &[&str] = &[ASSET_PREFIX, OLD_ASSET_PREFIX];
pub const RELEASES_PAGE: &str = "https://github.com/pandemoniummm/TalkNap/releases/latest";

pub fn feed_url(platform: &str) -> String {
    format!("https://github.com/{REPO}/releases/latest/download/feed-{platform}.json")
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Feed {
    pub version: String,
    pub url: String,
    #[serde(default)]
    pub size: u64,
    pub sha256: String,
    /// Ed25519 over `signed_message`, base64.
    #[serde(default)]
    pub signature_ed25519: String,
    #[serde(default)]
    pub page: Option<String>,
    /// The bottom banner's ad placement ids, signed on their own (`units_message`, Ed25519): KakaoTalk may change
    /// the id without a new release (#10). Copies before 2.5.0 ignore the fields.
    #[serde(default)]
    pub units: Vec<String>,
    #[serde(default)]
    pub units_signature: Option<String>,
}

#[derive(Debug, PartialEq)]
pub enum VerifyError {
    BadFeed,
    OutsideRepository,
    TooLarge,
    SizeMismatch,
    HashMismatch,
    BadSignature,
    NoKeys,
    BadPackage(String),
}

impl fmt::Display for VerifyError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            VerifyError::BadFeed => write!(f, "업데이트 정보 형식이 올바르지 않습니다."),
            VerifyError::OutsideRepository => write!(f, "다운로드 주소가 이 저장소의 릴리스가 아닙니다."),
            VerifyError::TooLarge => write!(f, "파일이 너무 큽니다."),
            VerifyError::SizeMismatch => write!(f, "파일 크기가 맞지 않습니다."),
            VerifyError::HashMismatch => write!(f, "SHA-256이 맞지 않습니다."),
            VerifyError::BadSignature => write!(f, "서명이 맞지 않습니다."),
            VerifyError::NoKeys => write!(f, "공개키가 없습니다."),
            VerifyError::BadPackage(why) => write!(f, "설치 파일을 확인하지 못했습니다: {why}"),
        }
    }
}

impl Feed {
    pub fn parse(data: &[u8]) -> Result<Feed, VerifyError> {
        let data = data.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(data);   // UTF-8 BOM (PowerShell may write one)
        let f: Feed = serde_json::from_slice(data).map_err(|_| VerifyError::BadFeed)?;
        if Version::parse(&f.version).is_none() || f.url.is_empty() || f.sha256.is_empty() || f.signature_ed25519.is_empty() {
            return Err(VerifyError::BadFeed);
        }
        Ok(f)
    }

    pub fn page_url(&self) -> String { self.page.clone().unwrap_or_else(|| RELEASES_PAGE.to_string()) }

    pub fn check_url(&self) -> Result<(), VerifyError> {
        let inside = ASSET_PREFIXES.iter().any(|p| self.url.starts_with(p) && !self.url[p.len()..].contains(".."));
        if inside { Ok(()) } else { Err(VerifyError::OutsideRepository) }
    }
}

/// x.y.z (two to four parts), compared numerically.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version(Vec<u64>);

impl Version {
    pub fn parse(s: &str) -> Option<Version> {
        let parts: Option<Vec<u64>> = s.split('.').map(|p| p.parse().ok()).collect();
        let mut p = parts?;
        if !(2..=4).contains(&p.len()) { return None; }
        p.resize(4, 0);
        Some(Version(p))
    }
}
impl PartialOrd for Version { fn partial_cmp(&self, o: &Self) -> Option<Ordering> { Some(self.cmp(o)) } }
impl Ord for Version { fn cmp(&self, o: &Self) -> Ordering { self.0.cmp(&o.0) } }

pub fn signed_message(version: &str, sha256_hex: &str) -> Vec<u8> {
    format!("TalkNap\n{version}\n{}", sha256_hex.to_ascii_lowercase()).into_bytes()
}

/// What `units_signature` signs: "TalkNap-units\n<platform>\n<id>\n<id>…" (UTF-8). The platform is in it so
/// one platform's list can't be passed off as the other's.
pub fn units_message(platform: &str, units: &[String]) -> Vec<u8> {
    let mut m = format!("TalkNap-units\n{platform}");
    for u in units { m.push('\n'); m.push_str(u); }
    m.into_bytes()
}

/// An ad placement id as KakaoTalk sends it: "DAN-" and letters or digits.
pub fn unit_ok(u: &str) -> bool {
    u.len() <= 64 && u.strip_prefix("DAN-").is_some_and(|r| !r.is_empty() && r.bytes().all(|b| b.is_ascii_alphanumeric()))
}

/// The feed's placement ids for `platform` when they're well formed and their signature verifies with one of
/// `keys`; none otherwise.
pub fn verified_units(feed: &Feed, platform: &str, keys: &[&str]) -> Option<Vec<String>> {
    let sig = feed.units_signature.as_deref()?;
    if feed.units.is_empty() || feed.units.len() > 20 || !feed.units.iter().all(|u| unit_ok(u)) { return None; }
    units_signature_valid(platform, &feed.units, sig, keys).then(|| feed.units.clone())
}

pub fn units_signature_valid(platform: &str, units: &[String], signature: &str, keys: &[&str]) -> bool {
    ed25519_valid(&units_message(platform, units), signature, keys)
}

/// True when `signature` (base64) over the version and hash verifies with any of `keys`.
pub fn signature_valid(version: &str, sha256_hex: &str, signature: &str, keys: &[&str]) -> bool {
    ed25519_valid(&signed_message(version, sha256_hex), signature, keys)
}

/// Ed25519 `signature` (base64) over `msg`, with any of `keys` (base64, 32 bytes).
pub fn ed25519_valid(msg: &[u8], signature: &str, keys: &[&str]) -> bool {
    let Ok(sig) = base64::engine::general_purpose::STANDARD.decode(signature.trim()) else { return false };
    keys.iter()
        .filter_map(|k| base64::engine::general_purpose::STANDARD.decode(k).ok().filter(|k| k.len() == 32))
        .any(|k| ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, k).verify(msg, &sig).is_ok())
}

pub fn sha256_hex_reader(mut r: impl Read) -> std::io::Result<(String, u64)> {
    let mut ctx = ring::digest::Context::new(&ring::digest::SHA256);
    let mut buf = vec![0u8; 1 << 16];
    let mut size = 0u64;
    loop {
        let n = r.read(&mut buf)?;
        if n == 0 { break; }
        ctx.update(&buf[..n]);
        size += n as u64;
    }
    let hex = ctx.finish().as_ref().iter().map(|b| format!("{b:02x}")).collect();
    Ok((hex, size))
}

/// Checks that the file is exactly what the signed feed describes.
pub fn verify_file(feed: &Feed, path: &Path, keys: &[&str]) -> Result<(), VerifyError> {
    if keys.is_empty() { return Err(VerifyError::NoKeys); }
    let f = std::fs::File::open(path).map_err(|e| VerifyError::BadPackage(e.to_string()))?;
    let (hex, size) = sha256_hex_reader(f).map_err(|e| VerifyError::BadPackage(e.to_string()))?;
    if feed.size != 0 && feed.size != size { return Err(VerifyError::SizeMismatch); }
    if hex != feed.sha256.to_ascii_lowercase() { return Err(VerifyError::HashMismatch); }
    if !signature_valid(&feed.version, &hex, &feed.signature_ed25519, keys) { return Err(VerifyError::BadSignature); }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::PUBLIC_KEYS;
    use base64::engine::general_purpose::STANDARD as B64;
    use ring::signature::{Ed25519KeyPair, KeyPair};

    // A throwaway test key (not a release key).
    fn test_key() -> (Ed25519KeyPair, String) {
        let k = Ed25519KeyPair::from_seed_unchecked(&[7; 32]).unwrap();
        let public = B64.encode(k.public_key().as_ref());
        (k, public)
    }

    const SHA: &str = "3f1c0e6a9b4d2c7e8f0a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60";

    fn signed_feed() -> (Feed, String) {
        let (k, public) = test_key();
        let sig = B64.encode(k.sign(&signed_message("2.5.1", SHA)).as_ref());
        let json = format!(r#"{{"version":"2.5.1","url":"{ASSET_PREFIX}v2.5.1/TalkNap.exe","size":1,"sha256":"{SHA}","signature":"rsa","signature_ed25519":"{sig}"}}"#);
        (Feed::parse(json.as_bytes()).unwrap(), public)
    }

    #[test]
    fn signature_verifies() {
        let (f, public) = signed_feed();
        let keys = [public.as_str()];
        assert!(signature_valid(&f.version, &f.sha256, &f.signature_ed25519, &keys));
        assert!(signature_valid(&f.version, &f.sha256.to_uppercase(), &f.signature_ed25519, &keys));
        assert!(f.check_url().is_ok());
    }

    #[test]
    fn tampering_fails() {
        let (f, public) = signed_feed();
        let keys = [public.as_str()];
        assert!(!signature_valid("2.5.2", &f.sha256, &f.signature_ed25519, &keys));
        let mut sha: Vec<char> = f.sha256.chars().collect();
        sha[0] = if sha[0] == '0' { '1' } else { '0' };
        assert!(!signature_valid(&f.version, &sha.iter().collect::<String>(), &f.signature_ed25519, &keys));
        assert!(!signature_valid(&f.version, &f.sha256, "not base64!", &keys));
        assert!(!signature_valid(&f.version, &f.sha256, &f.signature_ed25519, &[]));
        assert!(!signature_valid(&f.version, &f.sha256, &f.signature_ed25519, PUBLIC_KEYS));   // another key
    }

    #[test]
    fn urls() {
        let (mut f, _) = signed_feed();
        f.url = "https://github.com/someone/TalkNap/releases/download/v1/x.exe".into();
        assert_eq!(f.check_url(), Err(VerifyError::OutsideRepository));
        f.url = format!("{ASSET_PREFIX}../../evil");
        assert_eq!(f.check_url(), Err(VerifyError::OutsideRepository));
        // the repository under its new name (TalkNap) is accepted too
        f.url = "https://github.com/pandemoniummm/TalkNap/releases/download/v2.6.0/KakaoAdHide-windows.exe".into();
        assert_eq!(f.check_url(), Ok(()));
        f.url = format!("{OLD_ASSET_PREFIX}v2.6.0/KakaoAdHide-windows.exe");
        assert_eq!(f.check_url(), Ok(()));
        f.url = format!("{ASSET_PREFIX}v2.7.0/TalkNap.exe");
        assert_eq!(f.check_url(), Ok(()));
        assert_eq!(ASSET_PREFIX, "https://github.com/pandemoniummm/TalkNap/releases/download/");
        assert_eq!(ASSET_PREFIXES, [ASSET_PREFIX, OLD_ASSET_PREFIX]);
        f.url = "https://github.com/pandemoniummm/TalkNap/releases/download/../../evil".into();
        assert_eq!(f.check_url(), Err(VerifyError::OutsideRepository));
        f.url = "https://github.com/pandemoniummm/TalkNapX/releases/download/v1/x.exe".into();
        assert_eq!(f.check_url(), Err(VerifyError::OutsideRepository));
        // the 2.6.0+ feed; update-*.json stays pinned for 2.5.2
        assert_eq!(feed_url("macos"), "https://github.com/pandemoniummm/TalkNap/releases/latest/download/feed-macos.json");
    }

    #[test]
    fn messages_use_the_new_name() {
        assert_eq!(signed_message("2.6.0", "AB"), b"TalkNap\n2.6.0\nab".to_vec());
        assert_eq!(units_message("macos", &["DAN-a".into()]), b"TalkNap-units\nmacos\nDAN-a".to_vec());
        assert_eq!(feed_url("windows"), "https://github.com/pandemoniummm/TalkNap/releases/latest/download/feed-windows.json");
    }

    #[test]
    fn old_name_signatures_fail() {
        let (k, public) = test_key();
        let keys = [public.as_str()];
        let old = B64.encode(k.sign(format!("KakaoAdHide\n2.6.0\n{SHA}").as_bytes()).as_ref());
        assert!(!signature_valid("2.6.0", SHA, &old, &keys));
        let units = vec!["DAN-a".to_string()];
        let old_units = B64.encode(k.sign(b"KakaoAdHide-units\nwindows\nDAN-a").as_ref());
        assert!(!units_signature_valid("windows", &units, &old_units, &keys));
    }

    #[test]
    fn parsing() {
        assert_eq!(Feed::parse(b"{}"), Err(VerifyError::BadFeed));
        let bad = br#"{"version":"x","url":"u","size":1,"sha256":"a","signature_ed25519":"b"}"#;
        assert_eq!(Feed::parse(bad), Err(VerifyError::BadFeed));
        let mut bom = vec![0xEF, 0xBB, 0xBF];
        bom.extend_from_slice(br#"{"version":"1.2.3","url":"u","size":1,"sha256":"a","signature_ed25519":"b"}"#);
        assert_eq!(Feed::parse(&bom).unwrap().version, "1.2.3");
        // a feed without an Ed25519 signature (as before 2.5.1) isn't one this app can check
        assert_eq!(Feed::parse(br#"{"version":"1.0.0","url":"u","size":1,"sha256":"a","signature":"b"}"#), Err(VerifyError::BadFeed));
    }

    #[test]
    fn versions() {
        let v = |s| Version::parse(s).unwrap();
        assert!(v("1.0.10") > v("1.0.9"));
        assert_eq!(v("1.1"), v("1.1.0"));
        assert!(v("2.0.0") > v("1.99.99"));
        assert!(Version::parse("1.0.0-beta").is_none());
    }

    #[test]
    fn units() {
        let (k, public) = test_key();
        let keys = [public.as_str()];
        let (mut f, _) = signed_feed();
        assert!(f.units.is_empty() && verified_units(&f, "windows", &keys).is_none());   // none in the feed
        f.units = vec!["DAN-test1".into(), "DAN-test2".into()];
        f.units_signature = Some(B64.encode(k.sign(&units_message("windows", &f.units)).as_ref()));
        assert_eq!(verified_units(&f, "windows", &keys), Some(f.units.clone()));
        assert!(verified_units(&f, "macos", &keys).is_none());   // the other platform's list
        assert!(verified_units(&f, "windows", PUBLIC_KEYS).is_none());   // another key
        let mut g = f.clone();
        g.units.push("DAN-test3".into());
        assert!(verified_units(&g, "windows", &keys).is_none());
        g.units = vec!["DAN-test2".into(), "DAN-test1".into()];
        assert!(verified_units(&g, "windows", &keys).is_none());
        assert!(unit_ok("DAN-FTsOAPU4yZ6oHLGy") && !unit_ok("DAN-") && !unit_ok("DAN-a b") && !unit_ok("x-1"));
        // the fields parse, and a feed without them still does
        let j = br#"{"version":"1.2.3","url":"u","sha256":"a","signature_ed25519":"b","units":["DAN-x"],"units_signature":"c"}"#;
        assert_eq!(Feed::parse(j).unwrap().units, vec!["DAN-x".to_string()]);
    }

    #[test]
    fn release_keys() {
        // a primary and a backup key, both Ed25519 public keys
        assert_eq!(PUBLIC_KEYS.len(), 2);
        assert_ne!(PUBLIC_KEYS[0], PUBLIC_KEYS[1]);
        assert!(PUBLIC_KEYS.iter().all(|k| B64.decode(k).map(|k| k.len() == 32).unwrap_or(false)));
    }
}
