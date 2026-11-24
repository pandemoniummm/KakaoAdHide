//! The certificate the local ad server presents for the ad hosts.
//!
//! One self-signed certificate per computer, made here on first setup:
//!   - valid only for the ad hosts (hosts::AD_HOSTS) and only for TLS servers,
//!   - not a CA: it can't be used to issue certificates for any other site,
//!   - its private key never leaves this computer (a file only this user can read, in the app's data folder).
//!
//! The user trusts it once (platform::trust_certificate). Even if its key leaked, it could only be used to show
//! something else in KakaoTalk's ad slot. Valid for 800 days (Apple's limit for locally trusted TLS certificates
//! is 825) and renewed 30 days before it expires, which asks the user to trust the new one.

use crate::hosts::AD_HOSTS;
use rcgen::{CertificateParams, DistinguishedName, DnType, ExtendedKeyUsagePurpose, IsCa, KeyPair, KeyUsagePurpose, SerialNumber};
use std::path::{Path, PathBuf};
use time::{Duration, OffsetDateTime};

pub const VALID_DAYS: i64 = 800;
pub const RENEW_DAYS: i64 = 30;
pub const COMMON_NAME: &str = "TalkNap local ad server";
/// The name the KakaoAdHide builds (before 2.7.0) gave their certificate.
pub const OLD_COMMON_NAME: &str = "KakaoAdHide local ad server";

pub struct Paths { pub pem: PathBuf, pub der: PathBuf, pub key: PathBuf, pub expiry: PathBuf }

pub fn paths(dir: &Path) -> Paths {
    Paths {
        pem: dir.join("ad-server.pem"),
        der: dir.join("ad-server.cer"),
        key: dir.join("ad-server.key"),
        expiry: dir.join("ad-server.expires"),
    }
}

pub struct Generated { pub cert_pem: String, pub cert_der: Vec<u8>, pub key_pem: String, pub not_after: OffsetDateTime }

pub fn generate(now: OffsetDateTime) -> Result<Generated, rcgen::Error> { generate_with_name(now, COMMON_NAME) }

fn generate_with_name(now: OffsetDateTime, cn: &str) -> Result<Generated, rcgen::Error> {
    let mut params = CertificateParams::new(AD_HOSTS.iter().map(|h| h.to_string()).collect::<Vec<_>>())?;
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, cn);
    params.distinguished_name = dn;
    params.is_ca = IsCa::ExplicitNoCa;
    params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
    params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    params.not_before = now - Duration::days(1);
    params.not_after = now + Duration::days(VALID_DAYS);
    let mut serial = [0u8; 16];
    ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut serial).map_err(|_| rcgen::Error::RingUnspecified)?;
    serial[0] &= 0x7F;
    params.serial_number = Some(SerialNumber::from_slice(&serial));
    let key = KeyPair::generate()?;   // ECDSA P-256
    let cert = params.self_signed(&key)?;
    Ok(Generated { cert_pem: cert.pem(), cert_der: cert.der().to_vec(), key_pem: key.serialize_pem(), not_after: params.not_after })
}

/// Writes a new certificate and key (the key readable by this user only).
pub fn create(dir: &Path) -> std::io::Result<Paths> {
    std::fs::create_dir_all(dir)?;
    let g = generate(OffsetDateTime::now_utc()).map_err(|e| std::io::Error::other(e.to_string()))?;
    let p = paths(dir);
    write_private(&p.key, g.key_pem.as_bytes())?;
    std::fs::write(&p.pem, &g.cert_pem)?;
    std::fs::write(&p.der, &g.cert_der)?;
    std::fs::write(&p.expiry, g.not_after.unix_timestamp().to_string())?;
    Ok(p)
}

#[cfg(unix)]
fn write_private(path: &Path, data: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let _ = std::fs::remove_file(path);
    let mut f = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(path)?;
    f.write_all(data)
}
#[cfg(not(unix))]
fn write_private(path: &Path, data: &[u8]) -> std::io::Result<()> {
    // %LOCALAPPDATA% is readable by this user (and administrators) only.
    std::fs::write(path, data)
}

pub fn exists(dir: &Path) -> bool {
    let p = paths(dir);
    p.pem.exists() && p.key.exists()
}

/// True when the saved certificate (ad-server.cer) was made by a KakaoAdHide build: its subject holds the old name.
pub fn is_old(dir: &Path) -> bool {
    let Ok(der) = std::fs::read(paths(dir).der) else { return false };
    let needle = OLD_COMMON_NAME.as_bytes();
    der.windows(needle.len()).any(|w| w == needle)
}

/// True when the certificate is missing or expires within RENEW_DAYS.
pub fn needs_renewal(dir: &Path) -> bool {
    let p = paths(dir);
    let Some(exp) = std::fs::read_to_string(&p.expiry).ok().and_then(|s| s.trim().parse::<i64>().ok()) else { return true };
    !exists(dir) || exp - OffsetDateTime::now_utc().unix_timestamp() < RENEW_DAYS * 86_400
}

pub fn remove(dir: &Path) {
    let p = paths(dir);
    for f in [p.pem, p.der, p.key, p.expiry] { let _ = std::fs::remove_file(f); }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_a_leaf_for_the_ad_hosts_only() {
        let g = generate(OffsetDateTime::now_utc()).unwrap();
        assert!(g.cert_pem.starts_with("-----BEGIN CERTIFICATE-----"));
        assert!(g.key_pem.contains("PRIVATE KEY"));
        let der = &g.cert_der;
        let has = |needle: &[u8]| der.windows(needle.len()).any(|w| w == needle);
        for h in AD_HOSTS { assert!(has(h.as_bytes())); }
        assert!(has(&[0x06, 0x08, 0x2B, 0x06, 0x01, 0x05, 0x05, 0x07, 0x03, 0x01]));   // EKU serverAuth OID
        // basicConstraints CA:FALSE is encoded as an empty SEQUENCE (or cA=false): never cA=TRUE
        assert!(!has(&[0x01, 0x01, 0xFF, 0x30]));
    }

    #[test]
    fn old_certificate_is_detected() {
        let dir = std::env::temp_dir().join(format!("kah-certs-old-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let now = OffsetDateTime::now_utc();
        let new = generate(now).unwrap();
        assert!(new.cert_der.windows(COMMON_NAME.len()).any(|w| w == COMMON_NAME.as_bytes()));
        assert_eq!(COMMON_NAME, "TalkNap local ad server");
        assert!(!is_old(&dir));   // no certificate file
        std::fs::write(paths(&dir).der, &new.cert_der).unwrap();
        assert!(!is_old(&dir));
        let old = generate_with_name(now, OLD_COMMON_NAME).unwrap();
        std::fs::write(paths(&dir).der, &old.cert_der).unwrap();
        assert!(is_old(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn create_and_renew() {
        let dir = std::env::temp_dir().join(format!("kah-certs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(needs_renewal(&dir));
        create(&dir).unwrap();
        assert!(exists(&dir) && !needs_renewal(&dir));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(paths(&dir).key).unwrap().permissions().mode() & 0o777, 0o600);
        }
        remove(&dir);
        assert!(!exists(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
