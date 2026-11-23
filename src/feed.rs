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
    /// The bottom banner's ad placement ids, signed on their own (`units_message`): KakaoTalk may change the id
    /// without a new KakaoAdHide release (#10). Not in 1.0.0's feeds; it ignores the fields.
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
    let sig = base64::engine::general_purpose::STANDARD.decode(sig.trim()).ok()?;
    let msg = units_message(platform, &feed.units);
    keys.iter().filter_map(|k| rsa_xml(k)).any(|(n, e)| {
        ring::signature::RsaPublicKeyComponents { n: &n, e: &e }
            .verify(&ring::signature::RSA_PKCS1_2048_8192_SHA256, &msg, &sig)
            .is_ok()
    })
    .then(|| feed.units.clone())
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

    // A throwaway test key (not a release key) signed "KakaoAdHide-units\nwindows\nDAN-test1\nDAN-test2".
    const TEST_KEY: &str = "<RSAKeyValue><Modulus>veBqECW8oOoaDehjk2Wi5Qn7t4yXy19dtDI+IEf+Dyfje7t4DL19nSzvxJYQ+DrAhaO2a+7+W1htkQ+UVcOwbrPyGgCDaZJWWD4qA7+qcwA2oyb7JoLJozTJv013g08tESeerOOe+xSjVFgygY/2aGuI6lx6aANrw/56K4ehCNOEmxFVzpFZ40dXyE/tv+ad79gSaSkBrHp/3V/kAqXkWAk9AqRlzPT3szCCP9jpwjybSG9CcYEd3SXQX8SeDeWXxZb3Gt1ftZ17oL9Vh3lkpW/C7zmrx7JfBrQg+QO7IzJE8YUNLb7vKn4Bsr2qmt31rj6ITyDScy5nspPAzzzhPQ==</Modulus><Exponent>AQAB</Exponent></RSAKeyValue>";
    const TEST_UNITS_SIG: &str = "s6WH21vEb6S7NWHhTgD7t5zkABQuuk3NXUYF57I0+kXu2wvI7QfahDsWsKvAAkoe386g0iaYPojONlHIelDEzJ48VF+2iWMp84sWOR6j0GKot7yaspLQLPVnrw/IegI7ANOaYb+1FZzCcnuTb8l8BhWN1MYF2YpfHLQIO+tecHFwny2FDIv/zmBDIV7OLgPW1mHck+vx4/x4dQI427zg9IKCmySwZLdwsL783cfZ+rBPHq3I0liqQqQrXq4GFmlPDYv0+Z9EKRm8Iwx9i7fcOrDNiVjPdYdgb7pyNrlIrkn2DgD8llml11WEd/jt/39rEwMIiSmh4N+7INnVnNYUiQ==";

    #[test]
    fn units() {
        let mut f = v100();
        assert!(f.units.is_empty() && verified_units(&f, "windows", &[TEST_KEY]).is_none());   // 1.0.0's feed: none
        f.units = vec!["DAN-test1".into(), "DAN-test2".into()];
        f.units_signature = Some(TEST_UNITS_SIG.into());
        assert_eq!(verified_units(&f, "windows", &[TEST_KEY]), Some(f.units.clone()));
        assert!(verified_units(&f, "macos", &[TEST_KEY]).is_none());   // the other platform's list
        assert!(verified_units(&f, "windows", PUBLIC_KEYS).is_none());   // another key
        let mut g = f.clone();
        g.units.push("DAN-test3".into());
        assert!(verified_units(&g, "windows", &[TEST_KEY]).is_none());
        g.units = vec!["DAN-test2".into(), "DAN-test1".into()];
        assert!(verified_units(&g, "windows", &[TEST_KEY]).is_none());
        assert!(unit_ok("DAN-FTsOAPU4yZ6oHLGy") && !unit_ok("DAN-") && !unit_ok("DAN-a b") && !unit_ok("x-1"));
        // the fields parse, and a feed without them still does
        let j = br#"{"version":"1.2.3","url":"u","sha256":"a","signature":"b","units":["DAN-x"],"units_signature":"c"}"#;
        assert_eq!(Feed::parse(j).unwrap().units, vec!["DAN-x".to_string()]);
    }

    #[test]
    fn keys_match_the_windows_1_0_0_keys() {
        // the same two keys as the published Windows 1.0.0 (their moduli are what the fixture verifies against)
        assert_eq!(PUBLIC_KEYS.len(), 2);
        assert!(PUBLIC_KEYS.iter().all(|k| rsa_xml(k).map(|(n, _)| n.len() == 384).unwrap_or(false)));   // RSA-3072
    }
}
