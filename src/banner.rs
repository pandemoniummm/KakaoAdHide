//! What the local ad server answers in place of KakaoTalk's ad.
//!
//! KakaoTalk asks GET /sdk/banner on serv.ds.kakao.com for a JSON list of ads and shows the first one's HTML in a
//! web view at the bottom of its main window. An empty list makes it fall back to a built-in banner, so instead
//! this returns one "ad" whose HTML is just KakaoTalk's own background: the sidebar colour for the sidebar's
//! width, the list colour for the rest. KakaoTalk draws it inside its window, so it moves and resizes with the
//! window like any other part of it. No tracking events, and the real ad servers are never contacted.

/// KakaoTalk's sidebar width and background greys. macOS: measured on KakaoTalk for Mac 26.10 (dark; the light
/// values are its white list and light grey sidebar). Windows: measured on KakaoTalk 26.10 for Windows (light;
/// dark uses the macOS greys until measured).
#[cfg(target_os = "macos")]
pub const SIDEBAR_WIDTH: u32 = 78;
#[cfg(not(target_os = "macos"))]
pub const SIDEBAR_WIDTH: u32 = 81;
pub const DARK_SIDEBAR: &str = "#2d2d2d";
pub const DARK_LIST: &str = "#222222";
#[cfg(target_os = "macos")]
pub const LIGHT_SIDEBAR: &str = "#f2f2f2";
#[cfg(not(target_os = "macos"))]
pub const LIGHT_SIDEBAR: &str = "#ededed";
pub const LIGHT_LIST: &str = "#ffffff";

pub fn html() -> String {
    format!(
        "<!DOCTYPE html><html><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width\">\
<meta name=\"color-scheme\" content=\"light dark\"><style>\
html,body{{margin:0;height:100%;overflow:hidden;\
background:linear-gradient(to right,{DARK_SIDEBAR} {SIDEBAR_WIDTH}px,{DARK_LIST} {SIDEBAR_WIDTH}px)}}\
@media (prefers-color-scheme: light){{html,body{{\
background:linear-gradient(to right,{LIGHT_SIDEBAR} {SIDEBAR_WIDTH}px,{LIGHT_LIST} {SIDEBAR_WIDTH}px)}}}}\
</style></head><body></body></html>"
    )
}

/// The /sdk/banner response. refreshInterval is long: there is nothing to refresh.
pub fn response_json() -> String {
    serde_json::json!({
        "status": "OK",
        "id": "00000000-0000-0000-0000-000000000000",
        "ads": [{
            "content": html(),
            "mimeType": "text/html",
            "width": 400,
            "height": 100,
            "events": [],
            "type": "banner",
        }],
        "options": { "ext": { "refreshInterval": "3600" } },
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn response_shape() {
        let v: serde_json::Value = serde_json::from_str(&response_json()).unwrap();
        assert_eq!(v["status"], "OK");
        let ad = &v["ads"][0];
        assert_eq!(ad["mimeType"], "text/html");
        assert!(ad["events"].as_array().unwrap().is_empty());
        let html = ad["content"].as_str().unwrap();
        assert!(html.contains("#222222") && html.contains(&format!("{SIDEBAR_WIDTH}px")) && !html.contains("<script"));
    }
}
