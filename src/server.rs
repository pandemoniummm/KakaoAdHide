//! The local ad server: HTTPS on port 443 for the ad hosts (sent here by hosts.rs), answering banner.rs.
//!
//! - Only connections from this computer are served; anything else is closed before the TLS handshake (on macOS
//!   the port can only be opened on all addresses without root, see `listen_addresses`).
//! - It speaks plain HTTP/1.1 over TLS, answers /sdk/banner with the blank ad and everything else with
//!   204 No Content, and never contacts the real ad servers.
//! - One short-lived thread per connection: KakaoTalk makes a handful of requests an hour.

use rustls::ServerConfig;
use rustls_pki_types::pem::PemObject;
use rustls_pki_types::{CertificateDer, PrivateKeyDer};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

pub fn tls_config(cert_pem: &Path, key_pem: &Path) -> Result<Arc<ServerConfig>, String> {
    let certs: Vec<CertificateDer<'static>> = CertificateDer::pem_file_iter(cert_pem)
        .map_err(|e| e.to_string())?
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())?;
    let key = PrivateKeyDer::from_pem_file(key_pem).map_err(|e| e.to_string())?;
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut cfg = ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| e.to_string())?;
    cfg.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(Arc::new(cfg))
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

fn serve(mut tcp: TcpStream, cfg: Arc<ServerConfig>) -> std::io::Result<()> {
    tcp.set_read_timeout(Some(Duration::from_secs(15)))?;
    tcp.set_write_timeout(Some(Duration::from_secs(15)))?;
    let conn = rustls::ServerConnection::new(cfg).map_err(std::io::Error::other)?;
    let tls = rustls::StreamOwned::new(conn, &mut tcp);
    let mut reader = BufReader::new(tls);
    // keep-alive: a few requests per connection at most
    for _ in 0..32 {
        let Some(req) = read_request(&mut reader)? else { return Ok(()) };
        let (status, ctype, body) = respond(&req.method, &req.path);
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

pub struct Request { pub method: String, pub path: String }

/// Reads one request head (and discards its body). None at end of stream.
fn read_request(r: &mut impl BufRead) -> std::io::Result<Option<Request>> {
    let mut line = String::new();
    if r.read_line(&mut line)? == 0 { return Ok(None); }
    let mut parts = line.split_whitespace();
    let (Some(method), Some(target)) = (parts.next(), parts.next()) else { return Ok(None) };
    let (method, path) = (method.to_string(), target.split('?').next().unwrap_or("").to_string());
    let mut content_length = 0usize;
    for _ in 0..100 {
        let mut h = String::new();
        if r.read_line(&mut h)? == 0 { return Ok(None); }
        let h = h.trim_end();
        if h.is_empty() { break; }
        if let Some((k, v)) = h.split_once(':') {
            if k.eq_ignore_ascii_case("content-length") { content_length = v.trim().parse().unwrap_or(0).min(1 << 20); }
        }
    }
    std::io::copy(&mut r.take(content_length as u64), &mut std::io::sink())?;
    Ok(Some(Request { method, path }))
}

pub fn respond(method: &str, path: &str) -> (&'static str, &'static str, String) {
    if (method == "GET" || method == "HEAD") && path == "/sdk/banner" {
        ("200 OK", "application/json; charset=utf-8", crate::banner::response_json())
    } else {
        ("204 No Content", "text/plain", String::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, SocketAddr};

    #[test]
    fn routes() {
        assert_eq!(respond("GET", "/sdk/banner").0, "200 OK");
        assert_eq!(respond("GET", "/anything/else").0, "204 No Content");
        assert_eq!(respond("POST", "/sdk/banner").0, "204 No Content");
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
}
