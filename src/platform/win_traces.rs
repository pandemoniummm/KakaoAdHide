//! 2.7.0 moves the install from KakaoAdHide to TalkNap. Windows itself keeps records of the old program (recently
//! run programs, display names, notification settings, crash reports, ...). This deletes the ones that mention the
//! old name, once: `clean_user` needs no administrator, `clean_admin` runs inside the elevated hosts step.
//!
//! Only records that contain the old name are touched (case-insensitive; UserAssist keeps names in ROT13).
//! Every deletion ignores errors. Deliberately kept: Recent (shortcuts), downloaded installers, KakaoTalk's cache,
//! Amcache.hve and the event logs.

use std::path::{Path, PathBuf};
use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WRITE};
use winreg::RegKey;

const OLD: &str = "kakaoadhide";

/// Whether `s` contains the old name (any case).
fn mentions_old(s: &str) -> bool { s.to_lowercase().contains(OLD) }

fn rot13(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'a'..='z' => (((c as u8 - b'a') + 13) % 26 + b'a') as char,
            'A'..='Z' => (((c as u8 - b'A') + 13) % 26 + b'A') as char,
            _ => c,
        })
        .collect()
}

/// RunMRU's MRUList without the letters of the removed values (order kept).
fn mru_list_without(list: &str, removed: &[char]) -> String { list.chars().filter(|c| !removed.contains(c)).collect() }

/// The values of TypedPaths that stay, in order of their numbers: they are written back as url1, url2, ...
fn renumbered(values: &[(u32, String)]) -> Vec<String> {
    let mut v = values.to_vec();
    v.sort_by_key(|(n, _)| *n);
    v.into_iter().map(|(_, s)| s).collect()
}

/// A text without the lines that mention the old name; the other lines keep their line endings as they are.
fn without_old_lines(text: &str) -> String { text.split_inclusive('\n').filter(|l| !mentions_old(l)).collect() }

/// The same for bytes of an unknown 8-bit encoding: only the ASCII letters of the name are compared.
fn without_old_lines_bytes(bytes: &[u8]) -> Vec<u8> {
    bytes
        .split_inclusive(|&b| b == b'\n')
        .filter(|l| !l.to_ascii_lowercase().windows(OLD.len()).any(|w| w == OLD.as_bytes()))
        .flatten()
        .copied()
        .collect()
}

// ---------------------------------------------------------------- registry helpers (HKCU)

fn hkcu() -> RegKey { RegKey::predef(HKEY_CURRENT_USER) }

fn open_rw(root: &RegKey, path: &str) -> Option<RegKey> { root.open_subkey_with_flags(path, KEY_READ | KEY_WRITE).ok() }

/// Deletes the values of `key` whose name satisfies `pred`.
fn delete_values_where(key: &RegKey, pred: impl Fn(&str) -> bool) {
    let names: Vec<String> = key.enum_values().filter_map(|v| v.ok()).map(|(n, _)| n).filter(|n| pred(n)).collect();
    for n in names { let _ = key.delete_value(n); }
}

fn clean_values_at(root: &RegKey, path: &str, pred: impl Fn(&str) -> bool) {
    if let Some(k) = open_rw(root, path) { delete_values_where(&k, pred); }
}

/// The names of the direct subkeys of `path`.
fn subkey_names(root: &RegKey, path: &str) -> Vec<String> {
    root.open_subkey(path).map(|k| k.enum_keys().filter_map(|n| n.ok()).collect()).unwrap_or_default()
}

fn clean_run_mru(root: &RegKey) {
    let Some(k) = open_rw(root, r"Software\Microsoft\Windows\CurrentVersion\Explorer\RunMRU") else { return };
    let mut removed = Vec::new();
    for (name, _) in k.enum_values().filter_map(|v| v.ok()).collect::<Vec<_>>() {
        let mut chars = name.chars();
        let (Some(c), None) = (chars.next(), chars.next()) else { continue };   // letters only (not MRUList)
        if k.get_value::<String, _>(&name).map(|d| mentions_old(&d)).unwrap_or(false) {
            let _ = k.delete_value(&name);
            removed.push(c);
        }
    }
    if removed.is_empty() { return; }
    if let Ok(list) = k.get_value::<String, _>("MRUList") { let _ = k.set_value("MRUList", &mru_list_without(&list, &removed)); }
}

