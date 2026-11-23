//! Makes and signs KakaoAdHide's update feeds with Ed25519, the signature the app checks (src/feed.rs,
//! docs/update-feed.md). Not part of the app.
//!
//!   feed-sign new <package> <version> <tag> <feed.json>
//!       Writes a feed (update-*.json) for the package: its release link, size, SHA-256 and signature_ed25519.
//!   feed-sign keygen <private-key-file>
//!       New key pair: writes the private key (PKCS#8, base64) to the file and prints the public key for
//!       src/keys.rs. The private key is never printed.
//!   feed-sign release <feed.json>
//!       Adds `signature_ed25519` over the feed's version and sha256. Checks the feed's RSA `signature` first
//!       (tools/sign-release.ps1 writes it for copies before 2.5.1).
//!   feed-sign units <feed.json> <windows|macos> [ids-file]
//!       Puts the ad placement ids (default ad-units/<platform>.txt) in `units`, signed in `units_signature`.
//!
//! The private key comes from the SIGNING_KEY_ED25519 environment variable (the repository secret) or from
//! `--key-file <path>` after the command. Every signature is checked against the public keys in src/keys.rs
//! before the feed is written.

#[allow(dead_code)]
#[path = "../../../src/feed.rs"]
mod feed;
#[path = "../../../src/keys.rs"]
mod keys;
mod legacy_rsa;

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use ring::signature::{Ed25519KeyPair, KeyPair};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (args, key_file) = match args.iter().position(|a| a == "--key-file") {
        Some(i) if i + 1 < args.len() => {
            let f = args[i + 1].clone();
            ([&args[..i], &args[i + 2..]].concat(), Some(f))
        }
        _ => (args, None),
    };
    let a: Vec<&str> = args.iter().map(String::as_str).collect();
    let r = match a.as_slice() {
        ["keygen", out] => keygen(Path::new(out)),
        ["new", package, version, tag, out] => new(Path::new(package), version, tag, Path::new(out), key_file.as_deref()),
        ["release", feed] => release(Path::new(feed), key_file.as_deref()),
        ["units", feed, platform] => units(Path::new(feed), platform, None, key_file.as_deref()),
        ["units", feed, platform, ids] => units(Path::new(feed), platform, Some(Path::new(ids)), key_file.as_deref()),
        _ => Err("usage: feed-sign keygen <file> | new <package> <version> <tag> <feed.json> | release <feed.json> | units <feed.json> <windows|macos> [ids-file] [--key-file <file>]".into()),
    };
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => { eprintln!("feed-sign: {e}"); ExitCode::FAILURE }
    }
}

type R = Result<(), String>;

fn keygen(out: &Path) -> R {
    if out.exists() { return Err(format!("{} already exists", out.display())); }
    let pkcs8 = Ed25519KeyPair::generate_pkcs8(&ring::rand::SystemRandom::new()).map_err(|_| "key generation failed")?;
    let pair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).map_err(|_| "key generation failed")?;
    std::fs::write(out, B64.encode(pkcs8.as_ref())).map_err(|e| format!("{}: {e}", out.display()))?;
    println!("private key: {}", out.display());
    println!("public key (for src/keys.rs): {}", B64.encode(pair.public_key().as_ref()));
    Ok(())
}

/// The signing key, which must be one of the public keys the app has (src/keys.rs).
fn signing_key(key_file: Option<&str>) -> Result<Ed25519KeyPair, String> {
    let text = match key_file {
        Some(f) => std::fs::read_to_string(f).map_err(|e| format!("{f}: {e}"))?,
        None => std::env::var("SIGNING_KEY_ED25519").map_err(|_| "no signing key: set SIGNING_KEY_ED25519 or pass --key-file")?,
    };
    let der = B64.decode(text.trim_matches(|c: char| c == '\u{feff}' || c.is_whitespace())).map_err(|_| "the signing key is not base64")?;
    let pair = Ed25519KeyPair::from_pkcs8(&der).map_err(|_| "the signing key is not an Ed25519 PKCS#8 key")?;
    let public = B64.encode(pair.public_key().as_ref());
    if !keys::PUBLIC_KEYS.contains(&public.as_str()) { return Err("the signing key is not one of the public keys in src/keys.rs".into()); }
    Ok(pair)
}

