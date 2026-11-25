//! The local ad server: HTTPS on port 443 for the ad hosts (sent here by hosts.rs), answering banner.rs.
//!
//! - Only connections from this computer are served; anything else is closed before the TLS handshake (on macOS
//!   the port can only be opened on all addresses without root, see `listen_addresses`).
//! - It speaks plain HTTP/1.1 over TLS, answers /sdk/banner with the blank ad and everything else with
//!   204 No Content, and never contacts the real ad servers.
//! - One short-lived thread per connection: KakaoTalk makes a handful of requests an hour.

use rustls::server::{ClientHello, ResolvesServerCert};
use rustls::sign::CertifiedKey;
use rustls::ServerConfig;
use rustls_pki_types::pem::PemObject;
use rustls_pki_types::{CertificateDer, PrivateKeyDer};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, RwLock};
use std::time::Duration;

/// The server's TLS settings. Its certificate comes from `CURRENT` (see `reload_cert`), so a running server can
/// switch to a new certificate without opening the port again.
pub fn tls_config(cert_pem: &Path, key_pem: &Path) -> Result<Arc<ServerConfig>, String> {
    let certs = Arc::new(CertSlot(RwLock::new(load_cert(cert_pem, key_pem)?)));
    let mut cfg = ServerConfig::builder_with_provider(provider())
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?
        .with_no_client_auth()
        .with_cert_resolver(certs.clone());
    cfg.alpn_protocols = vec![b"http/1.1".to_vec()];
    if let Ok(mut g) = CURRENT.lock() { *g = Some(certs); }
    Ok(Arc::new(cfg))
}

fn provider() -> Arc<rustls::crypto::CryptoProvider> { Arc::new(rustls::crypto::ring::default_provider()) }

/// The certificate chain and key from PEM files (the key checked against the certificate).
fn load_cert(cert_pem: &Path, key_pem: &Path) -> Result<Arc<CertifiedKey>, String> {
    let certs: Vec<CertificateDer<'static>> = CertificateDer::pem_file_iter(cert_pem)
        .map_err(|e| e.to_string())?
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())?;
    if certs.is_empty() { return Err("no certificate".into()); }
    let key = PrivateKeyDer::from_pem_file(key_pem).map_err(|e| e.to_string())?;
    CertifiedKey::from_der(certs, key, &provider()).map(Arc::new).map_err(|e| e.to_string())
}

/// The certificate the server presents, swapped by `reload_cert`.
#[derive(Debug)]
struct CertSlot(RwLock<Arc<CertifiedKey>>);

impl ResolvesServerCert for CertSlot {
    fn resolve(&self, _: ClientHello<'_>) -> Option<Arc<CertifiedKey>> { self.0.read().ok().map(|k| k.clone()) }
}

impl CertSlot {
    fn set(&self, k: Arc<CertifiedKey>) -> Result<(), String> {
        *self.0.write().map_err(|e| e.to_string())? = k;
        Ok(())
    }
}

/// The certificate of the last `tls_config` (the app makes one, for the server it starts).
static CURRENT: Mutex<Option<Arc<CertSlot>>> = Mutex::new(None);

/// Makes the server present the certificate in these files from the next connection on (setup swapped the
/// certificate while the server was running). Nothing to do when no server config was made yet: it reads the
/// files when it starts. An error leaves the certificate it had.
pub fn reload_cert(cert_pem: &Path, key_pem: &Path) -> Result<(), String> {
    let Some(slot) = CURRENT.lock().map_err(|e| e.to_string())?.clone() else { return Ok(()) };
    slot.set(load_cert(cert_pem, key_pem)?)
}

/// Where to listen. Windows: the loopback addresses. macOS: a normal user may open port 443 only on all
/// addresses (the wildcard), so the server listens there and refuses anyone but this computer.
pub fn listen_addresses(port: u16) -> Vec<SocketAddr> {
    if cfg!(target_os = "macos") {
        vec![SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port), SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), port)]
    } else {
        vec![SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port), SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), port)]
    }
}

pub struct Running { pub addresses: Vec<SocketAddr> }

