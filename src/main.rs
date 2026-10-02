// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;

use eframe::egui;

fn icon() -> Option<egui::IconData> {
    use rust_pose_studio::{body::Body, export, presets::presets, render::framing_camera, skeleton::*};
    let skel = Skeleton::new(Proportions::default());
    let p = presets(&skel).into_iter().nth(10)?;
    let cam = framing_camera(&Body::new(&skel, &p.pose), 20.0, 5.0, true);
    let opts = export::ImageOptions { width: 64, height: 64, ..Default::default() };
    let pm = export::render_pose(&skel, &p.pose, &cam, &opts).ok()?;
    let rgba = pm.pixels().iter().flat_map(|c| {
        let c = c.demultiply();
        [c.red(), c.green(), c.blue(), c.alpha()]
    });
    Some(egui::IconData { rgba: rgba.collect(), width: 64, height: 64 })
}

/// Release builds use the GUI subsystem on Windows; re-attach to the parent console so
/// the command-line interface can print when started from a terminal.
#[cfg(windows)]
fn attach_parent_console() {
    unsafe extern "system" {
        fn AttachConsole(process_id: u32) -> i32;
    }
    const ATTACH_PARENT_PROCESS: u32 = u32::MAX;
    // SAFETY: plain Win32 call without pointers; failure (no parent console) is harmless.
    unsafe {
        AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    #[cfg(windows)]
    if !args.is_empty() {
        attach_parent_console();
    }
    if let Some(code) = rust_pose_studio::cli::run(&args) {
        std::process::exit(code);
    }
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Rust Pose Studio — drawing mannequin")
        .with_inner_size([1360.0, 860.0])
        .with_min_inner_size([900.0, 600.0])
        .with_app_id("rust-pose-studio");
    if let Some(i) = icon() {
        viewport = viewport.with_icon(i);
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native("rust-pose-studio", options, Box::new(|cc| Ok(Box::new(app::PoseApp::new(cc)))))
}
