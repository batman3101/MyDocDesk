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

fn main() -> eframe::Result<()> {
    let args = parse_args();
    let shot = args.screenshot.is_some();

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("MyDocDesk")
        .with_inner_size([1320.0, 840.0])
        .with_min_inner_size([980.0, 640.0]);
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