/// Opens the listeners and serves on background threads. Fails if no address could be opened (port in use).
pub fn start(cfg: Arc<ServerConfig>, addrs: &[SocketAddr]) -> Result<Running, String> {
    let mut opened = Vec::new();
    let mut errors = Vec::new();
    for addr in addrs {
        let listener = match bind(*addr) {
            Ok(l) => l,
            Err(e) => { errors.push(format!("{addr}: {e}")); continue; }
        };
        opened.push(listener.local_addr().unwrap_or(*addr));
        let cfg = cfg.clone();
        std::thread::Builder::new().name("ad-server".into()).spawn(move || accept_loop(listener, cfg)).ok();
    }
    if opened.is_empty() { Err(errors.join("; ")) } else { Ok(Running { addresses: opened }) }
}

fn bind(addr: SocketAddr) -> std::io::Result<TcpListener> {
    // IPV6_V6ONLY is the default on macOS and Windows, so the v4 and v6 wildcards can both be bound.
    TcpListener::bind(addr)
}

fn accept_loop(listener: TcpListener, cfg: Arc<ServerConfig>) {
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let local = stream.peer_addr().map(|a| a.ip().is_loopback() || is_mapped_loopback(a.ip())).unwrap_or(false);
        if !local { continue; }   // dropped: closes the connection
        let cfg = cfg.clone();
        std::thread::Builder::new().name("ad-conn".into()).spawn(move || { let _ = serve(stream, cfg); }).ok();
    }
}

fn is_mapped_loopback(ip: IpAddr) -> bool {
    matches!(ip, IpAddr::V6(v6) if v6.to_ipv4_mapped().map(|v4| v4.is_loopback()).unwrap_or(false))
}

// A small connection log (server.log in the data folder) for diagnosing "the ad still shows": whether KakaoTalk
// connected, which host name it asked for, whether it accepted our certificate, and which paths it requested
// (without query strings). Capped at MAX_LOG bytes; the previous file is kept as server.log.old.
const MAX_LOG: u64 = 64 * 1024;
static LOG: Mutex<Option<PathBuf>> = Mutex::new(None);

pub fn set_log(path: Option<PathBuf>) {
    if let Ok(mut g) = LOG.lock() { *g = path; }
}

/// A line in server.log from elsewhere in the app (e.g. why an update didn't verify).
pub fn note(line: &str) { log(line) }

fn log(line: &str) {
    let Ok(guard) = LOG.lock() else { return };
    let Some(path) = guard.as_ref() else { return };
    if std::fs::metadata(path).map(|m| m.len() > MAX_LOG).unwrap_or(false) {
        let _ = std::fs::rename(path, path.with_extension("log.old"));
    }
    let t = time::OffsetDateTime::now_utc();
    let stamp = format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}Z", t.year(), u8::from(t.month()), t.day(), t.hour(), t.minute(), t.second());
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{stamp} {line}");
    }
}

fn serve(mut tcp: TcpStream, cfg: Arc<ServerConfig>) -> std::io::Result<()> {
    tcp.set_read_timeout(Some(Duration::from_secs(15)))?;
    tcp.set_write_timeout(Some(Duration::from_secs(15)))?;
    let peer = tcp.peer_addr().map(|a| a.to_string()).unwrap_or_else(|_| "?".into());
    let mut conn = rustls::ServerConnection::new(cfg).map_err(std::io::Error::other)?;
    // Handshake first, so a client that rejects our certificate shows up in the log with its reason.
    while conn.is_handshaking() {
        let result = match conn.complete_io(&mut tcp) {
            Ok((0, 0)) => Err(std::io::Error::from(std::io::ErrorKind::UnexpectedEof)),
            Ok(_) => Ok(()),
            Err(e) => Err(e),
        };
        if let Err(e) = result {
            log(&format!("{peer} TLS failed (sni={}): {e}", conn.server_name().unwrap_or("-")));
            return Err(e);
        }
    }
    let sni = conn.server_name().unwrap_or("-").to_string();
    log(&format!("{peer} TLS ok (sni={sni})"));
    let tls = rustls::StreamOwned::new(conn, &mut tcp);
    let mut reader = BufReader::new(tls);
    // keep-alive: a few requests per connection at most
    for _ in 0..32 {
        let Some(req) = read_request(&mut reader)? else { return Ok(()) };
        if is_menu_path(&req.path) { open_menu(&req.query); }
        if is_state_path(&req.path) {
            log(&format!("{peer} {sni} {} {}", req.method, req.path));
            return stream_state(reader.get_mut(), &req);
        }
        let (status, ctype, body) = respond(&req.method, &req.path, &req.query);
        // the ad placement (a fixed id per placement, nothing about the user) shows which slot asked
        let unit = if req.path == "/sdk/banner" { ad_unit(&req.query).map(|u| format!(" [{u}]")).unwrap_or_default() } else { String::new() };
        log(&format!("{peer} {sni} {} {}{unit} -> {status}", req.method, req.path));
        let head = format!(
            "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: keep-alive\r\n\r\n",
            body.len()
        );
        let w = reader.get_mut();
        w.write_all(head.as_bytes())?;
        if req.method != "HEAD" { w.write_all(body.as_bytes())?; }
        w.flush()?;
    }
    Ok(())
}

