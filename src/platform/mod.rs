//! Per-OS parts: where data lives, settings, certificate trust, the hosts file (as administrator), install,
//! login item, single instance and update install. Both modules offer the same functions.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::*;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::*;
