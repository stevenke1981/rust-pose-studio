//! PNG export: single views and contact sheets, rendered offscreen with tiny-skia
//! from the same drawing list used on screen.

use std::path::Path;

use tiny_skia::Pixmap;

use crate::body::Body;
use crate::raster;
use crate::render::{Camera, FloorStyle, Framing, Style, View, draw_body, framing_camera};
use crate::skeleton::{Pose, Skeleton};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImageOptions {
    pub width: u32,
    pub height: u32,
    pub transparent: bool,
    /// Frame the figure tightly instead of using the camera's framing.
    pub fit: bool,
    pub floor: FloorStyle,
}

impl Default for ImageOptions {
    fn default() -> Self {
        ImageOptions { width: 1024, height: 1024, transparent: false, fit: true, floor: FloorStyle::None }
    }
}

/// Render one posed figure as seen by `camera`.
pub fn render_pose(skel: &Skeleton, pose: &Pose, camera: &Camera, opts: &ImageOptions) -> Result<Pixmap, String> {
    let (w, h) = (opts.width.clamp(16, 8192), opts.height.clamp(16, 8192));
    let mut pm = Pixmap::new(w, h).ok_or("cannot allocate image")?;
    raster::clear(&mut pm, opts.transparent);
    let body = Body::new(skel, pose);
    let view = View::new(camera);
    let style = Style {
        floor: opts.floor,
        line_width: if opts.fit { 0.0050 } else { 0.0036 },
        transparent: opts.transparent,
        ..Style::default()
    };
    let framing = if opts.fit { Framing::Fit { margin: 0.07 } } else { Framing::Camera };
    let (list, frame) = draw_body(&body, &view, &style, framing, [0.0, 0.0, w as f32, h as f32], 1.0);
    raster::draw_list(&mut pm, &list, &frame, h as f32);
    Ok(pm)
}

