//! Paint a backend-independent [`DrawList`] with egui.

use eframe::egui::{self, Color32, Pos2, Shape, Stroke};
use rust_pose_studio::render::{DrawList, Frame2, Rgba};

fn color(c: Rgba) -> Color32 {
    Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3])
}

/// Draw the list into `painter` with `frame` (view units -> screen points).
/// `frame_h` is the height the relative line widths refer to.
pub fn draw(painter: &egui::Painter, list: &DrawList, frame: &Frame2, frame_h: f32) {
    let mut shapes = Vec::with_capacity(list.prims.len());
    for prim in &list.prims {
        if prim.pts.len() < 2 {
            continue;
        }
        let pts: Vec<Pos2> = prim
            .pts
            .iter()
            .map(|p| {
                let q = frame.to_px(*p);
                Pos2::new(q[0], q[1])
            })
            .collect();
        let stroke =
            prim.stroke.map(|s| Stroke::new(frame.stroke_px(s.width, frame_h), color(s.color))).unwrap_or(Stroke::NONE);
        if prim.closed {
            let fill = prim.fill.map(color).unwrap_or(Color32::TRANSPARENT);
            shapes.push(Shape::convex_polygon(pts, fill, stroke));
        } else {
            shapes.push(Shape::line(pts, stroke));
        }
    }
    painter.extend(shapes);
}
