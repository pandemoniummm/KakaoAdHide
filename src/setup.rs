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

/// What still has to be done before the server can work. `old`: an old-name certificate is still trusted, or
/// `certs/` still holds the KakaoAdHide certificate (it goes to `certs/old/` and then away).
pub struct Missing {
    pub cert: bool,
    pub trust: bool,
    pub hosts: bool,
    pub old: bool,
    /// `certs/` holds a KakaoAdHide certificate: the new one is made beside it and swapped in once trusted.
    cert_old: bool,
    /// The hosts file still has the KakaoAdHide block.
    hosts_old: bool,
}

impl Missing {
    pub fn any(&self) -> bool { self.cert || self.trust || self.hosts || self.old }
}

pub fn missing() -> Missing {
    let dir = cert_dir();
    let cert = if certs::needs_renewal(&dir) { CertState::Missing }
        else if certs::is_old(&dir) { CertState::Old }
        else { CertState::Current };
    let trusted = cert != CertState::Missing && platform::certificate_trusted(&certs::paths(&dir));
    let text = hosts::read().unwrap_or_default();
    let hosts = if hosts::has_old_block(&text) { HostsState::Old }
        else if hosts::is_applied(&text) && routed() { HostsState::Current }
        else { HostsState::None };
    let old_left = platform::old_certificates_present();
    // `certs/old/` without an old-name certificate in the store is only a leftover: nothing to ask about.
    if !old_left { let _ = std::fs::remove_dir_all(dir.join("old")); }
    let s = steps(&SetupState { cert, trusted, hosts, old_left });
    Missing {
        cert: s.contains(&Step::NewCert),
        trust: s.contains(&Step::Trust),
        hosts: s.contains(&Step::Hosts),
        old: s.contains(&Step::RemoveOld),
        cert_old: cert == CertState::Old,
        hosts_old: hosts == HostsState::Old,
    }
}

/// Whether this is the move from the KakaoAdHide names (asked about first, and can be put off while the old
/// certificate and hosts block keep the ad slot empty).
pub fn is_moving(m: &Missing) -> bool { m.cert_old || m.hosts_old || m.old }

/// Makes what is missing, in order: a new certificate and its trust (the system asks the user), the hosts entries
/// (asks for an administrator; `hosts::add` drops the old block), then the old-name certificates. Stops at the
/// first step the user declines or that fails. A server already running (the move, served first with the old
/// certificate) is switched to the new certificate before the old ones are removed.
pub fn complete(m: &Missing) -> Result<(), String> {
    let dir = cert_dir();
    if m.cert && m.cert_old {
        replace_old_certificate(&dir)?;
    } else if m.cert {
        let old = certs::paths(&dir);
        if old.pem.exists() { platform::untrust_certificate(&old); }
        certs::create(&dir).map_err(|e| format!("인증서를 만들지 못했습니다: {e}"))?;
        platform::trust_certificate(&certs::paths(&dir))?;
    } else if m.trust {
        platform::trust_certificate(&certs::paths(&dir))?;
    }
    if m.cert {
        let p = certs::paths(&dir);
        server::reload_cert(&p.pem, &p.key).map_err(|e| format!("새 인증서로 바꾸지 못했습니다: {e}"))?;
    }
    if m.hosts {
        platform::set_hosts(true)?;
    }
    if m.old {
        platform::remove_old_certificates()?;
        let _ = std::fs::remove_dir_all(dir.join("old"));
    }
    Ok(())
}

/// Makes a certificate with the new name in `certs/new/` and asks to trust it. Only once it is trusted does it
/// take the place of the KakaoAdHide one (which goes to `certs/old/` until the old-name certificates are removed),
/// so the server's paths stay the same; if the user declines, the old one keeps serving.
fn replace_old_certificate(dir: &std::path::Path) -> Result<(), String> { replace_old_certificate_in(dir, &SystemStore) }

fn replace_old_certificate_in(dir: &std::path::Path, store: &dyn CertStore) -> Result<(), String> {
    let new_dir = dir.join("new");
    let left = certs::paths(&new_dir);
    if left.der.exists() { store.untrust(&left); }   // from an interrupted earlier try
    let _ = std::fs::remove_dir_all(&new_dir);
    certs::create(&new_dir).map_err(|e| format!("인증서를 만들지 못했습니다: {e}"))?;
    if let Err(e) = store.trust(&certs::paths(&new_dir)) {
        // Its files are about to go, and with them the only way to find it in the store again (macOS keeps it).
        store.forget_declined(&certs::paths(&new_dir));
        let _ = std::fs::remove_dir_all(&new_dir);
        return Err(e);
    }
    let moved = move_certificate(dir, &dir.join("old")).and_then(|_| move_certificate(&new_dir, dir));
    let _ = std::fs::remove_dir(&new_dir);
    moved.map_err(|e| format!("인증서를 옮기지 못했습니다: {e}"))
}

