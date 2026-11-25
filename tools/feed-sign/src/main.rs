//! Makes and signs TalkNap's update feeds with Ed25519, the signature the app checks (src/feed.rs,
//! docs/update-feed.md). Not part of the app.
//!
//!   feed-sign new <package> <version> <tag> <feed.json>
//!       Writes a feed (feed-*.json; update-*.json with --legacy) for the package: its release link, size,
//!       SHA-256 and signature_ed25519.
//!   feed-sign keygen <private-key-file>
//!       New key pair: writes the private key (PKCS#8, base64) to the file and prints the public key for
//!       src/keys.rs. The private key is never printed.
//!   feed-sign units <feed.json> <windows|macos> [ids-file]
//!       Puts the ad placement ids (default ad-units/<platform>.txt) in `units`, signed in `units_signature`.
//!   feed-sign verify <feed.json> <package>
//!       Checks a feed against the package and the public keys in src/keys.rs (exit 0 verified, 1 not).
//!
//! `--legacy` (any command, anywhere in the arguments) signs and checks with the old "KakaoAdHide" message
//! prefixes, for the one update-*.json that 2.5.2 installs read. Those messages live only here, not in the app.
//!
//! The private key comes from the SIGNING_KEY_ED25519 environment variable (the repository secret) or from
//! `--key-file <path>` after the command. Every signature is checked against the public keys in src/keys.rs
//! before the feed is written.

