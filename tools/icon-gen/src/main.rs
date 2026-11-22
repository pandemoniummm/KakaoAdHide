//! Renders assets/icon.svg into the app icon files. Run from the repository root:
//!
//!     cargo run --release --manifest-path tools/icon-gen/Cargo.toml
//!
//! Writes (and these are committed, so building the app needs no extra tools):
//! - packaging/windows/app.ico   16-256 px, full bleed (the exe, shortcuts, Installed apps, notifications)
//! - packaging/macos/AppIcon.icns 16-1024 px on the macOS icon grid (824 of 1024, centred), so it sits with
//!   other apps in the Dock and Finder
//! - assets/icon-256.png          for the README and the Windows notification icon

use resvg::{tiny_skia, usvg};
use std::path::Path;

fn render(tree: &usvg::Tree, size: u32, inset: f32) -> Vec<u8> {
    let mut pixmap = tiny_skia::Pixmap::new(size, size).expect("pixmap");
    let content = size as f32 * (1.0 - 2.0 * inset);
    let scale = content / tree.size().width();
    let offset = size as f32 * inset;
    let transform = tiny_skia::Transform::from_scale(scale, scale).post_translate(offset, offset);
    resvg::render(tree, transform, &mut pixmap.as_mut());
    pixmap.encode_png().expect("png")
}

/// ICO with PNG entries (Windows Vista and later read them at every size).
fn ico(images: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&[0, 0, 1, 0]);
    out.extend_from_slice(&(images.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * images.len() as u32;
    for (size, png) in images {
        let dim = if *size >= 256 { 0 } else { *size as u8 };
        out.extend_from_slice(&[dim, dim, 0, 0]);
        out.extend_from_slice(&1u16.to_le_bytes());   // planes
        out.extend_from_slice(&32u16.to_le_bytes());  // bits per pixel
        out.extend_from_slice(&(png.len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += png.len() as u32;
    }
    for (_, png) in images { out.extend_from_slice(png); }
    out
}

/// ICNS with PNG entries (macOS 10.7 and later).
fn icns(entries: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
    let body_len: usize = entries.iter().map(|(_, png)| 8 + png.len()).sum();
    let mut out = Vec::with_capacity(8 + body_len);
    out.extend_from_slice(b"icns");
    out.extend_from_slice(&((8 + body_len) as u32).to_be_bytes());
    for (kind, png) in entries {
        out.extend_from_slice(*kind);
        out.extend_from_slice(&((8 + png.len()) as u32).to_be_bytes());
        out.extend_from_slice(png);
    }
    out
}

fn main() {
    let svg = std::fs::read("assets/icon.svg").expect("run from the repository root (assets/icon.svg)");
    let tree = usvg::Tree::from_data(&svg, &usvg::Options::default()).expect("assets/icon.svg");

    let win: Vec<(u32, Vec<u8>)> = [16, 20, 24, 32, 40, 48, 64, 128, 256].iter().map(|&s| (s, render(&tree, s, 0.0))).collect();
    std::fs::write("packaging/windows/app.ico", ico(&win)).expect("app.ico");

    // macOS icon grid: the shape is 824 of 1024 px, centred (inset 100/1024 each side)
    let mac_inset = 100.0 / 1024.0;
    let m = |s: u32| render(&tree, s, mac_inset);
    let entries: Vec<(&[u8; 4], Vec<u8>)> = vec![
        (b"icp4", m(16)), (b"icp5", m(32)), (b"ic11", m(32)), (b"icp6", m(64)), (b"ic12", m(64)),
        (b"ic07", m(128)), (b"ic08", m(256)), (b"ic13", m(256)), (b"ic09", m(512)), (b"ic14", m(512)),
        (b"ic10", m(1024)),
    ];
    std::fs::write("packaging/macos/AppIcon.icns", icns(&entries)).expect("AppIcon.icns");

    std::fs::write("assets/icon-256.png", render(&tree, 256, 0.0)).expect("icon-256.png");
    for f in ["packaging/windows/app.ico", "packaging/macos/AppIcon.icns", "assets/icon-256.png"] {
        println!("{f}: {} bytes", Path::new(f).metadata().map(|m| m.len()).unwrap_or(0));
    }
}
