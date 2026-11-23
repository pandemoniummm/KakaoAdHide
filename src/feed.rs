//! The update feed (update-windows.json / update-macos.json, from 2.5.2) and its verification.
//!
//! Copies before 2.5.2 read latest-<platform>.json instead. Every release after 2.5.2 carries the 2.5.2 release's
//! latest-*.json unchanged (packaging/bridge/), so those copies first update to 2.5.2 and then follow this feed.
//!
//! See docs/update-feed.md:
//!   - the download link must point into this repository's releases,
//!   - size and SHA-256 must match the feed,
//!   - the feed's Ed25519 signature (`signature_ed25519`) over "KakaoAdHide\n<version>\n<sha256 hex>" (UTF-8)
//!     must verify with one of the public keys in keys.rs.
//!

use base64::Engine;
use serde::Deserialize;
use std::cmp::Ordering;
use std::fmt;
use std::io::Read;
use std::path::Path;

pub const REPO: &str = "pandemoniummm/KakaoAdHide";
pub const ASSET_PREFIX: &str = "https://github.com/pandemoniummm/KakaoAdHide/releases/download/";
pub const RELEASES_PAGE: &str = "https://github.com/pandemoniummm/KakaoAdHide/releases/latest";

pub fn feed_url(platform: &str) -> String {
    format!("https://github.com/{REPO}/releases/latest/download/update-{platform}.json")
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
    /// the id without a new KakaoAdHide release (#10). Copies before 2.5.0 ignore the fields.
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
        if self.url.starts_with(ASSET_PREFIX) && !self.url[ASSET_PREFIX.len()..].contains("..") { Ok(()) }
        else { Err(VerifyError::OutsideRepository) }
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
    format!("KakaoAdHide\n{version}\n{}", sha256_hex.to_ascii_lowercase()).into_bytes()
}

/// What `units_signature` signs: "KakaoAdHide-units\n<platform>\n<id>\n<id>…" (UTF-8). The platform is in it so
/// one platform's list can't be passed off as the other's.
pub fn units_message(platform: &str, units: &[String]) -> Vec<u8> {
    let mut m = format!("KakaoAdHide-units\n{platform}");
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
fn ed25519_valid(msg: &[u8], signature: &str, keys: &[&str]) -> bool {
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
        let json = format!(r#"{{"version":"2.5.1","url":"{ASSET_PREFIX}v2.5.1/KakaoAdHide-windows.exe","size":1,"sha256":"{SHA}","signature":"rsa","signature_ed25519":"{sig}"}}"#);
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
        f.url = "https://github.com/someone/KakaoAdHide/releases/download/v1/x.exe".into();
        assert_eq!(f.check_url(), Err(VerifyError::OutsideRepository));
        f.url = format!("{ASSET_PREFIX}../../evil");
        assert_eq!(f.check_url(), Err(VerifyError::OutsideRepository));
        // the 2.5.2+ feed; latest-*.json stays the 2.5.2 bridge for older copies
        assert_eq!(feed_url("macos"), "https://github.com/pandemoniummm/KakaoAdHide/releases/latest/download/update-macos.json");
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
