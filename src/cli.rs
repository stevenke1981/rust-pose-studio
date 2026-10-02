//! Headless command line interface (exports without opening a window).

use std::path::PathBuf;

use crate::body::Body;
use crate::export::{ImageOptions, SheetItem, SheetOptions, preset_items, render_contact_sheet, render_pose, save_png};
use crate::pose_io::PoseFile;
use crate::presets::presets;
use crate::render::{Camera, FloorStyle, framing_camera};
use crate::skeleton::{Proportions, Skeleton};

pub const USAGE: &str = "\
rust-pose-studio — posable drawing mannequin

USAGE:
  rust-pose-studio                         start the GUI
  rust-pose-studio --contact-sheet OUT.png [--cell PX] [--cols N] [--yaw DEG] [--pitch DEG]
                   [--transparent] [--no-names] [--title TEXT] [--only 1,5,9]
  rust-pose-studio --render POSE OUT.png   POSE = preset number (1-25) or a pose .json file
                   [--size PX] [--yaw DEG] [--pitch DEG] [--transparent] [--grid]
                   [--turnaround]   four views (front, left, back, right) side by side
  rust-pose-studio --list                  list the built-in presets
  rust-pose-studio --floor-report          show which body parts touch the floor per preset
  rust-pose-studio --help | --version
";

fn value<T: std::str::FromStr>(args: &[String], flag: &str) -> Result<Option<T>, String> {
    match args.iter().position(|a| a == flag) {
        None => Ok(None),
        Some(i) => {
            let v = args.get(i + 1).ok_or(format!("{flag} needs a value"))?;
            v.parse().map(Some).map_err(|_| format!("invalid value for {flag}: {v}"))
        }
    }
}

/// Run the CLI if any arguments were given. Returns `None` to start the GUI.
pub fn run(args: &[String]) -> Option<i32> {
    let first = args.first()?;
    let result = match first.as_str() {
        "--help" | "-h" => {
            print!("{USAGE}");
            Ok(())
        }
        "--version" | "-V" => {
            println!("rust-pose-studio {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        "--list" => {
            let s = Skeleton::new(Proportions::default());
            for (i, p) in presets(&s).iter().enumerate() {
                println!("{:>2}  {}", i + 1, p.name);
            }
            Ok(())
        }
        "--floor-report" => {
            floor_report();
            Ok(())
        }
        "--contact-sheet" => contact_sheet(args),
        "--render" => render(args),
        other => Err(format!("unknown argument: {other}\n\n{USAGE}")),
    };
    Some(match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("error: {e}");
            2
        }
    })
}

fn contact_sheet(args: &[String]) -> Result<(), String> {
    let out: PathBuf = args.get(1).ok_or("missing output path")?.into();
    let skel = Skeleton::new(Proportions::default());
    let only: Option<Vec<usize>> = value::<String>(args, "--only")?
        .map(|s| s.split(',').filter_map(|n| n.trim().parse::<usize>().ok()).map(|n| n.saturating_sub(1)).collect());
    let mut items = preset_items(&skel, only.as_deref());
    let yaw: Option<f32> = value(args, "--yaw")?;
    let pitch: Option<f32> = value(args, "--pitch")?;
    for it in &mut items {
        if let Some(y) = yaw {
            it.yaw = y;
        }
        if let Some(p) = pitch {
            it.pitch = p;
        }
    }
    let opts = SheetOptions {
        columns: value(args, "--cols")?.unwrap_or(5),
        cell: value(args, "--cell")?.unwrap_or(360),
        transparent: args.iter().any(|a| a == "--transparent"),
        names: !args.iter().any(|a| a == "--no-names"),
        title: value(args, "--title")?,
        ..SheetOptions::default()
    };
    let pm = render_contact_sheet(&skel, &items, &opts)?;
    save_png(&pm, &out)?;
    println!("wrote {} ({}x{})", out.display(), pm.width(), pm.height());
    Ok(())
}

fn render(args: &[String]) -> Result<(), String> {
    let what = args.get(1).ok_or("missing pose")?;
    let out: PathBuf = args.get(2).ok_or("missing output path")?.into();
    let mut props = Proportions::default();
    let (pose, mut yaw, mut pitch) = if let Ok(n) = what.parse::<usize>() {
        let skel = Skeleton::new(props);
        let list = presets(&skel);
        let p = list.get(n.wrapping_sub(1)).ok_or(format!("preset number must be 1-{}", list.len()))?;
        (p.pose.clone(), p.yaw, p.pitch)
    } else {
        let f = PoseFile::load(std::path::Path::new(what))?;
        if let Some(p) = f.proportions {
            props = p;
        }
        let cam = f.camera.unwrap_or_default();
        (f.to_pose()?, cam.yaw, cam.pitch)
    };
    yaw = value(args, "--yaw")?.unwrap_or(yaw);
    pitch = value(args, "--pitch")?.unwrap_or(pitch);
    let skel = Skeleton::new(props);
    let size: u32 = value(args, "--size")?.unwrap_or(1024);
    if args.iter().any(|a| a == "--turnaround") {
        let items: Vec<SheetItem> = [("Front", 0.0), ("Left", 90.0), ("Back", 180.0), ("Right", 270.0), ("3/4", yaw)]
            .iter()
            .enumerate()
            .map(|(i, (n, y))| SheetItem { number: i + 1, name: n.to_string(), pose: pose.clone(), yaw: *y, pitch })
            .collect();
        let opts = SheetOptions { columns: 5, cell: size / 2, numbers: false, ..SheetOptions::default() };
        let pm = render_contact_sheet(&skel, &items, &opts)?;
        save_png(&pm, &out)?;
        println!("wrote {} ({}x{})", out.display(), pm.width(), pm.height());
        return Ok(());
    }
    let body = Body::new(&skel, &pose);
    let cam: Camera = framing_camera(&body, yaw, pitch, true);
    let opts = ImageOptions {
        width: size,
        height: size,
        transparent: args.iter().any(|a| a == "--transparent"),
        floor: if args.iter().any(|a| a == "--grid") { FloorStyle::Grid } else { FloorStyle::Shadow },
        ..ImageOptions::default()
    };
    let pm = render_pose(&skel, &pose, &cam, &opts)?;
    save_png(&pm, &out)?;
    println!("wrote {} ({}x{})", out.display(), pm.width(), pm.height());
    Ok(())
}

fn floor_report() {
    let s = Skeleton::new(Proportions::default());
    for (i, p) in presets(&s).iter().enumerate() {
        let body = Body::new(&s, &p.pose);
        let mut touching: Vec<String> = body
            .parts
            .iter()
            .filter(|part| part.volume.min_y() < 0.025)
            .map(|part| format!("{}({:.0}mm)", part.joint.key(), part.volume.min_y() * 1000.0))
            .collect();
        touching.sort();
        println!("{:>2} {:<34} pelvis y={:.2}  {}", i + 1, p.name, p.pose.root.y, touching.join(" "));
    }
}
