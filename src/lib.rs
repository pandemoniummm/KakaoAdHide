//! KakaoAdHide: empties KakaoTalk's ad banner by answering its ad requests locally with a blank ad.
//! The platform-independent parts live here; src/platform has the per-OS setup, tray and install code.

pub mod banner;
pub mod certs;
pub mod feed;
pub mod hosts;
pub mod keys;
pub mod notices;
pub mod platform;
pub mod server;
pub mod setup;
pub mod ui;
pub mod updater;

pub const VERSION: &str = env!("TALKNAP_VERSION");
