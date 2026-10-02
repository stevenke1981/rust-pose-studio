//! The egui front-end.
//!
//! Layout: menu bar, preset gallery on the left, 3D viewport in the centre,
//! inspector (joint / pose / body / camera / display / export) on the right and
//! a status bar. Exports run on background threads.

mod inspector;
mod paint;
mod theme;
mod viewport;

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use eframe::egui::{self, Color32, RichText};
use rust_pose_studio::body::{Body, snap_to_floor};
use rust_pose_studio::export::{self, ImageOptions, SheetItem, SheetOptions};
use rust_pose_studio::pose_io::PoseFile;
use rust_pose_studio::posing::random_pose;
use rust_pose_studio::presets::{Preset, presets};
use rust_pose_studio::render::{Camera, DrawList, FloorStyle, Framing, Style, View, draw_body, framing_camera};
use rust_pose_studio::skeleton::{Joint, Pose, Proportions, Skeleton};

const UNDO_LIMIT: usize = 200;

#[derive(Clone, Debug, PartialEq)]
struct Snapshot {
    pose: Pose,
    props: Proportions,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Drag {
    /// Dragging a joint handle; `depth` is the view depth of the handle when grabbed,
    /// `offset` the screen offset from the pointer to the handle centre.
    Joint {
        joint: Joint,
        depth: f32,
        offset: egui::Vec2,
    },
    Orbit,
    Pan,
}

#[derive(Clone, Debug)]
struct ExportSettings {
    size: u32,
    transparent: bool,
    fit: bool,
    match_view: bool,
    floor: FloorStyle,
    sheet_cols: u32,
    sheet_cell: u32,
    sheet_numbers: bool,
    sheet_names: bool,
    sheet_title: String,
    sheet_current_angle: bool,
    sheet_add_current: bool,
}

impl Default for ExportSettings {
    fn default() -> Self {
        ExportSettings {
            size: 2048,
            transparent: false,
            fit: true,
            match_view: false,
            floor: FloorStyle::None,
            sheet_cols: 5,
            sheet_cell: 400,
            sheet_numbers: true,
            sheet_names: false,
            sheet_title: String::new(),
            sheet_current_angle: false,
            sheet_add_current: false,
        }
    }
}

struct Job {
    label: String,
    rx: mpsc::Receiver<Result<String, String>>,
}

pub struct PoseApp {
    props: Proportions,
    skel: Skeleton,
    pose: Pose,
    pose_name: String,
    camera: Camera,
    selected: Option<Joint>,
    hovered: Option<Joint>,
    drag: Option<Drag>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    stable: Snapshot,
    presets: Vec<Preset>,
    thumbs: Vec<DrawList>,
    presets_for: Proportions,
    current_preset: Option<usize>,
    sheet_selection: BTreeSet<usize>,
    ik: bool,
    snap: bool,
    show_handles: bool,
    floor: FloorStyle,
    view_cache: Option<ViewCache>,
    export: ExportSettings,
    jobs: Vec<Job>,
    status: Option<(bool, String)>,
    view_size: egui::Vec2,
    seed: u64,
    show_help: bool,
    show_about: bool,
}

impl PoseApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::apply(&cc.egui_ctx);
        let props = Proportions::default();
        let skel = Skeleton::new(props);
        let presets = presets(&skel);
        let thumbs = make_thumbs(&skel, &presets);
        let pose = presets[0].pose.clone();
        let mut app = PoseApp {
            props,
            skel,
            pose: pose.clone(),
            pose_name: presets[0].name.to_string(),
            camera: Camera { yaw: presets[0].yaw, pitch: presets[0].pitch.max(8.0), ..Camera::default() },
            selected: None,
            hovered: None,
            drag: None,
            undo: Vec::new(),
            redo: Vec::new(),
            stable: Snapshot { pose, props },
            presets,
            thumbs,
            presets_for: props,
            current_preset: Some(0),
            sheet_selection: BTreeSet::new(),
            ik: true,
            snap: true,
            show_handles: true,
            floor: FloorStyle::Grid,
            view_cache: None,
            export: ExportSettings::default(),
            jobs: Vec::new(),
            status: None,
            view_size: egui::vec2(800.0, 600.0),
            seed: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(7),
            show_help: false,
            show_about: false,
        };
        app.frame_camera();
        app
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot { pose: self.pose.clone(), props: self.props }
    }

    fn restore(&mut self, s: Snapshot) {
        self.pose = s.pose.clone();
        if self.props != s.props {
            self.props = s.props;
            self.skel = Skeleton::new(self.props);
        }
        self.stable = s;
    }

    fn undo(&mut self) {
        if let Some(s) = self.undo.pop() {
            self.redo.push(self.snapshot());
            self.restore(s);
            self.current_preset = None;
        }
    }

    fn redo(&mut self) {
        if let Some(s) = self.redo.pop() {
            self.undo.push(self.snapshot());
            self.restore(s);
            self.current_preset = None;
        }
    }

    /// Record an undo step once the user has finished an edit (no pointer button held).
    fn commit_history(&mut self, ctx: &egui::Context) {
        let busy = ctx.input(|i| i.pointer.any_down());
        if busy || self.drag.is_some() {
            return;
        }
        let now = self.snapshot();
        if now != self.stable {
            let prev = std::mem::replace(&mut self.stable, now);
            self.undo.push(prev);
            if self.undo.len() > UNDO_LIMIT {
                self.undo.remove(0);
            }
            self.redo.clear();
        }
    }

    fn set_props(&mut self, props: Proportions) {
        self.props = props.sanitized();
        self.skel = Skeleton::new(self.props);
        if self.snap {
            snap_to_floor(&self.skel, &mut self.pose);
        }
    }

    fn refresh_presets(&mut self) {
        if self.presets_for != self.props {
            self.presets = presets(&self.skel);
            self.thumbs = make_thumbs(&self.skel, &self.presets);
            self.presets_for = self.props;
        }
    }

    fn load_preset(&mut self, i: usize) {
        self.refresh_presets();
        if let Some(p) = self.presets.get(i).cloned() {
            self.pose = p.pose.clone();
            self.pose_name = p.name.to_string();
            self.current_preset = Some(i);
            self.camera.yaw = p.yaw;
            self.camera.pitch = p.pitch.max(8.0);
            self.frame_camera();
            self.set_status(true, format!("Loaded preset {} – {}", i + 1, p.name));
        }
    }

    /// Point the camera at the figure and pick a distance that fits it.
    fn frame_camera(&mut self) {
        let body = Body::new(&self.skel, &self.pose);
        let fit = framing_camera(&body, self.camera.yaw, self.camera.pitch, self.camera.perspective);
        let half = (self.camera.fov.to_radians() * 0.5).tan();
        let (lo, hi) = body.bounds();
        let size = (hi - lo).length().max(0.5);
        self.camera.target = fit.target;
        self.camera.distance = (size * 0.62 / half).clamp(1.0, 30.0);
    }

    fn set_status(&mut self, ok: bool, msg: impl Into<String>) {
        self.status = Some((ok, msg.into()));
    }

    fn reset_pose(&mut self) {
        self.pose = Pose::rest(&self.skel);
        self.pose_name = "Standing".into();
        self.current_preset = None;
    }

    fn mirror_pose(&mut self) {
        self.pose = self.pose.mirrored();
        self.selected = self.selected.map(Joint::mirror);
        self.current_preset = None;
    }

    fn random(&mut self) {
        self.seed = self.seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.pose = random_pose(&self.skel, self.seed);
        self.pose_name = "Random".into();
        self.current_preset = None;
    }

    // ------------------------------------------------------------------ files

    fn save_pose_dialog(&mut self) {
        let name = if self.pose_name.is_empty() { "pose".to_string() } else { sanitize(&self.pose_name) };
        let Some(path) =
            rfd::FileDialog::new().add_filter("Pose JSON", &["json"]).set_file_name(format!("{name}.json")).save_file()
        else {
            return;
        };
        let file = PoseFile::from_pose(&self.pose_name, &self.pose, Some(self.props), Some(self.camera));
        match file.save(&path) {
            Ok(()) => self.set_status(true, format!("Saved pose to {}", path.display())),
            Err(e) => self.set_status(false, e),
        }
    }

    fn open_pose_dialog(&mut self) {
        let Some(path) = rfd::FileDialog::new().add_filter("Pose JSON", &["json"]).pick_file() else { return };
        self.open_pose(path);
    }

    fn open_pose(&mut self, path: PathBuf) {
        match PoseFile::load(&path).and_then(|f| f.to_pose().map(|p| (f, p))) {
            Ok((file, pose)) => {
                if let Some(props) = file.proportions {
                    self.set_props(props);
                }
                if let Some(cam) = file.camera {
                    self.camera = cam;
                }
                self.pose = pose;
                if self.snap {
                    snap_to_floor(&self.skel, &mut self.pose);
                }
                self.pose_name = if file.name.is_empty() {
                    path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
                } else {
                    file.name
                };
                self.current_preset = None;
                self.set_status(true, format!("Opened {}", path.display()));
            }
            Err(e) => self.set_status(false, e),
        }
    }

    fn spawn_job(
        &mut self,
        ctx: &egui::Context,
        label: String,
        work: impl FnOnce() -> Result<String, String> + Send + 'static,
    ) {
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let r = work();
            let _ = tx.send(r);
            ctx.request_repaint();
        });
        self.set_status(true, format!("{label}…"));
        self.jobs.push(Job { label, rx });
    }

    fn poll_jobs(&mut self) {
        let mut done = Vec::new();
        self.jobs.retain(|j| match j.rx.try_recv() {
            Ok(r) => {
                done.push(r);
                false
            }
            Err(mpsc::TryRecvError::Empty) => true,
            Err(mpsc::TryRecvError::Disconnected) => {
                done.push(Err(format!("{} failed unexpectedly", j.label)));
                false
            }
        });
        for r in done {
            match r {
                Ok(m) => self.set_status(true, m),
                Err(e) => self.set_status(false, e),
            }
        }
    }

    fn export_png_dialog(&mut self, ctx: &egui::Context) {
        let name = sanitize(if self.pose_name.is_empty() { "pose" } else { &self.pose_name });
        let Some(path) =
            rfd::FileDialog::new().add_filter("PNG image", &["png"]).set_file_name(format!("{name}.png")).save_file()
        else {
            return;
        };
        let e = &self.export;
        let (w, h) = if e.match_view && self.view_size.x > 1.0 {
            let aspect = self.view_size.y / self.view_size.x;
            (e.size, ((e.size as f32) * aspect).round().max(16.0) as u32)
        } else {
            (e.size, e.size)
        };
        let opts = ImageOptions { width: w, height: h, transparent: e.transparent, fit: e.fit, floor: e.floor };
        let (skel, pose, camera) = (self.skel.clone(), self.pose.clone(), self.camera);
        self.spawn_job(ctx, format!("Exporting {}", path.display()), move || {
            let pm = export::render_pose(&skel, &pose, &camera, &opts)?;
            export::save_png(&pm, &path)?;
            Ok(format!("Exported {}x{} PNG to {}", pm.width(), pm.height(), path.display()))
        });
    }

    fn export_sheet_dialog(&mut self, ctx: &egui::Context) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("PNG image", &["png"])
            .set_file_name("pose-contact-sheet.png")
            .save_file()
        else {
            return;
        };
        self.refresh_presets();
        let e = self.export.clone();
        let mut items: Vec<SheetItem> = self
            .presets
            .iter()
            .enumerate()
            .filter(|(i, _)| self.sheet_selection.is_empty() || self.sheet_selection.contains(i))
            .map(|(i, p)| SheetItem {
                number: i + 1,
                name: p.name.to_string(),
                pose: p.pose.clone(),
                yaw: if e.sheet_current_angle { self.camera.yaw } else { p.yaw },
                pitch: if e.sheet_current_angle { self.camera.pitch } else { p.pitch },
            })
            .collect();
        if e.sheet_add_current {
            items.push(SheetItem {
                number: items.last().map(|i| i.number + 1).unwrap_or(1).max(self.presets.len() + 1),
                name: if self.pose_name.is_empty() { "Current pose".into() } else { self.pose_name.clone() },
                pose: self.pose.clone(),
                yaw: self.camera.yaw,
                pitch: self.camera.pitch,
            });
        }
        let opts = SheetOptions {
            columns: e.sheet_cols,
            cell: e.sheet_cell,
            transparent: e.transparent,
            numbers: e.sheet_numbers,
            names: e.sheet_names,
            floor: if e.floor == FloorStyle::Grid { FloorStyle::None } else { e.floor },
            title: (!e.sheet_title.trim().is_empty()).then(|| e.sheet_title.trim().to_string()),
        };
        let skel = self.skel.clone();
        let n = items.len();
        self.spawn_job(ctx, format!("Exporting contact sheet ({n} poses)"), move || {
            let pm = export::render_contact_sheet(&skel, &items, &opts)?;
            export::save_png(&pm, &path)?;
            Ok(format!("Exported contact sheet ({n} poses, {}x{}) to {}", pm.width(), pm.height(), path.display()))
        });
    }

    // ------------------------------------------------------------------ panels

    fn menu_bar(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("New (standing pose)").clicked() {
                    self.reset_pose();
                    ui.close();
                }
                if ui.button("Open pose…    Ctrl+O").clicked() {
                    ui.close();
                    self.open_pose_dialog();
                }
                if ui.button("Save pose…    Ctrl+S").clicked() {
                    ui.close();
                    self.save_pose_dialog();
                }
                ui.separator();
                if ui.button("Export PNG…    Ctrl+E").clicked() {
                    ui.close();
                    self.export_png_dialog(&ctx);
                }
                if ui.button("Export contact sheet…").clicked() {
                    ui.close();
                    self.export_sheet_dialog(&ctx);
                }
                ui.separator();
                if ui.button("Quit").clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            ui.menu_button("Edit", |ui| {
                if ui.add_enabled(!self.undo.is_empty(), egui::Button::new("Undo    Ctrl+Z")).clicked() {
                    self.undo();
                    ui.close();
                }
                if ui.add_enabled(!self.redo.is_empty(), egui::Button::new("Redo    Ctrl+Y")).clicked() {
                    self.redo();
                    ui.close();
                }
                ui.separator();
                if ui.button("Mirror pose L↔R    M").clicked() {
                    self.mirror_pose();
                    ui.close();
                }
                if ui.add_enabled(self.selected.is_some(), egui::Button::new("Reset selected joint    R")).clicked() {
                    if let Some(j) = self.selected {
                        self.pose.set(j, [0.0; 3]);
                    }
                    ui.close();
                }
                if ui.button("Reset to standing").clicked() {
                    self.reset_pose();
                    ui.close();
                }
                if ui.button("Random pose").clicked() {
                    self.random();
                    ui.close();
                }
            });
            ui.menu_button("View", |ui| {
                for (name, yaw, pitch) in VIEWS {
                    if ui.button(name).clicked() {
                        self.camera.yaw = yaw;
                        self.camera.pitch = pitch;
                        self.frame_camera();
                        ui.close();
                    }
                }
                ui.separator();
                if ui.button("Frame figure    F").clicked() {
                    self.frame_camera();
                    ui.close();
                }
                ui.checkbox(&mut self.camera.perspective, "Perspective");
                ui.checkbox(&mut self.show_handles, "Show joint handles");
            });
            ui.menu_button("Help", |ui| {
                if ui.button("Controls…").clicked() {
                    self.show_help = true;
                    ui.close();
                }
                if ui.button("About…").clicked() {
                    self.show_about = true;
                    ui.close();
                }
            });
        });
    }

    fn gallery(&mut self, ui: &mut egui::Ui) {
        self.refresh_presets();
        ui.horizontal(|ui| {
            ui.heading("Presets");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if !self.sheet_selection.is_empty()
                    && ui.small_button("Clear").on_hover_text("Clear contact-sheet selection").clicked()
                {
                    self.sheet_selection.clear();
                }
            });
        });
        let sel = self.sheet_selection.len();
        ui.label(
            RichText::new(if sel == 0 {
                "Click to load · Ctrl+click to pick for the contact sheet".to_string()
            } else {
                format!("{sel} picked for the contact sheet")
            })
            .small()
            .weak(),
        );
        ui.add_space(4.0);
        let cols = ((ui.available_width() + 6.0) / 118.0).floor().max(1.0) as usize;
        let cell = ((ui.available_width() - (cols as f32 - 1.0) * 6.0) / cols as f32).floor();
        let mut clicked = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            egui::Grid::new("preset_grid").spacing([6.0, 6.0]).show(ui, |ui| {
                for i in 0..self.presets.len() {
                    let (rect, resp) = ui.allocate_exact_size(egui::vec2(cell, cell), egui::Sense::click());
                    let painter = ui.painter_at(rect);
                    painter.rect_filled(rect, 4.0, Color32::WHITE);
                    let list = &self.thumbs[i];
                    let inner = rect.shrink(4.0);
                    let frame = rust_pose_studio::render::Frame2::fit(
                        list,
                        inner.left(),
                        inner.top() + 10.0,
                        inner.width(),
                        inner.height() - 12.0,
                        0.04,
                    );
                    paint::draw(&painter, list, &frame, cell * 1.6, None);
                    painter.text(
                        rect.left_top() + egui::vec2(5.0, 3.0),
                        egui::Align2::LEFT_TOP,
                        format!("{}", i + 1),
                        egui::FontId::proportional(12.0),
                        Color32::from_gray(30),
                    );
                    let picked = self.sheet_selection.contains(&i);
                    if picked {
                        painter.circle_filled(rect.right_top() + egui::vec2(-9.0, 9.0), 6.0, theme::ACCENT);
                        painter.text(
                            rect.right_top() + egui::vec2(-9.0, 9.0),
                            egui::Align2::CENTER_CENTER,
                            "✔",
                            egui::FontId::proportional(9.0),
                            Color32::WHITE,
                        );
                    }
                    let border = if self.current_preset == Some(i) {
                        egui::Stroke::new(2.5, theme::ACCENT)
                    } else if resp.hovered() {
                        egui::Stroke::new(1.5, Color32::from_gray(160))
                    } else {
                        egui::Stroke::new(1.0, Color32::from_gray(70))
                    };
                    painter.rect_stroke(rect, 4.0, border, egui::StrokeKind::Inside);
                    let resp = resp.on_hover_text(format!("{}. {}", i + 1, self.presets[i].name));
                    if resp.clicked() {
                        if ui.input(|inp| inp.modifiers.command) {
                            if !self.sheet_selection.remove(&i) {
                                self.sheet_selection.insert(i);
                            }
                        } else {
                            clicked = Some(i);
                        }
                    }
                    if (i + 1) % cols == 0 {
                        ui.end_row();
                    }
                }
            });
        });
        if let Some(i) = clicked {
            self.load_preset(i);
        }
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if !self.jobs.is_empty() {
                ui.spinner();
            }
            match &self.status {
                Some((true, m)) => ui.label(RichText::new(m).color(theme::OK_GREEN)),
                Some((false, m)) => ui.label(RichText::new(m).color(theme::ERR_RED)),
                None => ui.label(RichText::new("Ready").weak()),
            };
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(
                        "Drag handles to pose · drag empty space to orbit · right/middle-drag to pan · wheel to zoom",
                    )
                    .small()
                    .weak(),
                );
            });
        });
    }

    fn windows(&mut self, ctx: &egui::Context) {
        let mut open = self.show_help;
        egui::Window::new("Controls").open(&mut open).resizable(false).collapsible(false).show(ctx, |ui| {
            egui::Grid::new("help").num_columns(2).spacing([16.0, 4.0]).show(ui, |ui| {
                for (k, v) in [
                    ("Click a body part / handle", "select the joint"),
                    ("Drag a handle", "rotate that bone towards the pointer (FK)"),
                    ("Drag a wrist / ankle handle", "two-bone IK for the arm / leg (when IK is on)"),
                    ("Drag the pelvis handle", "move the whole figure"),
                    ("Drag empty space", "orbit the camera"),
                    ("Right / middle drag", "pan"),
                    ("Mouse wheel", "zoom"),
                    ("Ctrl+Z / Ctrl+Y", "undo / redo"),
                    ("M / R / F", "mirror pose / reset joint / frame figure"),
                    ("Ctrl+S / Ctrl+O / Ctrl+E", "save / open pose JSON / export PNG"),
                ] {
                    ui.label(RichText::new(k).strong());
                    ui.label(v);
                    ui.end_row();
                }
            });
        });
        self.show_help = open;
        let mut open = self.show_about;
        egui::Window::new("About Rust Pose Studio").open(&mut open).resizable(false).collapsible(false).show(
            ctx,
            |ui| {
                ui.label(RichText::new(format!("Rust Pose Studio {}", env!("CARGO_PKG_VERSION"))).heading());
                ui.label("A posable 3D drawing mannequin for figure-drawing reference.");
                ui.label("Built-in poses are original data inspired by common figure-drawing reference pose types.");
                ui.hyperlink("https://github.com/stevenke1981/rust-pose-studio");
                ui.label("© 2026 Ke Sheng Da — MIT License");
            },
        );
        self.show_about = open;
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        use egui::{Key, KeyboardShortcut, Modifiers};
        let typing = ctx.egui_wants_keyboard_input();
        // Ctrl+Shift+Z must be consumed before Ctrl+Z.
        let (redo_a, redo_b, save, open, export) = ctx.input_mut(|i| {
            (
                i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z)),
                i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::Y)),
                i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::S)),
                i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::O)),
                i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::E)),
            )
        });
        let plain_undo = ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::Z)));
        if redo_a || redo_b {
            self.redo();
        } else if plain_undo {
            self.undo();
        }
        if save {
            self.save_pose_dialog();
        }
        if open {
            self.open_pose_dialog();
        }
        if export {
            self.export_png_dialog(ctx);
        }
        if !typing {
            let (m, r, f) = ctx.input(|i| (i.key_pressed(Key::M), i.key_pressed(Key::R), i.key_pressed(Key::F)));
            if m {
                self.mirror_pose();
            }
            if r && let Some(j) = self.selected {
                self.pose.set(j, [0.0; 3]);
            }
            if f {
                self.frame_camera();
            }
        }
    }
}