fn read_feed(path: &Path) -> Result<Map<String, Value>, String> {
    let data = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let data = data.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&data);
    match serde_json::from_slice(data) {
        Ok(Value::Object(m)) => Ok(m),
        _ => Err(format!("{}: not a JSON object", path.display())),
    }
}

fn write_feed(path: &Path, feed: &Map<String, Value>) -> R {
    let text = serde_json::to_string_pretty(feed).map_err(|e| e.to_string())?;
    std::fs::write(path, text + "\n").map_err(|e| format!("{}: {e}", path.display()))
}

fn field<'a>(feed: &'a Map<String, Value>, name: &str) -> Result<&'a str, String> {
    feed.get(name).and_then(Value::as_str).filter(|s| !s.is_empty()).ok_or_else(|| format!("the feed has no {name}"))
}

fn new(package: &Path, version: &str, tag: &str, out: &Path, key_file: Option<&str>) -> R {
    if feed::Version::parse(version).is_none() { return Err(format!("not a version: {version}")); }
    if tag.is_empty() || !tag.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-') { return Err(format!("not a tag: {tag}")); }
    let name = package.file_name().and_then(|n| n.to_str()).ok_or("no file name")?;
    let key = signing_key(key_file)?;
    let file = std::fs::File::open(package).map_err(|e| format!("{}: {e}", package.display()))?;
    let (sha, size) = feed::sha256_hex_reader(file).map_err(|e| format!("{}: {e}", package.display()))?;
    let sig = B64.encode(key.sign(&feed::signed_message(version, &sha)).as_ref());
    if !feed::signature_valid(version, &sha, &sig, keys::PUBLIC_KEYS) { return Err("the new signature does not verify".into()); }
    let mut f = Map::new();
    f.insert("version".into(), version.into());
    f.insert("url".into(), format!("{}{tag}/{name}", feed::ASSET_PREFIX).into());
    f.insert("size".into(), size.into());
    f.insert("sha256".into(), sha.into());
    f.insert("page".into(), format!("https://github.com/{}/releases/tag/{tag}", feed::REPO).into());
    f.insert("signature_ed25519".into(), sig.into());
    write_feed(out, &f)?;
    println!("wrote {} (version {version})", out.display());
    Ok(())
}

fn release(path: &Path, key_file: Option<&str>) -> R {
    let key = signing_key(key_file)?;
    let mut f = read_feed(path)?;
    let (version, sha) = (field(&f, "version")?.to_string(), field(&f, "sha256")?.to_ascii_lowercase());
    let msg = feed::signed_message(&version, &sha);
    if !legacy_rsa::valid(&msg, field(&f, "signature")?) {
        return Err("the feed's RSA signature does not verify (copies before 2.5.1 could not update)".into());
    }
    let sig = B64.encode(key.sign(&msg).as_ref());
    if !feed::signature_valid(&version, &sha, &sig, keys::PUBLIC_KEYS) { return Err("the new signature does not verify".into()); }
    f.insert("signature_ed25519".into(), Value::String(sig));
    write_feed(path, &f)?;
    println!("signed {} (version {version})", path.display());
    Ok(())
}

fn units(path: &Path, platform: &str, ids: Option<&Path>, key_file: Option<&str>) -> R {
    if !matches!(platform, "windows" | "macos") { return Err(format!("unknown platform: {platform}")); }
    let ids: PathBuf = ids.map(Path::to_path_buf)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ad-units").join(format!("{platform}.txt")));
    let text = std::fs::read_to_string(&ids).map_err(|e| format!("{}: {e}", ids.display()))?;
    let list: Vec<String> = text.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect();
    if list.is_empty() || list.len() > 20 { return Err(format!("{} must list 1 to 20 ids", ids.display())); }
    if let Some(bad) = list.iter().find(|u| !feed::unit_ok(u)) { return Err(format!("not an ad placement id: {bad}")); }

    let key = signing_key(key_file)?;
    let sig = B64.encode(key.sign(&feed::units_message(platform, &list)).as_ref());
    if !feed::units_signature_valid(platform, &list, &sig, keys::PUBLIC_KEYS) { return Err("the new signature does not verify".into()); }
    let mut f = read_feed(path)?;
    f.insert("units".into(), Value::Array(list.iter().cloned().map(Value::String).collect()));
    f.insert("units_signature".into(), Value::String(sig));
    write_feed(path, &f)?;
    println!("signed {} ad placement id(s) for {platform} into {}", list.len(), path.display());
    Ok(())
}
