//! Turning the local ad server on and off: the certificate (certs.rs), its trust and the hosts entries
//! (platform), and the server itself (server.rs).

use crate::{certs, hosts, platform, server};

#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    /// Everything in place and the server is answering.
    Serving,
    /// Setup not done (or undone): the user has to agree first.
    NotSetUp,
    /// Set up, but the server couldn't open port 443 (another program uses it).
    PortBusy(String),
    /// Serving, but the ad hosts don't resolve to this computer (hosts file changed, resolver cache, or the hosts
    /// got an address the hosts entries don't cover).
    NotRouted,
    /// Something failed; the message says what.
    Failed(String),
}

pub fn cert_dir() -> std::path::PathBuf { platform::data_dir().join("certs") }

/// What still has to be done before the server can work.
pub struct Missing { pub cert: bool, pub trust: bool, pub hosts: bool }

impl Missing {
    pub fn any(&self) -> bool { self.cert || self.trust || self.hosts }
}

pub fn missing() -> Missing {
    let dir = cert_dir();
    let cert = certs::needs_renewal(&dir);
    let trust = cert || !platform::certificate_trusted(&certs::paths(&dir));
    let hosts = !hosts::read().map(|t| hosts::is_applied(&t)).unwrap_or(false) || !routed();
    Missing { cert, trust, hosts }
}

/// Makes what is missing: a new certificate if needed, its trust (the system asks the user), the hosts entries
/// (asks for an administrator). Stops at the first step the user declines or that fails.
pub fn complete(m: &Missing) -> Result<(), String> {
    let dir = cert_dir();
    if m.cert {
        let old = certs::paths(&dir);
        if old.pem.exists() { platform::untrust_certificate(&old); }
        certs::create(&dir).map_err(|e| format!("인증서를 만들지 못했습니다: {e}"))?;
    }
    if m.cert || m.trust {
        platform::trust_certificate(&certs::paths(&dir))?;
    }
    if m.hosts {
        platform::set_hosts(true)?;
    }
    Ok(())
}

/// Whether every ad host resolves only to this computer (through the system resolver, as KakaoTalk does).
pub fn routed() -> bool {
    use std::net::ToSocketAddrs;
    hosts::AD_HOSTS.iter().all(|h| match (*h, 443).to_socket_addrs() {
        Ok(addrs) => { let a: Vec<_> = addrs.collect(); !a.is_empty() && a.iter().all(|x| x.ip().is_loopback()) }
        Err(_) => false,
    })
}

/// Starts the server with the current certificate.
pub fn serve() -> Status {
    let p = certs::paths(&cert_dir());
    let cfg = match server::tls_config(&p.pem, &p.key) {
        Ok(c) => c,
        Err(e) => return Status::Failed(format!("인증서를 읽지 못했습니다: {e}")),
    };
    match server::start(cfg, &server::listen_addresses(443)) {
        Ok(_) => if routed() { Status::Serving } else { Status::NotRouted },
        Err(e) => Status::PortBusy(e),
    }
}

/// Removes everything setup added (hosts entries need an administrator; the trust removal may ask too).
pub fn undo() -> Result<(), String> {
    let dir = cert_dir();
    let mut errors = Vec::new();
    if hosts::read().map(|t| t != hosts::remove(&t)).unwrap_or(false) {
        if let Err(e) = platform::set_hosts(false) { errors.push(e); }
    }
    let p = certs::paths(&dir);
    if p.pem.exists() { platform::untrust_certificate(&p); }
    certs::remove(&dir);
    if errors.is_empty() { Ok(()) } else { Err(errors.join("\n")) }
}

/// SHA-1 of the certificate (the thumbprint the platform tools use to find it).
pub fn sha1_hex(der: &[u8]) -> String {
    ring::digest::digest(&ring::digest::SHA1_FOR_LEGACY_USE_ONLY, der).as_ref().iter().map(|b| format!("{b:02X}")).collect()
}
