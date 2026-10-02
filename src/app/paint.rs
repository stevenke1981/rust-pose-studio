//! Paint a backend-independent [`DrawList`] with egui.

use eframe::egui::{self, Color32, Pos2, Shape, Stroke};
use rust_pose_studio::render::{DrawList, Frame2, Prim, Rgba};

fn color(c: Rgba) -> Color32 {
    Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3])
}

/// Upload (or refresh) the body fill mask of `list` as a texture.
pub fn mask_texture(ctx: &egui::Context, list: &DrawList, tex: &mut Option<egui::TextureHandle>) {
    let Some(mask) = &list.mask else {
        *tex = None;
        return;
    };
    let img = egui::ColorImage::from_rgba_unmultiplied([mask.w, mask.h], &mask.rgba());
    match tex {
        Some(t) => t.set(img, egui::TextureOptions::LINEAR),
        None => *tex = Some(ctx.load_texture("body-mask", img, egui::TextureOptions::LINEAR)),
    }
}

fn prims(painter: &egui::Painter, prims: &[Prim], frame: &Frame2, frame_h: f32) {
    let mut shapes = Vec::with_capacity(prims.len());
    for prim in prims {
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

/// Draw the list into `painter` with `frame` (view units -> screen points).
/// `frame_h` is the height the relative line widths refer to; `mask` is the
/// texture made by [`mask_texture`] for this list (if it has a fill mask).
pub fn draw(
    painter: &egui::Painter,
    list: &DrawList,
    frame: &Frame2,
    frame_h: f32,
    mask: Option<&egui::TextureHandle>,
) {
    prims(painter, &list.under, frame, frame_h);
    if let (Some(m), Some(tex)) = (&list.mask, mask) {
        let a = frame.to_px([m.min[0], m.max[1]]);
        let b = frame.to_px([m.max[0], m.min[1]]);
        let rect = egui::Rect::from_min_max(Pos2::new(a[0], a[1]), Pos2::new(b[0], b[1]));
        painter.image(tex.id(), rect, egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
    }
    prims(painter, &list.prims, frame, frame_h);
}