#[derive(Clone, Debug, PartialEq)]
pub struct SheetItem {
    pub number: usize,
    pub name: String,
    pub pose: Pose,
    pub yaw: f32,
    pub pitch: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SheetOptions {
    pub columns: u32,
    /// Cell size in pixels (square cells).
    pub cell: u32,
    pub transparent: bool,
    /// Numbered badge in the top-left corner of every cell.
    pub numbers: bool,
    /// Pose name under every figure.
    pub names: bool,
    pub floor: FloorStyle,
    pub title: Option<String>,
}

impl Default for SheetOptions {
    fn default() -> Self {
        SheetOptions {
            columns: 5,
            cell: 360,
            transparent: false,
            numbers: true,
            names: false,
            floor: FloorStyle::None,
            title: None,
        }
    }
}

/// Badge colour of the cell numbers.
pub const BADGE: crate::render::Rgba = [226, 96, 122, 255];

/// Render a numbered grid of poses (like a printed pose reference sheet).
pub fn render_contact_sheet(skel: &Skeleton, items: &[SheetItem], opts: &SheetOptions) -> Result<Pixmap, String> {
    if items.is_empty() {
        return Err("no poses to export".into());
    }
    let cols = opts.columns.clamp(1, 12).min(items.len() as u32);
    let rows = (items.len() as u32).div_ceil(cols);
    let cell = opts.cell.clamp(64, 2048) as f32;
    let title_h = if opts.title.is_some() { (cell * 0.16).round() } else { 0.0 };
    let w = (cols as f32 * cell).round() as u32 + 1;
    let h = (rows as f32 * cell + title_h).round() as u32 + 1;
    if (w as u64) * (h as u64) > 400_000_000 {
        return Err("contact sheet would be too large".into());
    }
    let mut pm = Pixmap::new(w, h).ok_or("cannot allocate image")?;
    raster::clear(&mut pm, opts.transparent);
    let ink = crate::render::INK;
    let grey = [150, 150, 155, 255];
    if let Some(t) = &opts.title {
        raster::text(&mut pm, t, cell * 0.06, title_h * 0.68, title_h * 0.5, ink, true);
    }
    for (i, item) in items.iter().enumerate() {
        let (cx, cy) = ((i as u32 % cols) as f32 * cell, title_h + (i as u32 / cols) as f32 * cell);
        let body = Body::new(skel, &item.pose);
        let cam = framing_camera(&body, item.yaw, item.pitch, true);
        let view = View::new(&cam);
        let style = Style { floor: opts.floor, line_width: 0.0100, transparent: opts.transparent, ..Style::default() };
        let label_h = if opts.names { cell * 0.1 } else { 0.0 };
        let top = if opts.numbers { cell * 0.03 } else { 0.0 };
        let rect = [cx, cy + top, cell, cell - top - label_h];
        let (list, frame) = draw_body(&body, &view, &style, Framing::Fit { margin: 0.045 }, rect, 1.0);
        raster::draw_list(&mut pm, &list, &frame, cell);
        if opts.numbers {
            let r = cell * 0.074;
            let (bx, by) = (cx + cell * 0.03 + r, cy + cell * 0.03 + r);
            raster::fill_circle(&mut pm, bx, by, r, BADGE);
            let label = item.number.to_string();
            let size = r * 1.25;
            let tw = raster::text_width(&label, size);
            raster::text(
                &mut pm,
                &label,
                bx - tw * 0.5 - size * 0.02,
                by + size * 0.36,
                size,
                [255, 255, 255, 255],
                true,
            );
        }
        if opts.names {
            let size = cell * 0.052;
            let tw = raster::text_width(&item.name, size);
            raster::text(&mut pm, &item.name, cx + (cell - tw) * 0.5, cy + cell - cell * 0.035, size, grey, false);
        }
    }
    // Thin grey cell borders.
    let line_c = [196, 196, 200, 255];
    for c in 0..=cols {
        let x = c as f32 * cell + 0.5;
        raster::line(&mut pm, &[[x, title_h], [x, h as f32]], 1.0, line_c);
    }
    for r in 0..=rows {
        let y = title_h + r as f32 * cell + 0.5;
        raster::line(&mut pm, &[[0.0, y], [w as f32, y]], 1.0, line_c);
    }
    Ok(pm)
}

pub fn save_png(pm: &Pixmap, path: &Path) -> Result<(), String> {
    let bytes = raster::encode_png(pm)?;
    std::fs::write(path, bytes).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

/// Contact-sheet items for the built-in presets (all, or the given 0-based indices).
pub fn preset_items(skel: &Skeleton, only: Option<&[usize]>) -> Vec<SheetItem> {
    crate::presets::presets(skel)
        .into_iter()
        .enumerate()
        .filter(|(i, _)| only.is_none_or(|o| o.contains(i)))
        .map(|(i, p)| SheetItem { number: i + 1, name: p.name.to_string(), pose: p.pose, yaw: p.yaw, pitch: p.pitch })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skeleton::Proportions;

    fn non_white(pm: &Pixmap) -> usize {
        pm.pixels().iter().filter(|p| p.red() < 128).count()
    }

    #[test]
    fn single_export_produces_png() {
        let skel = Skeleton::new(Proportions::default());
        let pose = crate::presets::presets(&skel)[0].pose.clone();
        for transparent in [false, true] {
            let opts = ImageOptions { width: 256, height: 256, transparent, ..ImageOptions::default() };
            let pm = render_pose(&skel, &pose, &Camera::default(), &opts).unwrap();
            assert!(non_white(&pm) > 200, "outlines were drawn");
            let png = raster::encode_png(&pm).unwrap();
            assert!(png.len() > 1000);
            assert_eq!(&png[1..4], b"PNG");
            let img = image::load_from_memory(&png).unwrap().to_rgba8();
            assert_eq!(img.dimensions(), (256, 256));
            let corner = img.get_pixel(0, 0).0;
            assert_eq!(corner[3], if transparent { 0 } else { 255 });
        }
    }

    #[test]
    fn contact_sheet_layout_and_file() {
        let skel = Skeleton::new(Proportions::default());
        let items = preset_items(&skel, Some(&[0, 4, 9]));
        assert_eq!(items.iter().map(|i| i.number).collect::<Vec<_>>(), vec![1, 5, 10]);
        let opts = SheetOptions { columns: 2, cell: 160, title: Some("Test".into()), ..SheetOptions::default() };
        let pm = render_contact_sheet(&skel, &items, &opts).unwrap();
        assert_eq!(pm.width(), 321);
        assert!(pm.height() > 321);
        assert!(non_white(&pm) > 500);
        let path = std::env::temp_dir().join(format!("rps-sheet-{}.png", std::process::id()));
        save_png(&pm, &path).unwrap();
        let len = std::fs::metadata(&path).unwrap().len();
        std::fs::remove_file(&path).ok();
        assert!(len > 2000);
        assert!(render_contact_sheet(&skel, &[], &opts).is_err());
    }
}