#[allow(dead_code)]
#[path = "../../../src/feed.rs"]
mod feed;
#[path = "../../../src/keys.rs"]
mod keys;

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
    let legacy = args.iter().any(|a| a == "--legacy");
    let args: Vec<String> = args.into_iter().filter(|a| a != "--legacy").collect();
    let a: Vec<&str> = args.iter().map(String::as_str).collect();
    let r = match a.as_slice() {
        ["keygen", out] => keygen(Path::new(out)),
        ["new", package, version, tag, out] => new(Path::new(package), version, tag, Path::new(out), key_file.as_deref(), legacy),
        ["units", feed, platform] => units(Path::new(feed), platform, None, key_file.as_deref(), legacy),
        ["units", feed, platform, ids] => units(Path::new(feed), platform, Some(Path::new(ids)), key_file.as_deref(), legacy),
        ["verify", feed, package] => verify_with(Path::new(feed), Path::new(package), legacy, keys::PUBLIC_KEYS)
            .map(|()| println!("{feed} verifies against {package}")),
        _ => Err("usage: feed-sign keygen <file> | new <package> <version> <tag> <feed.json> | units <feed.json> <windows|macos> [ids-file] | verify <feed.json> <package> [--key-file <file>] [--legacy]".into()),
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

/// The old message 2.5.2 installs check: the same as `feed::signed_message` with the "KakaoAdHide" prefix.
fn legacy_message(version: &str, sha256_hex: &str) -> Vec<u8> {
    format!("KakaoAdHide\n{version}\n{}", sha256_hex.to_ascii_lowercase()).into_bytes()
}

/// The old `units_signature` message: "KakaoAdHide-units\n<platform>\n<id>…".
fn legacy_units_message(platform: &str, units: &[String]) -> Vec<u8> {
    let mut m = format!("KakaoAdHide-units\n{platform}");
    for u in units { m.push('\n'); m.push_str(u); }
    m.into_bytes()
}

/// Checks a feed file against its package and `keys`: url, size, SHA-256 and the signature (old message if `legacy`).
/// With `legacy` the url must also be under the old repository's prefix, the only one 2.5.2 accepts.
fn verify_with(feed_path: &Path, package: &Path, legacy: bool, keys: &[&str]) -> R {
    let data = std::fs::read(feed_path).map_err(|e| format!("{}: {e}", feed_path.display()))?;
    let f = feed::Feed::parse(&data).map_err(|e| e.to_string())?;
    f.check_url().map_err(|e| e.to_string())?;
    if legacy && !f.url.strip_prefix(feed::OLD_ASSET_PREFIX).is_some_and(|rest| !rest.contains("..")) {
        return Err(format!("url is not under {} (2.5.2 accepts only that)", feed::OLD_ASSET_PREFIX));
    }
    let file = std::fs::File::open(package).map_err(|e| format!("{}: {e}", package.display()))?;
    let (sha, size) = feed::sha256_hex_reader(file).map_err(|e| format!("{}: {e}", package.display()))?;
    if f.size != 0 && f.size != size { return Err("size does not match".into()); }
    if sha != f.sha256.to_ascii_lowercase() { return Err("SHA-256 does not match".into()); }
    let msg = if legacy { legacy_message(&f.version, &sha) } else { feed::signed_message(&f.version, &sha) };
    if !feed::ed25519_valid(&msg, &f.signature_ed25519, keys) { return Err("signature does not verify".into()); }
    Ok(())
}

fn new(package: &Path, version: &str, tag: &str, out: &Path, key_file: Option<&str>, legacy: bool) -> R {
    if feed::Version::parse(version).is_none() { return Err(format!("not a version: {version}")); }
    if tag.is_empty() || !tag.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-') { return Err(format!("not a tag: {tag}")); }
    let name = package.file_name().and_then(|n| n.to_str()).ok_or("no file name")?;
    let key = signing_key(key_file)?;
    let file = std::fs::File::open(package).map_err(|e| format!("{}: {e}", package.display()))?;
    let (sha, size) = feed::sha256_hex_reader(file).map_err(|e| format!("{}: {e}", package.display()))?;
    let msg = if legacy { legacy_message(version, &sha) } else { feed::signed_message(version, &sha) };
    let sig = B64.encode(key.sign(&msg).as_ref());
    if !feed::ed25519_valid(&msg, &sig, keys::PUBLIC_KEYS) { return Err("the new signature does not verify".into()); }
    let mut f = Map::new();
    f.insert("version".into(), version.into());
    // the bridge feeds (--legacy) must keep the old repository prefix: 2.5.2 accepts only that
    let prefix = if legacy { feed::OLD_ASSET_PREFIX } else { feed::ASSET_PREFIX };
    f.insert("url".into(), format!("{prefix}{tag}/{name}").into());
    f.insert("size".into(), size.into());
    f.insert("sha256".into(), sha.into());
    f.insert("page".into(), format!("https://github.com/{}/releases/tag/{tag}", feed::REPO).into());
    f.insert("signature_ed25519".into(), sig.into());
    write_feed(out, &f)?;
    println!("wrote {} (version {version})", out.display());
    Ok(())
}

fn units(path: &Path, platform: &str, ids: Option<&Path>, key_file: Option<&str>, legacy: bool) -> R {
    if !matches!(platform, "windows" | "macos") { return Err(format!("unknown platform: {platform}")); }
    let ids: PathBuf = ids.map(Path::to_path_buf)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ad-units").join(format!("{platform}.txt")));
    let text = std::fs::read_to_string(&ids).map_err(|e| format!("{}: {e}", ids.display()))?;
    let list: Vec<String> = text.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect();
    if list.is_empty() || list.len() > 20 { return Err(format!("{} must list 1 to 20 ids", ids.display())); }
    if let Some(bad) = list.iter().find(|u| !feed::unit_ok(u)) { return Err(format!("not an ad placement id: {bad}")); }

    let key = signing_key(key_file)?;
    let msg = if legacy { legacy_units_message(platform, &list) } else { feed::units_message(platform, &list) };
    let sig = B64.encode(key.sign(&msg).as_ref());
    if !feed::ed25519_valid(&msg, &sig, keys::PUBLIC_KEYS) { return Err("the new signature does not verify".into()); }
    let mut f = read_feed(path)?;
    f.insert("units".into(), Value::Array(list.iter().cloned().map(Value::String).collect()));
    f.insert("units_signature".into(), Value::String(sig));
    write_feed(path, &f)?;
    println!("signed {} ad placement id(s) for {platform} into {}", list.len(), path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_and_new_messages_differ() {
        assert_eq!(legacy_message("2.6.0", "AB"), b"KakaoAdHide\n2.6.0\nab".to_vec());
        assert_eq!(legacy_units_message("windows", &["DAN-a".into()]), b"KakaoAdHide-units\nwindows\nDAN-a".to_vec());
        assert_ne!(legacy_message("2.6.0", "ab"), feed::signed_message("2.6.0", "ab"));
    }

    /// Writes a 1-byte package and a feed for it signed with a throwaway key; returns (feed, package, public key).
    fn fixture(name: &str, legacy: bool) -> (PathBuf, PathBuf, String) {
        let dir = std::env::temp_dir().join(format!("feed-sign-test-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let package = dir.join("pkg.bin");
        std::fs::write(&package, [1u8]).unwrap();
        let key = Ed25519KeyPair::from_seed_unchecked(&[7; 32]).unwrap();
        let (sha, size) = feed::sha256_hex_reader(std::fs::File::open(&package).unwrap()).unwrap();
        let msg = if legacy { legacy_message("2.6.0", &sha) } else { feed::signed_message("2.6.0", &sha) };
        let mut f = Map::new();
        f.insert("version".into(), "2.6.0".into());
        let prefix = if legacy { feed::OLD_ASSET_PREFIX } else { feed::ASSET_PREFIX };
        f.insert("url".into(), format!("{prefix}v2.6.0/pkg.bin").into());
        f.insert("size".into(), size.into());
        f.insert("sha256".into(), sha.into());
        f.insert("signature_ed25519".into(), B64.encode(key.sign(&msg).as_ref()).into());
        let feed_path = dir.join("feed.json");
        write_feed(&feed_path, &f).unwrap();
        (feed_path, package, B64.encode(key.public_key().as_ref()))
    }

    #[test]
    fn verify_checks_the_chosen_message() {
        let (feed_path, package, public) = fixture("chosen", true);
        let keys = [public.as_str()];
        assert_eq!(verify_with(&feed_path, &package, true, &keys), Ok(()));
        assert!(verify_with(&feed_path, &package, false, &keys).is_err());
        std::fs::write(&package, [2u8]).unwrap();
        assert!(verify_with(&feed_path, &package, true, &keys).is_err());
        std::fs::remove_dir_all(feed_path.parent().unwrap()).unwrap();
    }

    #[test]
    fn verify_accepts_the_new_message_without_legacy() {
        let (feed_path, package, public) = fixture("new", false);
        let keys = [public.as_str()];
        assert_eq!(verify_with(&feed_path, &package, false, &keys), Ok(()));
        assert!(verify_with(&feed_path, &package, true, &keys).is_err());
        std::fs::remove_dir_all(feed_path.parent().unwrap()).unwrap();
    }

    #[test]
    fn legacy_verify_rejects_the_talknap_prefix() {
        let (feed_path, package, public) = fixture("prefix", true);
        let keys = [public.as_str()];
        let text = std::fs::read_to_string(&feed_path).unwrap().replace(feed::OLD_ASSET_PREFIX, feed::ASSET_PREFIX);
        std::fs::write(&feed_path, text).unwrap();
        assert!(verify_with(&feed_path, &package, true, &keys).unwrap_err().contains("2.5.2"));
        std::fs::remove_dir_all(feed_path.parent().unwrap()).unwrap();
    }
}
