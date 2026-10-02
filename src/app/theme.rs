//! Dark theme with an orange accent (matches the joint highlight colour).

use eframe::egui::{self, Color32, Stroke, Visuals};

pub const ACCENT: Color32 = Color32::from_rgb(240, 128, 20);
pub const ACCENT_DIM: Color32 = Color32::from_rgb(150, 85, 25);
pub const HANDLE: Color32 = Color32::from_rgb(40, 120, 230);
pub const IK_HANDLE: Color32 = Color32::from_rgb(30, 170, 90);
pub const OK_GREEN: Color32 = Color32::from_rgb(90, 200, 120);
pub const ERR_RED: Color32 = Color32::from_rgb(240, 90, 90);

pub fn apply(ctx: &egui::Context) {
    let mut v = Visuals::dark();
    v.panel_fill = Color32::from_rgb(30, 32, 36);
    v.window_fill = Color32::from_rgb(38, 41, 46);
    v.extreme_bg_color = Color32::from_rgb(18, 19, 22);
    v.faint_bg_color = Color32::from_rgb(36, 38, 43);
    v.selection.bg_fill = ACCENT_DIM;
    v.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    v.hyperlink_color = Color32::from_rgb(255, 170, 90);
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT);
    v.widgets.active.bg_fill = ACCENT_DIM;
    v.widgets.active.weak_bg_fill = ACCENT_DIM;
    v.slider_trailing_fill = true;
    ctx.set_visuals_of(egui::Theme::Dark, v);
    ctx.set_theme(egui::Theme::Dark);
}
