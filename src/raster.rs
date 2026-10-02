//! Offscreen software rasteriser (tiny-skia) for the drawing list, plus simple
//! text rendering for contact-sheet labels (ab_glyph + egui's bundled font).

use ab_glyph::{Font, FontRef, PxScale, ScaleFont, point};
use tiny_skia::{Color, FillRule, LineCap, LineJoin, Paint, PathBuilder, Pixmap, Stroke, Transform};

use crate::render::{DrawList, Frame2, Rgba};

fn paint(c: Rgba) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color_rgba8(c[0], c[1], c[2], c[3]);
    p.anti_alias = true;
    p
}

/// Rasterise a drawing list into `pixmap` using `frame` (view units -> pixels).
/// `frame_h` is the height of the frame the relative line widths refer to.
pub fn draw_list(pixmap: &mut Pixmap, list: &DrawList, frame: &Frame2, frame_h: f32) {
    for prim in &list.prims {
        if prim.pts.len() < 2 {
            continue;
        }
        let mut pb = PathBuilder::new();
        let p0 = frame.to_px(prim.pts[0]);
        pb.move_to(p0[0], p0[1]);
        for q in &prim.pts[1..] {
            let p = frame.to_px(*q);
            pb.line_to(p[0], p[1]);
        }
        if prim.closed {
            pb.close();
        }
        let Some(path) = pb.finish() else { continue };
        if prim.closed
            && let Some(f) = prim.fill
        {
            pixmap.fill_path(&path, &paint(f), FillRule::Winding, Transform::identity(), None);
        }
        if let Some(s) = prim.stroke {
            let stroke = Stroke {
                width: frame.stroke_px(s.width, frame_h),
                line_cap: LineCap::Round,
                line_join: LineJoin::Round,
                ..Stroke::default()
            };
            pixmap.stroke_path(&path, &paint(s.color), &stroke, Transform::identity(), None);
        }
    }
}

/// Fill an axis-aligned rectangle.
pub fn fill_rect(pixmap: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, c: Rgba) {
    if let Some(r) = tiny_skia::Rect::from_xywh(x, y, w, h) {
        pixmap.fill_rect(r, &paint(c), Transform::identity(), None);
    }
}

/// Stroke a polyline (used for cell borders).
pub fn line(pixmap: &mut Pixmap, pts: &[[f32; 2]], width: f32, c: Rgba) {
    let mut pb = PathBuilder::new();
    for (i, p) in pts.iter().enumerate() {
        if i == 0 { pb.move_to(p[0], p[1]) } else { pb.line_to(p[0], p[1]) }
    }
    if let Some(path) = pb.finish() {
        let stroke = Stroke { width, ..Stroke::default() };
        pixmap.stroke_path(&path, &paint(c), &stroke, Transform::identity(), None);
    }
}

pub fn clear(pixmap: &mut Pixmap, transparent: bool) {
    pixmap.fill(if transparent { Color::TRANSPARENT } else { Color::WHITE });
}

fn font() -> FontRef<'static> {
    FontRef::try_from_slice(epaint_default_fonts::UBUNTU_LIGHT).expect("bundled font is valid")
}

/// Width in pixels of `text` at `size` px.
pub fn text_width(text: &str, size: f32) -> f32 {
    let f = font();
    let sf = f.as_scaled(PxScale::from(size));
    let mut w = 0.0;
    let mut prev = None;
    for c in text.chars() {
        let id = sf.glyph_id(c);
        if let Some(p) = prev {
            w += sf.kern(p, id);
        }
        w += sf.h_advance(id);
        prev = Some(id);
    }
    w
}

/// Draw `text` with its baseline-left at (x, y). `bold` draws it twice, offset.
pub fn text(pixmap: &mut Pixmap, text: &str, x: f32, y: f32, size: f32, c: Rgba, bold: bool) {
    let passes: &[f32] = if bold { &[0.0, size * 0.045] } else { &[0.0] };
    for &dx in passes {
        draw_text_pass(pixmap, text, x + dx, y, size, c);
    }
}

fn draw_text_pass(pixmap: &mut Pixmap, text: &str, x: f32, y: f32, size: f32, c: Rgba) {
    let f = font();
    let sf = f.as_scaled(PxScale::from(size));
    let (w, h) = (pixmap.width() as i32, pixmap.height() as i32);
    let mut caret = x;
    let mut prev = None;
    let pixels = pixmap.pixels_mut();
    for ch in text.chars() {
        let id = sf.glyph_id(ch);
        if let Some(p) = prev {
            caret += sf.kern(p, id);
        }
        let glyph = id.with_scale_and_position(PxScale::from(size), point(caret, y));
        caret += sf.h_advance(id);
        prev = Some(id);
        let Some(outlined) = f.outline_glyph(glyph) else { continue };
        let bb = outlined.px_bounds();
        outlined.draw(|gx, gy, cov| {
            let px = bb.min.x as i32 + gx as i32;
            let py = bb.min.y as i32 + gy as i32;
            if px < 0 || py < 0 || px >= w || py >= h {
                return;
            }
            let a = (cov.clamp(0.0, 1.0) * c[3] as f32 / 255.0).min(1.0);
            let i = (py * w + px) as usize;
            let d = pixels[i];
            let blend = |s: u8, dst: u8| (s as f32 * a + dst as f32 * (1.0 - a)).round().clamp(0.0, 255.0) as u8;
            let na = blend(255, d.alpha());
            let nr = blend(c[0], d.red()).min(na);
            let ng = blend(c[1], d.green()).min(na);
            let nb = blend(c[2], d.blue()).min(na);
            if let Some(p) = tiny_skia::PremultipliedColorU8::from_rgba(nr, ng, nb, na) {
                pixels[i] = p;
            }
        });
    }
}

/// Encode a pixmap as PNG (demultiplying alpha).
pub fn encode_png(pixmap: &Pixmap) -> Result<Vec<u8>, String> {
    let mut rgba = Vec::with_capacity(pixmap.data().len());
    for p in pixmap.pixels() {
        let c = p.demultiply();
        rgba.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
    }
    let img = image::RgbaImage::from_raw(pixmap.width(), pixmap.height(), rgba).ok_or("bad image buffer size")?;
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).map_err(|e| format!("PNG encoding failed: {e}"))?;
    Ok(out.into_inner())
}