/// `origin`: the Origin header, for the state stream's CORS answer.
pub struct Request { pub method: String, pub path: String, pub query: String, pub origin: String }

// KakaoTalk's window state for the blank ad's menu button, which dims with KakaoTalk's own sidebar icons while
// KakaoTalk isn't the active app. The web view can't tell (it never has the focus), so the app follows the
// foreground window and the blank ad keeps one event stream open here (STATE_PATH?t=<menu token>), sent
// "active" or "inactive" whenever it changes. Nothing is polled, and it's still only this computer.
pub const STATE_PATH: &str = "/talknap/state";
/// The path blank ads made by KakaoAdHide builds (before 2.7.0) ask; KakaoTalk may still show one of those.
pub const OLD_STATE_PATH: &str = "/kakaoadhide/state";
fn is_state_path(path: &str) -> bool { path == STATE_PATH || path == OLD_STATE_PATH }
static STATE: Mutex<(bool, u64)> = Mutex::new((true, 0));   // (KakaoTalk active, change count)
static STATE_CHANGED: Condvar = Condvar::new();
static STREAMS: AtomicUsize = AtomicUsize::new(0);
const MAX_STREAMS: usize = 8;

/// Called by the platform code when KakaoTalk becomes or stops being the active app.
pub fn set_kakao_active(active: bool) {
    let Ok(mut s) = STATE.lock() else { return };
    if s.0 != active { *s = (active, s.1 + 1); STATE_CHANGED.notify_all(); }
}

/// The blank ad's width in points (macOS sends it with the state stream: `&w=`), 0 until known. It is the width
/// of KakaoTalk's main window, which the platform code looks for among KakaoTalk's windows.
static AD_WIDTH: AtomicUsize = AtomicUsize::new(0);

pub fn ad_width() -> usize { AD_WIDTH.load(Ordering::SeqCst) }

fn token_ok(query: &str) -> bool {
    menu_token().is_some_and(|t| query.split('&').any(|kv| kv.strip_prefix("t=") == Some(t.as_str())))
}

/// The state as server-sent events: the current state now, then each change, and a comment every 20 s to keep
/// the connection open. Ends when the page goes away (a write fails).
fn stream_state(w: &mut impl Write, req: &Request) -> std::io::Result<()> {
    if !token_ok(&req.query) || STREAMS.load(Ordering::SeqCst) >= MAX_STREAMS {
        w.write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")?;
        return w.flush();
    }
    if let Some(w) = req.query.split('&').find_map(|kv| kv.strip_prefix("w=")).and_then(|w| w.parse::<usize>().ok()) {
        if (1..=10_000).contains(&w) { AD_WIDTH.store(w, Ordering::SeqCst); }
    }
    STREAMS.fetch_add(1, Ordering::SeqCst);
    struct Done;
    impl Drop for Done { fn drop(&mut self) { STREAMS.fetch_sub(1, Ordering::SeqCst); } }
    let _done = Done;
    // the blank ad runs on KakaoTalk's ad page origin; the token, not the origin, is what's checked
    let origin = if req.origin.is_empty() { "*".to_string() } else { req.origin.clone() };
    write!(w, "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-store\r\n\
Access-Control-Allow-Origin: {origin}\r\nConnection: close\r\n\r\n")?;
    let (mut active, mut seen) = STATE.lock().map(|s| *s).unwrap_or((true, 0));
    loop {
        write!(w, "data: {}\n\n", if active { "active" } else { "inactive" })?;
        w.flush()?;
        loop {
            let Ok(g) = STATE.lock() else { return Ok(()) };
            let Ok((g, timeout)) = STATE_CHANGED.wait_timeout_while(g, Duration::from_secs(20), |s| s.1 == seen) else { return Ok(()) };
            if timeout.timed_out() { drop(g); w.write_all(b": ping\n\n")?; w.flush()?; continue; }
            (active, seen) = *g;
            break;
        }
    }
}

