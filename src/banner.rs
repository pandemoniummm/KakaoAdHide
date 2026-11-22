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
///
/// The sidebar part is sized by the page itself (SIZE_SIDEBAR) as a fraction of the ad slot's height:
/// SIDEBAR_PER_SLOT = sidebar width / slot height, both as KakaoTalk draws them. They scale together, so this holds
/// whatever the monitor scale or KakaoTalk's own zoom; the web view's CSS pixels don't (on Windows with mixed
/// monitor scales its devicePixelRatio was 1.25 on one monitor and 1.875 on another). SIDEBAR_WIDTH is only the
/// fallback before the script runs, in CSS pixels at the default size.
/// Windows: sidebar 122 px, slot 188 px tall (measured at 150%). macOS: 78 pt over a 100 pt slot (measured on Retina, default size).
#[cfg(target_os = "macos")]
pub const SIDEBAR_PER_SLOT: (u32, u32) = (78, 100);
#[cfg(not(target_os = "macos"))]
pub const SIDEBAR_PER_SLOT: (u32, u32) = (122, 188);
#[cfg(target_os = "macos")]
pub const SIDEBAR_WIDTH: u32 = 78;
#[cfg(not(target_os = "macos"))]
pub const SIDEBAR_WIDTH: u32 = 65;
pub const DARK_SIDEBAR: &str = "#2d2d2d";
pub const DARK_LIST: &str = "#222222";
#[cfg(target_os = "macos")]
pub const LIGHT_SIDEBAR: &str = "#f5f5f5";
#[cfg(not(target_os = "macos"))]
pub const LIGHT_SIDEBAR: &str = "#ededed";
pub const LIGHT_LIST: &str = "#ffffff";

/// The menu button's size over the slot height, like SIDEBAR_PER_SLOT (Windows: KakaoTalk's sidebar icons are
/// 36 px in a 188 px slot at 150%; macOS: 18 pt in the 100 pt slot, KakaoTalk for Mac 26.10).
#[cfg(target_os = "macos")]
pub const ICON_PER_SLOT: (u32, u32) = (20, 100);
#[cfg(not(target_os = "macos"))]
pub const ICON_PER_SLOT: (u32, u32) = (36, 188);

/// On hover the button gets a round highlight like KakaoTalk's own sidebar icons (KakaoTalk for Mac 26.10: a
/// 36 pt circle, the same size as theirs with ICON_PER_SLOT 20/100): padding on each side as a fraction of the
/// icon size.
#[cfg(target_os = "macos")]
const HOVER_PAD: f64 = 0.4;
#[cfg(not(target_os = "macos"))]
const HOVER_PAD: f64 = 0.15;

/// The blank ad. With `menu` (the server's menu token) it also shows the menu button; clicking it asks MENU_URL on
/// this computer's KakaoAdHide, which opens its menu at the cursor. That one request is the only one the page can
/// make, and only when clicked. Without `menu`: no button, no requests at all.
pub fn html(menu: Option<&str>) -> String {
    let ((sn, sd), (inum, iden)) = (SIDEBAR_PER_SLOT, ICON_PER_SLOT);
    // Sidebar part width (--sb) and button size (--ic) from the slot height, again on every resize.
    let size = format!(
        "<script>var size=function(){{var s=document.documentElement.style;\
s.setProperty('--sb',(innerHeight*{sn}/{sd})+'px');s.setProperty('--ic',(innerHeight*{inum}/{iden})+'px')}};\
size();window.addEventListener('resize',size);</script>"
    );
    // The button sits in the sidebar part, under KakaoTalk's own sidebar icons, drawn like them: the app icon as
    // a grey line icon (assets/icon-line.svg: a filled chat bubble, and the emptied ad slot outlined below it).
    let button = match menu {
        Some(token) => format!(
            "<style>#kah{{position:fixed;top:50%;left:calc(var(--sb,{SIDEBAR_WIDTH}px)/2);width:var(--ic,19px);\
height:var(--ic,19px);padding:calc(var(--ic,19px)*{HOVER_PAD});border-radius:50%;\
transform:translate(-50%,-50%);color:#6f6f6f;cursor:default}}#kah:hover{{background:rgba(0,0,0,.06)}}\
@media (prefers-color-scheme: dark){{#kah{{color:#b9b9b9}}#kah:hover{{background:rgba(255,255,255,.05)}}}}</style>\
<svg id=\"kah\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" \
stroke-linecap=\"round\"><title>KakaoAdHide</title><rect x=\"2\" y=\"3\" width=\"20\" height=\"8.5\" rx=\"4.25\" fill=\"currentColor\" stroke=\"none\"/>\
<rect x=\"3\" y=\"13.5\" width=\"18\" height=\"6.5\" rx=\"3.25\"/></svg>\
<script>document.getElementById('kah').addEventListener('click',function(){{\
fetch('{MENU_URL}?t={token}',{{mode:'no-cors',cache:'no-store'}}).catch(function(){{}})}});\
document.addEventListener('contextmenu',function(e){{e.preventDefault()}});</script>"
        ),
        None => String::new(),
    };
    format!(
        "<!DOCTYPE html><html><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width\">\
<meta name=\"color-scheme\" content=\"light dark\"><style>\
html,body{{margin:0;height:100%;overflow:hidden;\
background:linear-gradient(to right,{DARK_SIDEBAR} var(--sb,{SIDEBAR_WIDTH}px),{DARK_LIST} var(--sb,{SIDEBAR_WIDTH}px))}}\
@media (prefers-color-scheme: light){{html,body{{\
background:linear-gradient(to right,{LIGHT_SIDEBAR} var(--sb,{SIDEBAR_WIDTH}px),{LIGHT_LIST} var(--sb,{SIDEBAR_WIDTH}px))}}}}\
</style></head><body>{size}{button}{WEBVIEW_READY}</body></html>"
    )
}

