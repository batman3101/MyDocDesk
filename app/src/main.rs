#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod i18n;
mod ai;
mod analyze;
mod app;
mod cli;
mod demo;
mod docparse;
mod extract;
mod local;
mod lock;
mod model;
mod pipeline;
mod rag;
mod search;
mod share;
mod sync;
mod theme;

use std::path::PathBuf;

pub struct Args {
    pub screenshot: Option<PathBuf>,
    pub page: Option<String>,
}

fn parse_args() -> Args {
    let mut a = Args { screenshot: None, page: None };
    let mut it = std::env::args().skip(1);
    while let Some(x) = it.next() {
        match x.as_str() {
            "--screenshot" => a.screenshot = it.next().map(PathBuf::from),
            "--page" => a.page = it.next(),
            _ => {}
        }
    }
    a
}

/// The app icon (assets/icon.png) for the window and taskbar.
fn app_icon() -> egui::IconData {
    match image::load_from_memory_with_format(include_bytes!("../assets/icon.png"), image::ImageFormat::Png) {
        Ok(img) => {
            let img = img.into_rgba8();
            let (width, height) = img.dimensions();
            egui::IconData { rgba: img.into_raw(), width, height }
        }
        Err(_) => egui::IconData::default(),
    }
}

fn main() -> eframe::Result<()> {
    let args = parse_args();
    let shot = args.screenshot.is_some();

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("MyDocDesk")
        .with_inner_size([1320.0, 840.0])
        .with_min_inner_size([980.0, 640.0])
        .with_icon(std::sync::Arc::new(app_icon()));
    if shot {
        viewport = viewport.with_active(false);
        #[cfg(target_os = "windows")]
        {
            viewport = viewport.with_window_level(egui::WindowLevel::AlwaysOnBottom);
        }
    }

    #[allow(unused_mut)]
    let mut options = eframe::NativeOptions { viewport, ..Default::default() };
    if shot {
        options.persist_window = false;
    }
    #[cfg(target_os = "macos")]
    if shot {
        let hook: eframe::EventLoopBuilderHook = Box::new(|b| {
            use winit::platform::macos::EventLoopBuilderExtMacOS;
            b.with_activate_ignoring_other_apps(false);
        });
        options.event_loop_builder = Some(hook);
    }

    eframe::run_native(
        if shot { "MyDocDesk-screenshot" } else { "MyDocDesk" },
        options,
        Box::new(move |cc| {
            theme::install_fonts(&cc.egui_ctx);
            theme::apply(&cc.egui_ctx);
            Ok(Box::new(app::DeskApp::new(args)))
        }),
    )
}

#[cfg(test)]
mod icon_tests {
    /// The window/taskbar icon is the app icon (256x256, the blue rounded square), not eframe's default.
    #[test]
    fn window_icon_is_the_app_icon() {
        let i = super::app_icon();
        assert_eq!((i.width, i.height), (256, 256));
        assert_eq!(i.rgba.len(), 256 * 256 * 4);
        let px = |x: usize, y: usize| { let o = (y * 256 + x) * 4; [i.rgba[o], i.rgba[o + 1], i.rgba[o + 2], i.rgba[o + 3]] };
        assert_eq!(px(0, 0)[3], 0, "the corner is transparent (rounded)");
        assert_eq!(px(30, 128), [0x1f, 0x5b, 0xd1, 255], "the edge is the accent blue");
        assert_eq!(px(128, 100), [255, 255, 255, 255], "the middle is the white page");
    }
}

#[cfg(test)]
mod packaging_tests {
    fn read(rel: &str) -> String {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
        std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
    }

    /// The Windows installer script (Inno Setup) keeps the rules an installed app depends on.
    #[test]
    fn windows_installer_script_keeps_its_rules() {
        let iss = read("packaging/windows/MyDocDesk.iss");
        // Windows recognises updates by this id: it must never change
        assert!(iss.contains("AppId={{AA5D7B82-B99D-4CD3-858E-0858E85F77BB}"), "AppId changed");
        // the version comes only from Cargo.toml (build-installer.ps1 passes it in)
        assert!(iss.contains("AppVersion={#AppVersion}"));
        assert!(iss.contains("#error"), "building without a version must stop");
        assert!(!iss.lines().any(|l| l.trim_start().starts_with("AppVersion=") && !l.contains("{#AppVersion}")), "no fixed version");
        // installs for this user without asking for administrator rights
        assert!(iss.contains("PrivilegesRequired=lowest"));
        // removing the app never removes the person's documents and settings
        for data in ["{userappdata}", r"{localappdata}\MyDocDesk", r"{app}\.."] {
            assert!(!iss.contains(data), "the uninstaller must not touch {data}");
        }
        assert!(!iss.lines().any(|l| l.trim().eq_ignore_ascii_case("[UninstallDelete]")), "no extra files are deleted on uninstall");
    }

    /// The .exe must start on a PC without the Visual C++ runtime installed: the C runtime is linked in.
    #[test]
    fn windows_build_links_the_c_runtime_statically() {
        let cfg = read(".cargo/config.toml");
        assert!(cfg.contains("[target.x86_64-pc-windows-msvc]") && cfg.contains("+crt-static"));
    }
}
