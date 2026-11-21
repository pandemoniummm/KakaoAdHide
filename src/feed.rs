//! The update feed (latest-windows.json / latest-macos.json) and its verification.
//!
//! Same format and signature as KakaoAdHide 1.0.0 (see docs/update-feed.md):
//!   - the download link must point into this repository's releases,
//!   - size and SHA-256 must match the feed,
//!   - the feed's signature (RSA, PKCS#1 v1.5, SHA-256) over "KakaoAdHide\n<version>\n<sha256 hex>" (UTF-8) must
//!     verify with one of the public keys in keys.rs.

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
    format!("https://github.com/{REPO}/releases/latest/download/latest-{platform}.json")
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Feed {
    pub version: String,
    pub url: String,
    #[serde(default)]
    pub size: u64,
    pub sha256: String,
    pub signature: String,
    #[serde(default)]
    pub page: Option<String>,
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
        if Version::parse(&f.version).is_none() || f.url.is_empty() || f.sha256.is_empty() || f.signature.is_empty() {
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

/// .NET RSA XML public key (<RSAKeyValue><Modulus/><Exponent/></RSAKeyValue>) -> (n, e), big-endian.
fn rsa_xml(xml: &str) -> Option<(Vec<u8>, Vec<u8>)> {
    fn element(xml: &str, name: &str) -> Option<Vec<u8>> {
        let a = xml.find(&format!("<{name}>"))? + name.len() + 2;
        let b = xml[a..].find(&format!("</{name}>"))? + a;
        base64::engine::general_purpose::STANDARD.decode(xml[a..b].trim()).ok()
    }
    Some((element(xml, "Modulus")?, element(xml, "Exponent")?))
}

/// True when `signature` (base64) over the version and hash verifies with any of `keys`.
pub fn signature_valid(version: &str, sha256_hex: &str, signature: &str, keys: &[&str]) -> bool {
    let Ok(sig) = base64::engine::general_purpose::STANDARD.decode(signature.trim()) else { return false };
    let msg = signed_message(version, sha256_hex);
    keys.iter().filter_map(|k| rsa_xml(k)).any(|(n, e)| {
        ring::signature::RsaPublicKeyComponents { n: &n, e: &e }
            .verify(&ring::signature::RSA_PKCS1_2048_8192_SHA256, &msg, &sig)
            .is_ok()
    })
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
    if !signature_valid(&feed.version, &hex, &feed.signature, keys) { return Err(VerifyError::BadSignature); }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::PUBLIC_KEYS;

    fn v100() -> Feed { Feed::parse(include_bytes!("../tests/fixtures/v1.0.0-latest-windows.json")).unwrap() }

    // The published v1.0.0 feed was signed by the release workflow with the primary key.
    #[test]
    fn published_feed_verifies() {
        let f = v100();
        assert_eq!(f.version, "1.0.0");
        assert!(signature_valid(&f.version, &f.sha256, &f.signature, PUBLIC_KEYS));
        assert!(signature_valid(&f.version, &f.sha256.to_uppercase(), &f.signature, PUBLIC_KEYS));
        assert!(f.check_url().is_ok());
    }

    #[test]
    fn tampering_fails() {
        let f = v100();
        assert!(!signature_valid("1.0.1", &f.sha256, &f.signature, PUBLIC_KEYS));
        let mut sha: Vec<char> = f.sha256.chars().collect();
        sha[0] = if sha[0] == '0' { '1' } else { '0' };
        assert!(!signature_valid(&f.version, &sha.iter().collect::<String>(), &f.signature, PUBLIC_KEYS));
        assert!(!signature_valid(&f.version, &f.sha256, "not base64!", PUBLIC_KEYS));
        assert!(!signature_valid(&f.version, &f.sha256, &f.signature, &[]));
    }

    #[test]
    fn urls() {
        let mut f = v100();
        f.url = "https://github.com/someone/KakaoAdHide/releases/download/v1/x.exe".into();
        assert_eq!(f.check_url(), Err(VerifyError::OutsideRepository));
        f.url = format!("{ASSET_PREFIX}../../evil");
        assert_eq!(f.check_url(), Err(VerifyError::OutsideRepository));
    }

    #[test]
    fn parsing() {
        assert_eq!(Feed::parse(b"{}"), Err(VerifyError::BadFeed));
        let bad = br#"{"version":"x","url":"u","size":1,"sha256":"a","signature":"b"}"#;
        assert_eq!(Feed::parse(bad), Err(VerifyError::BadFeed));
        let mut bom = vec![0xEF, 0xBB, 0xBF];
        bom.extend_from_slice(br#"{"version":"1.2.3","url":"u","size":1,"sha256":"a","signature":"b"}"#);
        assert_eq!(Feed::parse(&bom).unwrap().version, "1.2.3");
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
    fn keys_match_the_windows_1_0_0_keys() {
        // the same two keys as the published Windows 1.0.0 (their moduli are what the fixture verifies against)
        assert_eq!(PUBLIC_KEYS.len(), 2);
        assert!(PUBLIC_KEYS.iter().all(|k| rsa_xml(k).map(|(n, _)| n.len() == 384).unwrap_or(false)));   // RSA-3072
    }
}