/// KakaoTalk for Windows shows the ad in WebView2 and sends it "ReadyToListen"; an ad that doesn't answer
/// "AdLoaded" with result true counts as failed, and KakaoTalk swaps in its built-in fallback banner. Real ads
/// answer it the same way. Does nothing where there is no WebView2 (macOS).
const WEBVIEW_READY: &str = "<script>if(window.chrome&&window.chrome.webview){\
window.chrome.webview.addEventListener('message',\
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

/// The blank ad's logo asks this to open the app's menu (see server::MENU_PATH).
pub const MENU_URL: &str = "https://serv.ds.kakao.com/kakaoadhide/menu";

/// KakaoTalk's ad placements that get the blank ad: the bottom banner, which can't be removed, only filled.
/// Every other placement (Windows: a popup by the tray on the first start of the day) gets no ad, so it doesn't
/// open. macOS: only the bottom banner (DAN-ECTpSP1g9fCcyreq on KakaoTalk for Mac 26.10) was ever requested, also
/// on the first start of the day, so every placement keeps the blank ad: if KakaoTalk changed the banner's id, a
/// list would turn the slot into KakaoTalk's fallback banner instead.
#[cfg(windows)]
pub const BOTTOM_BANNER_UNITS: &[&str] = &["DAN-FTsOAPU4yZ6oHLGy"];   // KakaoTalk for Windows 26.10

pub fn is_bottom_banner(unit: Option<&str>) -> bool {
    #[cfg(windows)]
    { unit.is_none_or(|u| BOTTOM_BANNER_UNITS.contains(&u)) }   // no id: the old request shape, a banner
    #[cfg(not(windows))]
    { let _ = unit; true }
}

/// "No ad": an empty list. For a popup placement that means nothing to show.
pub fn no_ad_json() -> String {
    serde_json::json!({ "status": "OK", "id": "00000000-0000-0000-0000-000000000000", "ads": [] }).to_string()
}

/// The /sdk/banner response, in the field set of a real ad (KakaoTalk for Windows 26.10 checks more than macOS).
/// refreshInterval is a day: there is nothing to refresh (KakaoTalk asks again by itself; this only sets when).
pub fn response_json(menu: Option<&str>) -> String {
    let events: Vec<_> = ["viewable", "hide", "click", "rendered"]
        .iter()
        .map(|t| serde_json::json!({ "type": t, "url": format!("{LOCAL}{t}") }))
        .collect();
    serde_json::json!({
        "status": "OK",
        "id": "00000000-0000-0000-0000-000000000000",
        "ads": [{
            "content": html(menu),
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
                "refreshInterval": "86400",
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
        let v: serde_json::Value = serde_json::from_str(&response_json(None)).unwrap();
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
        // the only scripts size the sidebar and answer the WebView2 handshake: no network access, no tracking
        assert_eq!(html.matches("<script").count(), 2);
        let ((num, den), (inum, iden)) = (SIDEBAR_PER_SLOT, ICON_PER_SLOT);
        assert!(html.contains(&format!("innerHeight*{num}/{den}")) && html.contains("var(--sb,"));
        assert!(html.contains(&format!("innerHeight*{inum}/{iden}")));
        assert!(html.contains("AdLoaded"));
        for banned in ["http:", "https:", "//", "src=", "fetch", "XMLHttpRequest", "WebSocket", "sendBeacon", "import"] {
            assert!(!html.contains(banned), "blank ad must not load anything: found {banned}");
        }
    }

    #[test]
    fn menu_logo_makes_only_the_local_menu_request() {
        assert!(!html(None).contains("kah"));
        let html = html(Some("0123abcd"));
        assert_eq!(html.matches("<script").count(), 3);
        // exactly one address, the menu on this computer, with the token
        assert_eq!(html.matches("//").count(), 1);
        assert_eq!(html.matches("fetch(").count(), 1);
        assert!(html.contains(&format!("{MENU_URL}?t=0123abcd")));
        assert!(MENU_URL.starts_with("https://serv.ds.kakao.com/") && MENU_URL.ends_with(crate::server::MENU_PATH));
        for banned in ["http:", "src=", "XMLHttpRequest", "WebSocket", "sendBeacon", "import"] {
            assert!(!html.contains(banned), "found {banned}");
        }
    }
}
