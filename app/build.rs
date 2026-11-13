// On Windows, puts the app icon and the version information (from Cargo.toml) into the .exe,
// so Explorer, the taskbar and Settings > Apps show them.
fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico")
            .set("ProductName", "MyDocDesk")
            .set("FileDescription", "MyDocDesk")
            .set("OriginalFilename", "MyDocDesk.exe")
            .set("LegalCopyright", "Apache License 2.0");
        if let Err(e) = res.compile() {
            panic!("could not embed the Windows icon/version resource: {e}");
        }
    }
}
