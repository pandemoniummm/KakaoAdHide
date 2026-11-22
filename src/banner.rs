//! What the local ad server answers in place of KakaoTalk's ad.
//!
//! KakaoTalk asks GET /sdk/banner on serv.ds.kakao.com for a JSON list of ads and shows the first one's HTML in a
//! web view at the bottom of its main window. An empty list makes it fall back to a built-in banner, so instead
//! this returns one "ad" whose HTML is just KakaoTalk's own background: the sidebar colour for the sidebar's
//! width, the list colour for the rest. KakaoTalk draws it inside its window, so it moves and resizes with the
//! window like any other part of it. No tracking events, and the real ad servers are never contacted.

/// KakaoTalk's sidebar width and background greys. macOS: measured on KakaoTalk for Mac 26.10, dark and light
/// (light: #f5f5f5 here shows as the sidebar's on-screen #f7f7f7 after the web view's color conversion). Windows: measured on KakaoTalk 26.10 for Windows (light;
/// dark uses the macOS greys until measured).
#[cfg(target_os = "macos")]
pub const SIDEBAR_WIDTH: u32 = 78;
#[cfg(not(target_os = "macos"))]
pub const SIDEBAR_WIDTH: u32 = 81;
pub const DARK_SIDEBAR: &str = "#2d2d2d";
pub const DARK_LIST: &str = "#222222";
#[cfg(target_os = "macos")]
pub const LIGHT_SIDEBAR: &str = "#f5f5f5";
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
</style></head><body>{WEBVIEW_READY}</body></html>"
    )
}

/// KakaoTalk for Windows shows the ad in WebView2 and sends it "ReadyToListen"; an ad that doesn't answer
/// "AdLoaded" with result true counts as failed, and KakaoTalk swaps in its built-in fallback banner. Real ads
/// answer it the same way. Does nothing where there is no WebView2 (macOS).
const WEBVIEW_READY: &str = "<script>if(window.chrome&&window.chrome.webview){window.chrome.webview.addEventListener('message',\
function(a){try{if(a.data.event.name==='ReadyToListen'){window.chrome.webview.postMessage(\
{event:{name:'AdLoaded',data:{result:true}}})}}catch(e){}})}</script>";

/// Width of the ad slot as the real ad server reports it (Windows: 390, measured on KakaoTalk 26.10).
#[cfg(target_os = "macos")]
const AD_WIDTH: u32 = 400;
#[cfg(not(target_os = "macos"))]
const AD_WIDTH: u32 = 390;

/// Where the ad's event and feedback links point: back at this local server (serv.ds.kakao.com resolves here),
/// which answers them with 204. KakaoTalk for Windows rejects an ad without these fields and shows its fallback
/// banner, so they are filled in the shape the real server uses, but nothing ever leaves the computer.
const LOCAL: &str = "https://serv.ds.kakao.com/ev/";

/// The /sdk/banner response, in the field set of a real ad (KakaoTalk for Windows 26.10 checks more than macOS).
/// refreshInterval is long: there is nothing to refresh.
pub fn response_json() -> String {
    let events: Vec<_> = ["viewable", "hide", "click", "rendered"]
        .iter()
        .map(|t| serde_json::json!({ "type": t, "url": format!("{LOCAL}{t}") }))
        .collect();
    serde_json::json!({
        "status": "OK",
        "id": "00000000-0000-0000-0000-000000000000",
        "ads": [{
            "content": html(),
            "mimeType": "text/html",
            "width": AD_WIDTH,
            "height": 100,
            "events": events,
            "feedback": {
                "feedbacklinks": [format!("{LOCAL}fb")],
                "optoutpage": format!("{LOCAL}optout"),
                "reportpage": format!("{LOCAL}report"),
            },
            "dspId": "MOMENT",
            "bidId": "00000000000000000000000000000000",
            "etxId": "MOMENT:x",
            "cid": "1",
            "crid": "1",
            "adid": "1",
            "type": "banner",
        }],
        "options": {
            "ext": {
                "forceRefreshTime": "1895395637",
                "replayIntervalKey": "AD_1",
                "guaranteed": "N",
                "replayInterval": "1200",
                "refreshInterval": "3600",
            },
            "allowFpc": "N",
        },
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
        // every link the ad carries points back at the local server
        let links: Vec<String> = ad["events"].as_array().unwrap().iter().map(|e| e["url"].as_str().unwrap().to_string())
            .chain(ad["feedback"]["feedbacklinks"].as_array().unwrap().iter().map(|u| u.as_str().unwrap().to_string()))
            .chain(["optoutpage", "reportpage"].iter().map(|k| ad["feedback"][*k].as_str().unwrap().to_string()))
            .collect();
        assert_eq!(links.len(), 7);
        assert!(links.iter().all(|u| u.starts_with(LOCAL)), "{links:?}");
        let html = ad["content"].as_str().unwrap();
        assert!(html.contains("#222222") && html.contains(&format!("{SIDEBAR_WIDTH}px")));
        // the only script is the WebView2 handshake: no network access, no tracking
        assert_eq!(html.matches("<script").count(), 1);
        assert!(html.contains("AdLoaded"));
        for banned in ["http:", "https:", "//", "src=", "fetch", "XMLHttpRequest", "WebSocket", "sendBeacon", "import"] {
            assert!(!html.contains(banned), "blank ad must not load anything: found {banned}");
        }
    }
}
