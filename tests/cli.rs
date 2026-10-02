//! End-to-end checks of the headless command-line interface.

use rust_pose_studio::{cli, pose_io, presets, skeleton};

fn tmp(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("rps-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

fn args(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

#[test]
fn render_preset_writes_png() {
    let out = tmp("p5.png");
    let code = cli::run(&args(&["--render", "5", out.to_str().unwrap(), "--size", "256"]));
    assert_eq!(code, Some(0));
    let img = image::open(&out).unwrap();
    assert_eq!((img.width(), img.height()), (256, 256));
}

#[test]
fn contact_sheet_writes_png() {
    let out = tmp("sheet.png");
    let code = cli::run(&args(&["--contact-sheet", out.to_str().unwrap(), "--cell", "96", "--cols", "5"]));
    assert_eq!(code, Some(0));
    let img = image::open(&out).unwrap();
    assert!(img.width() >= 5 * 96 && img.height() >= 5 * 96);
}

#[test]
fn render_from_saved_json() {
    let skeleton = skeleton::Skeleton::new(skeleton::Proportions::default());
    let preset = &presets::presets(&skeleton)[2];
    let file = pose_io::PoseFile::from_pose(preset.name, &preset.pose, None, None);
    let json_path = tmp("pose.json");
    file.save(&json_path).unwrap();
    assert!(file.to_pose().is_ok());
    let out = tmp("from-json.png");
    let code = cli::run(&args(&["--render", json_path.to_str().unwrap(), out.to_str().unwrap(), "--size", "128"]));
    assert_eq!(code, Some(0));
    assert!(std::fs::metadata(&out).unwrap().len() > 100);
}

#[test]
fn no_args_means_gui() {
    assert_eq!(cli::run(&[]), None);
}