/// The certificate store as `replace_old_certificate` uses it (the tests stand in for the system's).
trait CertStore {
    fn trust(&self, p: &certs::Paths) -> Result<(), String>;
    fn untrust(&self, p: &certs::Paths);
    fn forget_declined(&self, p: &certs::Paths);
}

struct SystemStore;

impl CertStore for SystemStore {
    fn trust(&self, p: &certs::Paths) -> Result<(), String> { platform::trust_certificate(p) }
    fn untrust(&self, p: &certs::Paths) { platform::untrust_certificate(p) }
    fn forget_declined(&self, p: &certs::Paths) { platform::forget_declined_certificate(p) }
}

fn move_certificate(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    let (a, b) = (certs::paths(from), certs::paths(to));
    for (f, t) in [(a.pem, b.pem), (a.der, b.der), (a.key, b.key), (a.expiry, b.expiry)] {
        if f.exists() { std::fs::rename(f, t)?; }
    }
    Ok(())
}

/// Whether the server can run as things are: a certificate in `certs/` that is trusted, and the ad hosts routed
/// here (by our block or the KakaoAdHide one).
pub fn can_serve_now() -> bool {
    let dir = cert_dir();
    certs::exists(&dir) && platform::certificate_trusted(&certs::paths(&dir)) && routed()
}

/// Whether every ad host resolves only to this computer (through the system resolver, as KakaoTalk does).
pub fn routed() -> bool {
    use std::net::ToSocketAddrs;
    hosts::AD_HOSTS.iter().all(|h| match (*h, 443).to_socket_addrs() {
        Ok(addrs) => { let a: Vec<_> = addrs.collect(); !a.is_empty() && a.iter().all(|x| x.ip().is_loopback()) }
        Err(_) => false,
    })
}

/// Sends `server::note` and the server's lines to server.log in the data folder.
pub fn open_log() { server::set_log(Some(platform::data_dir().join("server.log"))); }

/// Starts the server with the current certificate.
pub fn serve() -> Status {
    let p = certs::paths(&cert_dir());
    let cfg = match server::tls_config(&p.pem, &p.key) {
        Ok(c) => c,
        Err(e) => return Status::Failed(format!("인증서를 읽지 못했습니다: {e}")),
    };
    open_log();
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
    for d in [dir.join("new"), dir.clone()] {
        let p = certs::paths(&d);
        if p.pem.exists() { platform::untrust_certificate(&p); }
    }
    certs::remove(&dir);
    let _ = std::fs::remove_dir_all(dir.join("new"));
    let _ = std::fs::remove_dir_all(dir.join("old"));
    platform::remove_leftover_certificates();
    if platform::old_certificates_present() {
        if let Err(e) = platform::remove_old_certificates() { errors.push(e); }
    }
    if errors.is_empty() { Ok(()) } else { Err(errors.join("\n")) }
}

/// SHA-1 of the certificate (the thumbprint the platform tools use to find it).
pub fn sha1_hex(der: &[u8]) -> String {
    ring::digest::digest(&ring::digest::SHA1_FOR_LEGACY_USE_ONLY, der).as_ref().iter().map(|b| format!("{b:02X}")).collect()
}

/// The certificate in `certs/`: none (or about to expire), one with the KakaoAdHide name, or one with ours.
#[derive(Debug, Clone, Copy, PartialEq)]
enum CertState { Missing, Old, Current }

/// Our hosts entries: none (or not working), the KakaoAdHide block (even next to ours), or ours and working.
#[derive(Debug, Clone, Copy, PartialEq)]
enum HostsState { None, Old, Current }

/// What `missing()` found; `old_left`: an old-name certificate is still in the store.
#[derive(Debug, Clone, Copy)]
struct SetupState { cert: CertState, trusted: bool, hosts: HostsState, old_left: bool }

#[derive(Debug, Clone, Copy, PartialEq)]
enum Step { NewCert, Trust, Hosts, RemoveOld }