fn clean_typed_paths(root: &RegKey) {
    let Some(k) = open_rw(root, r"Software\Microsoft\Windows\CurrentVersion\Explorer\TypedPaths") else { return };
    let mut all: Vec<(u32, String)> = Vec::new();
    for (name, _) in k.enum_values().filter_map(|v| v.ok()).collect::<Vec<_>>() {
        let Some(n) = name.strip_prefix("url").and_then(|d| d.parse::<u32>().ok()) else { continue };
        if let Ok(v) = k.get_value::<String, _>(&name) { all.push((n, v)); }
    }
    if !all.iter().any(|(_, v)| mentions_old(v)) { return; }
    let keep: Vec<(u32, String)> = all.iter().filter(|(_, v)| !mentions_old(v)).cloned().collect();
    let rest = renumbered(&keep);
    for (i, v) in rest.iter().enumerate() { let _ = k.set_value(format!("url{}", i + 1), v); }
    for (n, _) in &all {
        if *n as usize > rest.len() { let _ = k.delete_value(format!("url{n}")); }
    }
}

// ---------------------------------------------------------------- files

/// Deletes the entries of `dir` (files or folders, directly inside it) whose name satisfies `pred`.
fn remove_entries_where(dir: &Path, pred: impl Fn(&str) -> bool) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.filter_map(|e| e.ok()) {
        if !pred(&e.file_name().to_string_lossy()) { continue; }
        let p = e.path();
        if e.file_type().map(|t| t.is_dir()).unwrap_or(false) { let _ = std::fs::remove_dir_all(&p); } else { let _ = std::fs::remove_file(&p); }
    }
}

fn env_dir(var: &str) -> Option<PathBuf> {
    std::env::var_os(var).map(PathBuf::from).filter(|p| p.is_absolute())
}

/// Rewrites the file without the lines that mention the old name, keeping its encoding (UTF-16LE with a BOM, or
/// 8-bit text). Writes only when something was removed.
fn clean_text_file(path: &Path) {
    let Ok(bytes) = std::fs::read(path) else { return };
    let new: Vec<u8> = if let Some(body) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        let units: Vec<u16> = body.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect();
        let text = String::from_utf16_lossy(&units);
        let kept = without_old_lines(&text);
        if kept == text { return; }
        [0xFF, 0xFE].into_iter().chain(kept.encode_utf16().flat_map(|u| u.to_le_bytes())).collect()
    } else {
        without_old_lines_bytes(&bytes)
    };
    if new != bytes { let _ = std::fs::write(path, new); }
}

// ---------------------------------------------------------------- entry points

/// Removes this user's records of the old name (no administrator needed).
pub fn clean_user() {
    let root = hkcu();
    let explorer = r"Software\Microsoft\Windows\CurrentVersion\Explorer";

    // Programs the shell saw running: the old exe's path is in the value names.
    let compat = r"Software\Microsoft\Windows NT\CurrentVersion\AppCompatFlags\Compatibility Assistant";
    for sub in ["Store", "Persisted"] { clean_values_at(&root, &format!(r"{compat}\{sub}"), mentions_old); }
    clean_values_at(&root, r"Software\Classes\Local Settings\Software\Microsoft\Windows\Shell\MuiCache", mentions_old);
    let usage = format!(r"{explorer}\FeatureUsage");
    for sub in subkey_names(&root, &usage) { clean_values_at(&root, &format!(r"{usage}\{sub}"), mentions_old); }

    // Notification area entries of the old exe (the key is named by an id; the exe path is a value).
    let icons = r"Control Panel\NotifyIconSettings";
    for id in subkey_names(&root, icons) {
        let path = format!(r"{icons}\{id}");
        let old = root.open_subkey(&path).ok().and_then(|k| k.get_value::<String, _>("ExecutablePath").ok()).map(|p| mentions_old(&p)).unwrap_or(false);
        if old { if let Some(k) = open_rw(&root, icons) { let _ = k.delete_subkey_all(&id); } }
    }

    // UserAssist keeps its names in ROT13.
    let assist = format!(r"{explorer}\UserAssist");
    for guid in subkey_names(&root, &assist) { clean_values_at(&root, &format!(r"{assist}\{guid}\Count"), |n| mentions_old(&rot13(n))); }

    clean_run_mru(&root);
    clean_typed_paths(&root);

    let old_lnk = format!("{}.lnk", crate::platform::OLD_APP);
    let approved = format!(r"{explorer}\StartupApproved\StartupFolder");
    // Turned off in Task Manager's startup apps: the new shortcut keeps that choice (unless it has its own).
    if let Some(k) = open_rw(&root, &approved) {
        let new_lnk = format!("{}.lnk", crate::platform::APP);
        if k.get_raw_value(&new_lnk).is_err() {
            if let Ok(v) = k.get_raw_value(&old_lnk) { let _ = k.set_raw_value(&new_lnk, &v); }
        }
    }
    clean_values_at(&root, &approved, |n| n.eq_ignore_ascii_case(&old_lnk));
    clean_values_at(&root, r"Software\Microsoft\Windows\CurrentVersion\RunNotification", |n| n.eq_ignore_ascii_case(&format!("StartupTNoti{old_lnk}")));
    let id = "pandemoniummm.KakaoAdHide";
    for base in [r"Software\Microsoft\Windows\CurrentVersion\Notifications\Settings", r"Software\Microsoft\Windows\CurrentVersion\PushNotifications\Backup"] {
        if let Some(k) = open_rw(&root, base) { let _ = k.delete_subkey_all(id); }
    }

    if let Some(local) = env_dir("LOCALAPPDATA") {
        remove_entries_where(&local.join("CrashDumps"), |n| n.to_lowercase().starts_with("kakaoadhide.exe."));
        for sub in ["ReportArchive", "ReportQueue"] { remove_entries_where(&local.join(r"Microsoft\Windows\WER").join(sub), mentions_old); }
    }
}

