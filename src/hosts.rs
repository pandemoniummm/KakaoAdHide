//! The hosts file entries that send KakaoTalk's ad requests to the local server (server.rs).
//!
//! Our entries sit in a block between two marker comment lines; only that block is ever added or removed and
//! everything else in the file is kept byte for byte (including its line endings). The markers are whole
//! comment lines because macOS's resolver doesn't treat a `#` after an entry as a comment: it read it as one
//! more host name and then ignored the IPv4 entry.

/// The hosts KakaoTalk loads its ad banner from (measured on KakaoTalk for Mac 26.10).
pub const AD_HOSTS: &[&str] = &["serv.ds.kakao.com", "display.ad.daum.net"];
pub const BEGIN: &str = "# KakaoAdHide begin (added by KakaoAdHide; removed when it is uninstalled)";
pub const END: &str = "# KakaoAdHide end";
const OLD_MARKER: &str = "# KakaoAdHide";   // pre-release builds put this at the end of each entry

#[cfg(windows)]
pub const HOSTS_PATH: &str = r"C:\Windows\System32\drivers\etc\hosts";
#[cfg(not(windows))]
pub const HOSTS_PATH: &str = "/etc/hosts";

fn newline(text: &str) -> &'static str { if text.contains("\r\n") { "\r\n" } else { "\n" } }

fn is_begin(line: &str) -> bool { line.trim_end().starts_with("# KakaoAdHide begin") }
fn is_end(line: &str) -> bool { line.trim_end() == END }

/// The text without our block (and without entries marked by pre-release builds).
pub fn remove(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut inside = false;
    for line in text.split_inclusive('\n') {
        if inside { if is_end(line) { inside = false; } continue; }
        if is_begin(line) { inside = true; continue; }
        if line.trim_end().ends_with(OLD_MARKER) && !line.trim_start().starts_with('#') { continue; }
        out.push_str(line);
    }
    out
}

/// IPv4 only: the ad hosts have no IPv6 address (no AAAA records), and on macOS adding "::1" entries for them
/// made the resolver ignore the IPv4 ones.
fn entries() -> Vec<String> { AD_HOSTS.iter().map(|h| format!("127.0.0.1 {h}")).collect() }

/// The text with exactly our block at the end.
pub fn add(text: &str) -> String {
    let nl = newline(text);
    let mut out = remove(text);
    if !out.is_empty() && !out.ends_with('\n') { out.push_str(nl); }
    out.push_str(BEGIN);
    out.push_str(nl);
    for e in entries() { out.push_str(&e); out.push_str(nl); }
    out.push_str(END);
    out.push_str(nl);
    out
}

/// Whether our block is present with exactly the current entries.
pub fn is_applied(text: &str) -> bool {
    let mut inside = false;
    let mut found = Vec::new();
    for line in text.lines() {
        if inside {
            if is_end(line) { break; }
            found.push(line.split_whitespace().collect::<Vec<_>>().join(" "));
        } else if is_begin(line) {
            inside = true;
        }
    }
    found.retain(|l| !l.is_empty());
    found == entries()   // exactly ours: a block from an older build is rewritten
}

pub fn read() -> std::io::Result<String> { std::fs::read_to_string(HOSTS_PATH) }

#[cfg(test)]
mod tests {
    use super::*;

    const MAC: &str = "##\n# Host Database\n##\n127.0.0.1\tlocalhost\n255.255.255.255\tbroadcasthost\n::1             localhost\n";

    #[test]
    fn add_then_remove_roundtrip() {
        let added = add(MAC);
        assert!(is_applied(&added));
        assert!(added.starts_with(MAC));
        assert_eq!(added.lines().count(), MAC.lines().count() + 4);
        assert!(!added.contains("::1 serv"));
        assert_eq!(remove(&added), MAC);
        assert!(!is_applied(MAC));
    }

    #[test]
    fn entries_have_no_trailing_comment() {
        // macOS's resolver reads a trailing "# ..." as more host names
        for line in add(MAC).lines().filter(|l| l.starts_with("127.0.0.1 serv")) {
            assert!(!line.contains('#'), "{line}");
        }
    }

    #[test]
    fn add_is_idempotent() {
        assert_eq!(add(&add(MAC)), add(MAC));
    }

    #[test]
    fn keeps_crlf_and_missing_final_newline() {
        let win = "# Copyright (c) 1993-2009 Microsoft Corp.\r\n#\r\n# 127.0.0.1 example";
        let added = add(win);
        assert!(added.starts_with("# Copyright (c) 1993-2009 Microsoft Corp.\r\n#\r\n# 127.0.0.1 example\r\n"));
        assert!(added.contains("127.0.0.1 serv.ds.kakao.com\r\n"));
        assert!(!added.replace("\r\n", "").contains('\n'));
        assert_eq!(remove(&added), "# Copyright (c) 1993-2009 Microsoft Corp.\r\n#\r\n# 127.0.0.1 example\r\n");
    }

    #[test]
    fn leaves_other_lines_mentioning_the_hosts() {
        let text = format!("10.0.0.1 serv.ds.kakao.com\n{}", add(""));
        assert_eq!(remove(&text), "10.0.0.1 serv.ds.kakao.com\n");
        assert!(!is_applied("127.0.0.1 serv.ds.kakao.com\n127.0.0.1 display.ad.daum.net\n"));   // not ours: not applied
    }

    #[test]
    fn older_block_is_not_applied() {
        let old = format!("{MAC}{BEGIN}\n127.0.0.1 serv.ds.kakao.com\n::1 serv.ds.kakao.com\n127.0.0.1 display.ad.daum.net\n::1 display.ad.daum.net\n{END}\n");
        assert!(!is_applied(&old));
        assert_eq!(add(&old), add(MAC));
    }

    #[test]
    fn removes_pre_release_entries() {
        let text = format!("{MAC}127.0.0.1 serv.ds.kakao.com # KakaoAdHide\n::1 serv.ds.kakao.com # KakaoAdHide\n");
        assert_eq!(remove(&text), MAC);
    }

    #[test]
    fn empty_file() {
        let added = add("");
        assert!(is_applied(&added));
        assert_eq!(remove(&added), "");
    }
}