// Opening the app's menu from the blank ad: its logo asks MENU_PATH?t=<token> on this server when clicked, and
// the app shows its menu at the cursor. The token is random per run and only ever sent inside the blank ad, so
// another page that happens to reach this server can't pop the menu up. No handler (macOS for now): no logo.
pub const MENU_PATH: &str = "/talknap/menu";
/// The old path (see OLD_STATE_PATH).
pub const OLD_MENU_PATH: &str = "/kakaoadhide/menu";
fn is_menu_path(path: &str) -> bool { path == MENU_PATH || path == OLD_MENU_PATH }
type MenuHandler = Box<dyn Fn() + Send>;
static MENU: Mutex<Option<(String, MenuHandler)>> = Mutex::new(None);

pub fn set_menu_handler(token: String, f: impl Fn() + Send + 'static) {
    if let Ok(mut g) = MENU.lock() { *g = Some((token, Box::new(f))); }
}

pub fn clear_menu_handler() {
    if let Ok(mut g) = MENU.lock() { *g = None; }
}

/// The token for the blank ad's logo, when the app has a menu to open from it.
pub fn menu_token() -> Option<String> {
    MENU.lock().ok().and_then(|g| g.as_ref().map(|(t, _)| t.clone()))
}

fn open_menu(query: &str) {
    let Ok(g) = MENU.lock() else { return };
    let Some((token, f)) = g.as_ref() else { return };
    if query.split('&').any(|kv| kv.strip_prefix("t=") == Some(token.as_str())) { f(); }
}

/// Reads one request head (and discards its body). None at end of stream.
fn read_request(r: &mut impl BufRead) -> std::io::Result<Option<Request>> {
    let mut line = String::new();
    if r.read_line(&mut line)? == 0 { return Ok(None); }
    let mut parts = line.split_whitespace();
    let (Some(method), Some(target)) = (parts.next(), parts.next()) else { return Ok(None) };
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let (method, path, query) = (method.to_string(), path.to_string(), query.to_string());
    let mut content_length = 0usize;
    let mut origin = String::new();
    for _ in 0..100 {
        let mut h = String::new();
        if r.read_line(&mut h)? == 0 { return Ok(None); }
        let h = h.trim_end();
        if h.is_empty() { break; }
        if let Some((k, v)) = h.split_once(':') {
            if k.eq_ignore_ascii_case("content-length") { content_length = v.trim().parse().unwrap_or(0).min(1 << 20); }
            if k.eq_ignore_ascii_case("origin") { origin = v.trim().chars().filter(|c| !c.is_control()).take(200).collect(); }
        }
    }
    std::io::copy(&mut r.take(content_length as u64), &mut std::io::sink())?;
    Ok(Some(Request { method, path, query, origin }))
}

/// The ad placement a /sdk/banner request is for (its `id` parameter, e.g. DAN-FTsOAPU4yZ6oHLGy).
pub fn ad_unit(query: &str) -> Option<&str> {
    query.split('&').find_map(|kv| kv.strip_prefix("id=")).filter(|u| !u.is_empty() && u.len() <= 64)
}