/// Removes the machine-wide records of the old name. Needs administrator (the elevated hosts step runs it).
pub fn clean_admin() {
    let tracing = r"SOFTWARE\Microsoft\Tracing";
    if let Some(k) = open_rw(&RegKey::predef(HKEY_LOCAL_MACHINE), tracing) {
        let names: Vec<String> = k.enum_keys().filter_map(|n| n.ok()).filter(|n| n.to_lowercase().starts_with("kakaoadhide_")).collect();
        for n in names { let _ = k.delete_subkey_all(n); }
    }
    if let Some(win) = env_dir("windir") {
        remove_entries_where(&win.join("Prefetch"), |n| {
            let n = n.to_lowercase();
            n.starts_with(OLD) && n.ends_with(".pf")
        });
        clean_text_file(&win.join(r"appcompat\pca\PcaAppLaunchDic.txt"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rot13_of_the_old_name() { assert_eq!(rot13("KakaoAdHide"), "XnxnbNqUvqr"); assert_eq!(rot13(&rot13("a/B-1")), "a/B-1"); }

    #[test]
    fn mentions_old_ignores_case() {
        assert!(mentions_old("C:\\Users\\a\\AppData\\Local\\KAKAOADHIDE\\x"));
        assert!(!mentions_old("C:\\Users\\a\\AppData\\Local\\TalkNap\\TalkNap.exe"));
        assert!(mentions_old(&rot13("P:\\Hfref\\NWE\\XnxnbNqUvqr.rkr")));
    }

    #[test]
    fn mru_list_drops_the_removed_letters() { assert_eq!(mru_list_without("gijklm", &['i', 'k']), "gjlm"); }

    #[test]
    fn typed_paths_are_renumbered_in_order() {
        assert_eq!(renumbered(&[(1, "a".into()), (3, "c".into())]), ["a", "c"]);
        assert_eq!(renumbered(&[(3, "c".into()), (1, "a".into())]), ["a", "c"]);
    }

    #[test]
    fn pca_lines_with_the_old_name_go_and_endings_stay() {
        assert_eq!(without_old_lines("x\r\nC:\\a\\KakaoAdHide.exe|1\r\ny\r\n"), "x\r\ny\r\n");
        assert_eq!(without_old_lines("x\ny"), "x\ny");
        assert_eq!(without_old_lines_bytes(b"x\r\nC:\\a\\KAKAOADHIDE.exe|1\r\ny\r\n"), b"x\r\ny\r\n");
        assert_eq!(without_old_lines_bytes(b"\xff\xfe-ansi\nz"), b"\xff\xfe-ansi\nz");
    }
}