/// The steps left, in the order `complete()` does them.
fn steps(s: &SetupState) -> Vec<Step> {
    let mut v = Vec::new();
    match s.cert {
        CertState::Missing | CertState::Old => v.extend([Step::NewCert, Step::Trust]),
        CertState::Current if !s.trusted => v.push(Step::Trust),
        CertState::Current => {}
    }
    if s.hosts != HostsState::Current { v.push(Step::Hosts); }
    if s.old_left || s.cert == CertState::Old { v.push(Step::RemoveOld); }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use CertState as C;
    use HostsState as H;
    use Step::*;

    fn state(cert: CertState, trusted: bool, hosts: HostsState, old_left: bool) -> SetupState {
        SetupState { cert, trusted, hosts, old_left }
    }

    #[test]
    fn migration_steps_all() {
        assert_eq!(steps(&state(C::Old, true, H::Old, true)), [NewCert, Trust, Hosts, RemoveOld]);
    }

    #[test]
    fn migration_steps_after_trust_declined() {
        // The new certificate was thrown away, so `certs/` still holds the old one: the same steps again.
        assert_eq!(steps(&state(C::Old, true, H::Old, true)), steps(&state(C::Old, true, H::Old, true)));
        assert_eq!(steps(&state(C::Old, true, H::Old, true)), [NewCert, Trust, Hosts, RemoveOld]);
    }

    #[test]
    fn migration_steps_after_hosts_declined() {
        assert_eq!(steps(&state(C::Current, true, H::Old, true)), [Hosts, RemoveOld]);
    }

    #[test]
    fn migration_steps_only_old_left() {
        assert_eq!(steps(&state(C::Current, true, H::Current, true)), [RemoveOld]);
    }

    #[test]
    fn fresh_install_steps() {
        assert_eq!(steps(&state(C::Missing, false, H::None, false)), [NewCert, Trust, Hosts]);
    }

    #[test]
    fn nothing_to_do() {
        assert_eq!(steps(&state(C::Current, true, H::Current, false)), []);
    }

    #[test]
    fn current_cert_untrusted_is_trusted_again() {
        assert_eq!(steps(&state(C::Current, false, H::Current, false)), [Trust]);
    }

    #[test]
    fn old_cert_no_longer_trusted_still_cleans_up() {
        // The old certificate was removed from the store by hand: it's still swapped, and `certs/old/` then removed.
        assert_eq!(steps(&state(C::Old, false, H::Current, false)), [NewCert, Trust, RemoveOld]);
    }

    #[test]
    fn renewal_is_not_a_move() {
        assert_eq!(steps(&state(C::Missing, true, H::Current, false)), [NewCert, Trust]);
    }

    /// Records what `replace_old_certificate_in` asks of the store: (call, SHA-1 of the certificate it names).
    struct FakeStore { trusts: bool, calls: std::cell::RefCell<Vec<(&'static str, String)>> }

    impl FakeStore {
        fn new(trusts: bool) -> Self { FakeStore { trusts, calls: Default::default() } }
        fn note(&self, call: &'static str, p: &certs::Paths) {
            self.calls.borrow_mut().push((call, std::fs::read(&p.der).map(|d| sha1_hex(&d)).unwrap_or_default()));
        }
        fn calls(&self) -> Vec<(&'static str, String)> { self.calls.borrow().clone() }
    }

    impl CertStore for FakeStore {
        fn trust(&self, p: &certs::Paths) -> Result<(), String> {
            self.note("trust", p);
            if self.trusts { Ok(()) } else { Err("declined".into()) }
        }
        fn untrust(&self, p: &certs::Paths) { self.note("untrust", p) }
        fn forget_declined(&self, p: &certs::Paths) { self.note("forget", p) }
    }

    fn temp_cert_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("kah-setup-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        certs::create(&dir).unwrap();
        dir
    }

    fn der_sha1(dir: &std::path::Path) -> String { sha1_hex(&std::fs::read(certs::paths(dir).der).unwrap()) }

    #[test]
    fn declined_new_certificate_is_removed_from_the_store() {
        let dir = temp_cert_dir("declined");
        let before = der_sha1(&dir);
        let store = FakeStore::new(false);
        assert!(replace_old_certificate_in(&dir, &store).is_err());
        let calls = store.calls();
        // The certificate it asked to trust is the one it then removes, while its files still existed.
        assert_eq!(calls.iter().map(|c| c.0).collect::<Vec<_>>(), ["trust", "forget"]);
        assert!(!calls[0].1.is_empty() && calls[0].1 == calls[1].1 && calls[0].1 != before);
        assert!(!dir.join("new").exists());
        assert_eq!(der_sha1(&dir), before);   // the old one keeps serving
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn trusted_new_certificate_takes_the_old_ones_place() {
        let dir = temp_cert_dir("trusted");
        let before = der_sha1(&dir);
        let store = FakeStore::new(true);
        assert!(replace_old_certificate_in(&dir, &store).is_ok());
        let calls = store.calls();
        assert_eq!(calls.iter().map(|c| c.0).collect::<Vec<_>>(), ["trust"]);
        assert_eq!(der_sha1(&dir), calls[0].1);
        assert_eq!(der_sha1(&dir.join("old")), before);
        assert!(!dir.join("new").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn certificate_left_from_an_interrupted_try_is_untrusted_first() {
        let dir = temp_cert_dir("interrupted");
        certs::create(&dir.join("new")).unwrap();
        let left = der_sha1(&dir.join("new"));
        let store = FakeStore::new(false);
        let _ = replace_old_certificate_in(&dir, &store);
        let calls = store.calls();
        assert_eq!(calls.iter().map(|c| c.0).collect::<Vec<_>>(), ["untrust", "trust", "forget"]);
        assert_eq!(calls[0].1, left);
        assert_ne!(calls[1].1, left);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