pub fn respond(method: &str, path: &str, query: &str) -> (&'static str, &'static str, String) {
    if (method == "GET" || method == "HEAD") && path == "/sdk/banner" {
        let body = if crate::banner::is_bottom_banner(ad_unit(query)) {
            crate::banner::response_json(menu_token().as_deref())
        } else {
            crate::banner::no_ad_json()
        };
        ("200 OK", "application/json", body)
    } else {
        ("204 No Content", "text/plain", String::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, SocketAddr};

    /// The menu token and KakaoTalk state are process-wide: tests that set them take turns.
    static GLOBALS: Mutex<()> = Mutex::new(());

    /// Collects what the state stream writes; fails (ending the stream) after `limit` events.
    struct Sink { out: Arc<Mutex<String>>, limit: usize }
    impl Write for Sink {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            let mut o = self.out.lock().unwrap();
            // fail at the start of an event, not in the middle of one
            if b.starts_with(b"data: ") && o.matches("data: ").count() >= self.limit { return Err(std::io::Error::other("closed")); }
            o.push_str(&String::from_utf8_lossy(b));
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
    }

    #[test]
    fn state_stream() {
        let _turn = GLOBALS.lock().unwrap_or_else(|e| e.into_inner());
        set_menu_handler("tok42".into(), || {});
        set_kakao_active(true);
        let req = |q: &str| Request { method: "GET".into(), path: STATE_PATH.into(), query: q.into(), origin: "https://t1.kakaocdn.net".into() };
        // no token: refused
        let out = Arc::new(Mutex::new(String::new()));
        stream_state(&mut Sink { out: out.clone(), limit: 9 }, &req("t=nope")).unwrap();
        assert!(out.lock().unwrap().starts_with("HTTP/1.1 403"));
        // the current state at once, then the change
        let out = Arc::new(Mutex::new(String::new()));
        let o2 = out.clone();
        let t = std::thread::spawn(move || { let _ = stream_state(&mut Sink { out: o2, limit: 2 }, &req("t=tok42")); });
        while !out.lock().unwrap().contains("data: active") { std::thread::sleep(Duration::from_millis(5)); }
        set_kakao_active(false);
        while !out.lock().unwrap().contains("data: inactive") { std::thread::sleep(Duration::from_millis(5)); }
        set_kakao_active(true);   // the third event fails the write and ends the stream
        t.join().unwrap();
        let text = out.lock().unwrap().clone();
        assert!(text.contains("text/event-stream") && text.contains("Access-Control-Allow-Origin: https://t1.kakaocdn.net"));
        clear_menu_handler();
    }

    #[test]
    fn old_paths_still_served() {
        assert_eq!((MENU_PATH, STATE_PATH), ("/talknap/menu", "/talknap/state"));
        assert_eq!((OLD_MENU_PATH, OLD_STATE_PATH), ("/kakaoadhide/menu", "/kakaoadhide/state"));
        assert!(is_menu_path(MENU_PATH) && is_menu_path(OLD_MENU_PATH) && !is_menu_path(STATE_PATH));
        assert!(is_state_path(STATE_PATH) && is_state_path(OLD_STATE_PATH) && !is_state_path(MENU_PATH));
        assert!(!is_menu_path("/kakaoadhide/state") && !is_state_path("/kakaoadhide/menu") && !is_menu_path("/menu"));
    }

    #[test]
    fn menu_needs_the_token() {
        let _turn = GLOBALS.lock().unwrap_or_else(|e| e.into_inner());
        use std::sync::atomic::{AtomicUsize, Ordering};
        static OPENED: AtomicUsize = AtomicUsize::new(0);
        set_menu_handler("abc123".into(), || { OPENED.fetch_add(1, Ordering::SeqCst); });
        assert_eq!(menu_token().as_deref(), Some("abc123"));
        open_menu("");
        open_menu("t=wrong");
        open_menu("t=abc1234");
        assert_eq!(OPENED.load(Ordering::SeqCst), 0);
        open_menu("x=1&t=abc123");
        assert_eq!(OPENED.load(Ordering::SeqCst), 1);
        clear_menu_handler();
        open_menu("t=abc123");
        assert_eq!(OPENED.load(Ordering::SeqCst), 1);
        assert!(menu_token().is_none());
    }

    #[test]
    fn routes() {
        assert_eq!(respond("GET", "/sdk/banner", "").0, "200 OK");
        assert_eq!(respond("GET", "/anything/else", "").0, "204 No Content");
        assert_eq!(respond("POST", "/sdk/banner", "").0, "204 No Content");
        // the bottom banner gets the blank ad; another placement (Windows) no ad at all
        let blank = respond("GET", "/sdk/banner", "cnt=1&id=DAN-FTsOAPU4yZ6oHLGy&os=windows").2;
        assert!(blank.contains("\"ads\":[{"));
        let other = respond("GET", "/sdk/banner", "cnt=1&id=DAN-somethingElse").2;
        if cfg!(windows) { assert!(other.contains("\"ads\":[]")); } else { assert!(other.contains("\"ads\":[{")); }
        assert_eq!(ad_unit("a=1&id=DAN-x&b=2"), Some("DAN-x"));
        assert_eq!(ad_unit("a=1"), None);
    }

    #[test]
    fn parses_requests_with_bodies_and_keepalive() {
        let raw = b"POST /track HTTP/1.1\r\nHost: x\r\nContent-Length: 5\r\n\r\nhelloGET /sdk/banner?x=1 HTTP/1.1\r\nHost: x\r\n\r\n";
        let mut r = BufReader::new(&raw[..]);
        let a = read_request(&mut r).unwrap().unwrap();
        assert_eq!((a.method.as_str(), a.path.as_str()), ("POST", "/track"));
        let b = read_request(&mut r).unwrap().unwrap();
        assert_eq!((b.method.as_str(), b.path.as_str()), ("GET", "/sdk/banner"));
        assert!(read_request(&mut r).unwrap().is_none());
    }

    // End to end: a TLS client that trusts only our certificate fetches the banner over loopback.
    #[test]
    fn serves_the_banner_over_tls() {
        let _turn = GLOBALS.lock().unwrap_or_else(|e| e.into_inner());   // tls_config sets the current certificate
        let dir = std::env::temp_dir().join(format!("kah-server-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let p = crate::certs::create(&dir).unwrap();
        let cfg = tls_config(&p.pem, &p.key).unwrap();
        let running = start(cfg, &[SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 0)]).unwrap();
        let port = running.addresses[0].port();

        let mut roots = rustls::RootCertStore::empty();
        roots.add(CertificateDer::from(std::fs::read(&p.der).unwrap())).unwrap();
        let ccfg = rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions().unwrap()
            .with_root_certificates(roots).with_no_client_auth();
        let name = rustls_pki_types::ServerName::try_from("serv.ds.kakao.com").unwrap();
        let mut conn = rustls::ClientConnection::new(Arc::new(ccfg), name).unwrap();
        let mut tcp = TcpStream::connect(("127.0.0.1", port)).unwrap();
        let mut tls = rustls::Stream::new(&mut conn, &mut tcp);
        tls.write_all(b"GET /sdk/banner?accid=1 HTTP/1.1\r\nHost: serv.ds.kakao.com\r\nConnection: close\r\n\r\n").unwrap();
        let mut buf = vec![0u8; 8192];
        let mut got = Vec::new();
        while !String::from_utf8_lossy(&got).contains("refreshInterval") {
            let n = tls.read(&mut buf).unwrap();
            if n == 0 { break; }
            got.extend_from_slice(&buf[..n]);
        }
        let text = String::from_utf8_lossy(&got);
        assert!(text.starts_with("HTTP/1.1 200 OK"), "{text}");
        assert!(text.contains("\"status\":\"OK\""));
        crate::certs::remove(&dir);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A TLS handshake with a client that trusts only `der`; true when it succeeds.
    fn handshake_trusting(port: u16, der: &Path) -> bool {
        let mut roots = rustls::RootCertStore::empty();
        roots.add(CertificateDer::from(std::fs::read(der).unwrap())).unwrap();
        let ccfg = rustls::ClientConfig::builder_with_provider(provider())
            .with_safe_default_protocol_versions().unwrap()
            .with_root_certificates(roots).with_no_client_auth();
        let name = rustls_pki_types::ServerName::try_from("serv.ds.kakao.com").unwrap();
        let mut conn = rustls::ClientConnection::new(Arc::new(ccfg), name).unwrap();
        let mut tcp = TcpStream::connect(("127.0.0.1", port)).unwrap();
        tcp.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        while conn.is_handshaking() {
            if conn.complete_io(&mut tcp).is_err() { return false; }
        }
        true
    }

    // The running server switches to a new certificate (the move to the new name) without opening the port again.
    #[test]
    fn reloads_the_certificate_while_running() {
        let _turn = GLOBALS.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("kah-reload-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let a = crate::certs::create(&dir.join("a")).unwrap();
        let b = crate::certs::create(&dir.join("b")).unwrap();
        let running = start(tls_config(&a.pem, &a.key).unwrap(), &[SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 0)]).unwrap();
        let port = running.addresses[0].port();
        assert!(handshake_trusting(port, &a.der));
        assert!(!handshake_trusting(port, &b.der));
        // a key that doesn't belong to the certificate is refused, and the old certificate stays
        assert!(reload_cert(&b.pem, &a.key).is_err());
        assert!(handshake_trusting(port, &a.der));
        reload_cert(&b.pem, &b.key).unwrap();
        assert!(handshake_trusting(port, &b.der));
        assert!(!handshake_trusting(port, &a.der));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
