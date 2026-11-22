// Version from VERSION (the one file both platforms and the release workflow read), plus the Windows exe's
// icon and version resource.
fn main() {
    let version = std::fs::read_to_string("VERSION").expect("VERSION").trim().to_string();
    assert!(version.split('.').count() == 3 && version.split('.').all(|p| p.parse::<u32>().is_ok()),
            "VERSION must look like 1.2.3");
    let version = std::env::var("KAKAOADHIDE_VERSION").unwrap_or(version);   // the release workflow may override
    println!("cargo:rustc-env=KAKAOADHIDE_VERSION={version}");
    println!("cargo:rerun-if-changed=VERSION");
    println!("cargo:rerun-if-changed=packaging/windows/app.ico");
    println!("cargo:rerun-if-env-changed=KAKAOADHIDE_VERSION");

    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        let v: Vec<u64> = version.split('.').map(|p| p.parse().unwrap()).collect();
        let packed = (v[0] << 48) | (v[1] << 32) | (v[2] << 16);
        res.set_version_info(winresource::VersionInfo::FILEVERSION, packed);
        res.set_version_info(winresource::VersionInfo::PRODUCTVERSION, packed);
        res.set("ProductName", "KakaoAdHide");
        res.set("FileDescription", "KakaoAdHide - empties the KakaoTalk ad banner");
        res.set("ProductVersion", &version);
        res.set("FileVersion", &version);
        res.set("LegalCopyright", "Copyright (c) 2026 pandemoniummm, MIT License");
        if std::path::Path::new("packaging/windows/app.ico").exists() { res.set_icon("packaging/windows/app.ico"); }
        res.compile().expect("Windows resources");
    }
}
