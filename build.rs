//! Embeds the application icon, manifest and version information into the Windows exe.
//!
//! On other platforms this is a no-op; the icon is supplied by the desktop
//! environment (see `assets/icon.png`).

#[path = "build_support/windows_resources.rs"]
mod windows_resources;

fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    println!("cargo:rerun-if-changed=assets/app.manifest");
    println!("cargo:rerun-if-changed=build_support/windows_resources.rs");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        let bytes = windows_resources::compile(
            include_bytes!("assets/icon.ico"),
            include_bytes!("assets/app.manifest"),
            env!("CARGO_PKG_VERSION"),
        )
        .expect("invalid Windows application resources");
        let output = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap())
            .join("aurora-export.res");
        std::fs::write(&output, bytes).expect("could not write Windows application resources");
        println!("cargo:rustc-link-arg={}", output.display());
    } else {
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon("assets/icon.ico");
        resource.set_manifest_file("assets/app.manifest");
        resource.set("ProductName", "AuroraExport");
        resource.set(
            "FileDescription",
            "原神祈愿 / 崩坏：星穹铁道跃迁记录导出工具",
        );
        resource.set("LegalCopyright", "MIT License");
        resource.set("OriginalFilename", "aurora-export.exe");
        resource
            .compile()
            .expect("could not embed Windows resources; check the WINDRES toolchain");
    }
}
