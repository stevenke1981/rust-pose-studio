//! Right-hand inspector panel.

use eframe::egui::{self, RichText};
use rust_pose_studio::render::FloorStyle;
use rust_pose_studio::skeleton::{BodyType, Joint, Proportions};

use super::{PoseApp, VIEWS};

/// Sliders round their value to the displayed decimals even without user input. Only
/// accept changes larger than that rounding so an untouched slider never edits the pose
/// (which would otherwise create spurious undo steps).
fn accept(new: f32, old: &mut f32, half_step: f32) -> bool {
    if (new - *old).abs() > half_step * 1.001 {
        *old = new;
        true
    } else {
        false
    }
}

fn angle_slider(ui: &mut egui::Ui, v: &mut f32, label: &str, range: std::ops::RangeInclusive<f32>) -> bool {
    let mut t = *v;
    ui.add(egui::Slider::new(&mut t, range).text(label).suffix("°").fixed_decimals(0).drag_value_speed(0.5));
    accept(t, v, 0.5)
}

impl PoseApp {
    pub(super) fn inspector(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        ui.add_space(4.0);
        egui::CollapsingHeader::new(RichText::new("Selected joint").strong())
            .default_open(true)
            .show(ui, |ui| self.joint_section(ui));
        egui::CollapsingHeader::new(RichText::new("Pose").strong()).default_open(true).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Name");
                ui.text_edit_singleline(&mut self.pose_name);
            });
            ui.horizontal_wrapped(|ui| {
                if ui.add_enabled(!self.undo.is_empty(), egui::Button::new("⟲ Undo")).clicked() {
                    self.undo();
                }
                if ui.add_enabled(!self.redo.is_empty(), egui::Button::new("⟳ Redo")).clicked() {
                    self.redo();
                }
                if ui.button("⇄ Mirror").on_hover_text("Mirror the pose left ↔ right (M)").clicked() {
                    self.mirror_pose();
                }
                if ui.button("🎲 Random").clicked() {
                    self.random();
                }
                if ui.button("Standing").on_hover_text("Reset to the standing rest pose").clicked() {
                    self.reset_pose();
                }
            });
            ui.horizontal_wrapped(|ui| {
                if ui.button("💾 Save JSON…").clicked() {
                    self.save_pose_dialog();
                }
                if ui.button("📂 Open JSON…").clicked() {
                    self.open_pose_dialog();
                }
            });
            ui.checkbox(&mut self.ik, "IK: drag wrists / ankles to place hands & feet");
            ui.checkbox(&mut self.snap, "Snap figure to the floor");
        });
        egui::CollapsingHeader::new(RichText::new("Body proportions").strong()).default_open(true).show(ui, |ui| {
            let mut p = self.props;
            let mut t = p;
            ui.add(egui::Slider::new(&mut t.height, 1.2..=2.1).text("Height").suffix(" m").fixed_decimals(2));
            ui.add(egui::Slider::new(&mut t.head_size, 0.7..=1.4).text("Head size").fixed_decimals(2));
            ui.add(egui::Slider::new(&mut t.shoulder_width, 0.7..=1.4).text("Shoulder width").fixed_decimals(2));
            ui.add(egui::Slider::new(&mut t.hip_width, 0.7..=1.4).text("Hip width").fixed_decimals(2));
            accept(t.height, &mut p.height, 0.005);
            accept(t.head_size, &mut p.head_size, 0.005);
            accept(t.shoulder_width, &mut p.shoulder_width, 0.005);
            accept(t.hip_width, &mut p.hip_width, 0.005);
            ui.horizontal(|ui| {
                ui.label("Body type");
                for t in BodyType::ALL {
                    ui.selectable_value(&mut p.body_type, t, t.label());
                }
            });
            if ui.small_button("Reset proportions").clicked() {
                p = Proportions::default();
            }
            if p != self.props {
                self.set_props(p);
            }
        });
        egui::CollapsingHeader::new(RichText::new("Camera").strong()).default_open(true).show(ui, |ui| {
            let c = &mut self.camera;
            let mut t = *c;
            ui.add(egui::Slider::new(&mut t.yaw, -180.0..=180.0).text("Orbit").suffix("°").fixed_decimals(0));
            ui.add(egui::Slider::new(&mut t.pitch, -85.0..=89.0).text("Elevation").suffix("°").fixed_decimals(0));
            ui.add(
                egui::Slider::new(&mut t.distance, 0.8..=40.0)
                    .logarithmic(true)
                    .text("Distance")
                    .suffix(" m")
                    .fixed_decimals(1),
            );
            ui.add(egui::Slider::new(&mut t.fov, 10.0..=70.0).text("Field of view").suffix("°").fixed_decimals(0));
            accept(t.yaw, &mut c.yaw, 0.5);
            accept(t.pitch, &mut c.pitch, 0.5);
            accept(t.distance, &mut c.distance, 0.05);
            accept(t.fov, &mut c.fov, 0.5);
            ui.checkbox(&mut c.perspective, "Perspective (off = orthographic)");
            ui.horizontal_wrapped(|ui| {
                for (name, yaw, pitch) in VIEWS {
                    if ui.small_button(name).clicked() {
                        self.camera.yaw = yaw;
                        self.camera.pitch = pitch;
                        self.frame_camera();
                    }
                }
                if ui.small_button("⛶ Frame").clicked() {
                    self.frame_camera();
                }
            });
        });
        egui::CollapsingHeader::new(RichText::new("Display").strong()).default_open(false).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Floor");
                ui.selectable_value(&mut self.floor, FloorStyle::Grid, "Grid");
                ui.selectable_value(&mut self.floor, FloorStyle::Shadow, "Shadow");
                ui.selectable_value(&mut self.floor, FloorStyle::None, "None");
            });
            ui.checkbox(&mut self.show_handles, "Joint handles");
        });
        egui::CollapsingHeader::new(RichText::new("Export").strong()).default_open(true).show(ui, |ui| {
            let e = &mut self.export;
            ui.horizontal(|ui| {
                ui.label("Size");
                egui::ComboBox::from_id_salt("export_size").selected_text(format!("{} px", e.size)).show_ui(ui, |ui| {
                    for s in [512, 1024, 2048, 4096] {
                        ui.selectable_value(&mut e.size, s, format!("{s} px"));
                    }
                });
            });
            ui.horizontal(|ui| {
                ui.label("Background");
                ui.selectable_value(&mut e.transparent, false, "White");
                ui.selectable_value(&mut e.transparent, true, "Transparent");
            });
            ui.horizontal(|ui| {
                ui.label("Floor");
                ui.selectable_value(&mut e.floor, FloorStyle::Shadow, "Shadow");
                ui.selectable_value(&mut e.floor, FloorStyle::Grid, "Grid");
                ui.selectable_value(&mut e.floor, FloorStyle::None, "None");
            });
            ui.checkbox(&mut e.fit, "Fit the figure to the image");
            ui.checkbox(&mut e.match_view, "Match the viewport aspect ratio");
            if ui.button("🖼 Export current view as PNG…").clicked() {
                self.export_png_dialog(&ctx);
            }
            ui.separator();
            let e = &mut self.export;
            ui.label(RichText::new("Contact sheet").strong());
            ui.add(egui::Slider::new(&mut e.sheet_cols, 1..=10).text("Columns"));
            ui.add(egui::Slider::new(&mut e.sheet_cell, 160..=1024).text("Cell size").suffix(" px"));
            ui.horizontal(|ui| {
                ui.checkbox(&mut e.sheet_numbers, "Numbers");
                ui.checkbox(&mut e.sheet_names, "Names");
            });
            ui.checkbox(&mut e.sheet_current_angle, "Use the current camera angle for all");
            ui.checkbox(&mut e.sheet_add_current, "Append the current pose");
            ui.horizontal(|ui| {
                ui.label("Title");
                ui.text_edit_singleline(&mut e.sheet_title);
            });
            let n = if self.sheet_selection.is_empty() { self.presets.len() } else { self.sheet_selection.len() };
            if ui.button(format!("▦ Export contact sheet ({n} presets)…")).clicked() {
                self.export_sheet_dialog(&ctx);
            }
        });
    }

    fn joint_section(&mut self, ui: &mut egui::Ui) {
        egui::ComboBox::from_id_salt("joint_select")
            .width(ui.available_width() - 8.0)
            .selected_text(self.selected.map(|j| j.label()).unwrap_or("(none)"))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.selected, None, "(none)");
                for j in Joint::ALL {
                    ui.selectable_value(&mut self.selected, Some(j), j.label());
                }
            });
        let Some(j) = self.selected else {
            ui.label(RichText::new("Select a joint in the viewport or the list to edit its rotation.").weak());
            return;
        };
        let mut e = self.pose.get(j);
        let mut changed = false;
        if j.is_hinge() {
            let range = if matches!(j, Joint::ForearmL | Joint::ForearmR) { -165.0..=5.0 } else { -5.0..=165.0 };
            changed |= angle_slider(ui, &mut e[0], "Bend (X)", range);
            ui.label(RichText::new("Elbows and knees are hinge joints.").small().weak());
        } else {
            changed |= angle_slider(ui, &mut e[0], "X (pitch)", -180.0..=180.0);
            changed |= angle_slider(ui, &mut e[1], "Y (twist)", -180.0..=180.0);
            changed |= angle_slider(ui, &mut e[2], "Z (roll)", -180.0..=180.0);
        }
        if changed {
            self.pose.set(j, e);
            self.current_preset = None;
        }
        if j == Joint::Pelvis {
            let mut t = self.pose.root;
            ui.add(egui::Slider::new(&mut t.x, -2.0..=2.0).text("Position X").suffix(" m").fixed_decimals(2));
            ui.add_enabled(
                !self.snap,
                egui::Slider::new(&mut t.y, 0.0..=2.5).text("Position Y").suffix(" m").fixed_decimals(2),
            );
            ui.add(egui::Slider::new(&mut t.z, -2.0..=2.0).text("Position Z").suffix(" m").fixed_decimals(2));
            let r = &mut self.pose.root;
            accept(t.x, &mut r.x, 0.005);
            accept(t.y, &mut r.y, 0.005);
            accept(t.z, &mut r.z, 0.005);
        }
        ui.horizontal_wrapped(|ui| {
            if ui.button("Reset joint").clicked() {
                self.pose.set(j, [0.0; 3]);
            }
            if j.mirror() != j
                && ui
                    .button(format!("Copy to {}", side_name(j.mirror())))
                    .on_hover_text("Mirror this joint's rotation onto the other side")
                    .clicked()
            {
                let [x, y, z] = self.pose.get(j);
                self.pose.set(j.mirror(), [x, -y, -z]);
            }
        });
    }
}

fn side_name(j: Joint) -> &'static str {
    match j.side() {
        rust_pose_studio::skeleton::Side::Left => "left side",
        rust_pose_studio::skeleton::Side::Right => "right side",
        rust_pose_studio::skeleton::Side::Center => "centre",
    }
}