const VIEWS: [(&str, f32, f32); 7] = [
    ("Front", 0.0, 5.0),
    ("Three-quarter", 35.0, 12.0),
    ("Left side", 90.0, 5.0),
    ("Back", 180.0, 5.0),
    ("Right side", -90.0, 5.0),
    ("High angle", 30.0, 45.0),
    ("Top", 0.0, 85.0),
];

fn sanitize(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c.to_ascii_lowercase() } else { '-' })
        .collect();
    let s = s.split('-').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("-");
    if s.is_empty() { "pose".into() } else { s }
}

/// Everything the viewport drawing depends on.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ViewKey {
    pose: rust_pose_studio::skeleton::Pose,
    props: Proportions,
    camera: Camera,
    rect: [f32; 4],
    ppp: f32,
    floor: FloorStyle,
    highlight: Option<rust_pose_studio::skeleton::Joint>,
}

/// The last viewport drawing and its fill texture.
pub(crate) struct ViewCache {
    key: ViewKey,
    list: DrawList,
    tex: Option<egui::TextureHandle>,
}

fn make_thumbs(skel: &Skeleton, presets: &[Preset]) -> Vec<DrawList> {
    presets
        .iter()
        .map(|p| {
            let body = Body::new(skel, &p.pose);
            let cam = framing_camera(&body, p.yaw, p.pitch, true);
            let style = Style { line_width: 0.0042, fill: false, ..Style::default() };
            draw_body(&body, &View::new(&cam), &style, Framing::Fit { margin: 0.04 }, [0.0, 0.0, 180.0, 180.0], 1.0).0
        })
        .collect()
}

impl eframe::App for PoseApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll_jobs();
        self.shortcuts(&ctx);

        egui::Panel::top("menu_bar").show(ui, |ui| self.menu_bar(ui));
        egui::Panel::bottom("status_bar").show(ui, |ui| self.status_bar(ui));
        egui::Panel::left("gallery")
            .resizable(true)
            .default_size(268.0)
            .min_size(140.0)
            .show(ui, |ui| self.gallery(ui));
        egui::Panel::right("inspector").resizable(true).default_size(300.0).min_size(240.0).show(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| self.inspector(ui));
        });
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(Color32::from_rgb(24, 25, 28)).inner_margin(6))
            .show(ui, |ui| {
                self.viewport(ui);
            });
        self.windows(&ctx);

        if self.snap && self.drag.is_none() {
            snap_to_floor(&self.skel, &mut self.pose);
        }
        self.commit_history(&ctx);
        if !self.jobs.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }
}
