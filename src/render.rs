//! Camera, projection and the backend-independent mannequin drawing list.
//!
//! The scene is turned into a list of flat 2D primitives (convex filled polygons
//! with outlines and open polylines) sorted back-to-front. The same list is drawn
//! on screen by egui and rasterised offscreen by tiny-skia for PNG export, so the
//! exported images match the viewport exactly.

use serde::{Deserialize, Serialize};

use crate::body::{Body, Part, Volume};
use crate::math::{Mat3, Vec3, v3};
use crate::skeleton::Joint;

pub type Rgba = [u8; 4];

pub const INK: Rgba = [20, 20, 22, 255];
pub const PAPER: Rgba = [255, 255, 255, 255];
pub const ACCENT: Rgba = [240, 128, 20, 255];

/// Orbit camera around a target point.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Camera {
    /// Rotation around the vertical axis in degrees (0 = looking at the figure's front).
    pub yaw: f32,
    /// Elevation in degrees (positive = looking down from above).
    pub pitch: f32,
    pub distance: f32,
    pub target: [f32; 3],
    /// Vertical field of view in degrees (perspective) / framing reference (orthographic).
    pub fov: f32,
    pub perspective: bool,
}

impl Default for Camera {
    fn default() -> Self {
        Camera { yaw: 30.0, pitch: 12.0, distance: 4.2, target: [0.0, 0.85, 0.0], fov: 30.0, perspective: true }
    }
}

/// A camera resolved into a view basis.
#[derive(Clone, Copy, Debug)]
pub struct View {
    pub eye: Vec3,
    pub right: Vec3,
    pub up: Vec3,
    pub fwd: Vec3,
    pub focal: f32,
    pub ortho_half: f32,
    pub perspective: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct Projected {
    pub x: f32,
    pub y: f32,
    /// Distance along the view direction (larger = further away).
    pub depth: f32,
    /// World-units to view-units scale at this point.
    pub scale: f32,
}

impl View {
    pub fn new(cam: &Camera) -> View {
        let yaw = cam.yaw.to_radians();
        let pitch = cam.pitch.clamp(-89.0, 89.0).to_radians();
        let target = Vec3::from_array(cam.target);
        let dir = v3(yaw.sin() * pitch.cos(), pitch.sin(), yaw.cos() * pitch.cos());
        let eye = target + dir * cam.distance.max(0.1);
        let fwd = -dir;
        let right = fwd.cross(Vec3::Y).normalized_or(Vec3::X);
        let up = right.cross(fwd);
        let half = (cam.fov.clamp(5.0, 120.0).to_radians() * 0.5).tan();
        View {
            eye,
            right,
            up,
            fwd,
            focal: 1.0 / half,
            ortho_half: cam.distance.max(0.1) * half,
            perspective: cam.perspective,
        }
    }

    /// Project a world point into view units: the visible frame spans y in [-1, 1].
    pub fn project(&self, p: Vec3) -> Projected {
        let d = p - self.eye;
        let depth = d.dot(self.fwd);
        let scale = if self.perspective { self.focal / depth.max(0.05) } else { 1.0 / self.ortho_half };
        Projected { x: d.dot(self.right) * scale, y: d.dot(self.up) * scale, depth, scale }
    }

    /// Inverse projection at a given view depth.
    pub fn unproject(&self, x: f32, y: f32, depth: f32) -> Vec3 {
        let k = if self.perspective { depth.max(0.05) / self.focal } else { self.ortho_half };
        self.eye + self.right * (x * k) + self.up * (y * k) + self.fwd * depth
    }

    /// Unit vector from a world point towards the viewer.
    pub fn to_viewer(&self, p: Vec3) -> Vec3 {
        if self.perspective { (self.eye - p).normalized() } else { -self.fwd }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FloorStyle {
    None,
    #[default]
    Shadow,
    Grid,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Style {
    pub floor: FloorStyle,
    /// Lighter fill for nearer parts.
    pub depth_shading: bool,
    pub highlight: Option<Joint>,
    /// Outline width as a fraction of the frame height.
    pub line_width: f32,
    /// Paper is transparent (affects the shadow colour only).
    pub transparent: bool,
}

impl Default for Style {
    fn default() -> Self {
        Style {
            floor: FloorStyle::Shadow,
            depth_shading: true,
            highlight: None,
            line_width: 0.0042,
            transparent: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stroke {
    /// Width as a fraction of the frame height (view units / 2).
    pub width: f32,
    pub color: Rgba,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Prim {
    /// Points in view units (+y up).
    pub pts: Vec<[f32; 2]>,
    pub fill: Option<Rgba>,
    pub stroke: Option<Stroke>,
    pub closed: bool,
    /// Body part this primitive belongs to (for picking).
    pub joint: Option<Joint>,
}

#[derive(Clone, Debug, Default)]
pub struct DrawList {
    pub prims: Vec<Prim>,
    /// Bounds of the figure (excluding floor decoration) in view units.
    pub min: [f32; 2],
    pub max: [f32; 2],
}

impl DrawList {
    pub fn is_empty(&self) -> bool {
        self.prims.is_empty()
    }

    /// Topmost body part under a view-space point.
    pub fn pick(&self, x: f32, y: f32) -> Option<Joint> {
        self.prims.iter().rev().filter(|p| p.closed && p.fill.is_some()).find_map(|p| {
            let j = p.joint?;
            point_in_convex(&p.pts, x, y).then_some(j)
        })
    }
}

fn point_in_convex(pts: &[[f32; 2]], x: f32, y: f32) -> bool {
    if pts.len() < 3 {
        return false;
    }
    let mut sign = 0.0f32;
    for i in 0..pts.len() {
        let a = pts[i];
        let b = pts[(i + 1) % pts.len()];
        let c = (b[0] - a[0]) * (y - a[1]) - (b[1] - a[1]) * (x - a[0]);
        if c.abs() < 1e-12 {
            continue;
        }
        if sign == 0.0 {
            sign = c.signum();
        } else if c.signum() != sign {
            return false;
        }
    }
    true
}

/// Convex hull (Andrew's monotone chain), counter-clockwise.
pub fn convex_hull(mut pts: Vec<[f32; 2]>) -> Vec<[f32; 2]> {
    pts.retain(|p| p[0].is_finite() && p[1].is_finite());
    pts.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    pts.dedup_by(|a, b| (a[0] - b[0]).abs() < 1e-7 && (a[1] - b[1]).abs() < 1e-7);
    if pts.len() < 3 {
        return pts;
    }
    let cross = |o: [f32; 2], a: [f32; 2], b: [f32; 2]| (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0]);
    let mut lower: Vec<[f32; 2]> = Vec::with_capacity(pts.len());
    for &p in &pts {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], p) <= 0.0 {
            lower.pop();
        }
        lower.push(p);
    }
    let mut upper: Vec<[f32; 2]> = Vec::with_capacity(pts.len());
    for &p in pts.iter().rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], p) <= 0.0 {
            upper.pop();
        }
        upper.push(p);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

fn circle_pts(out: &mut Vec<[f32; 2]>, cx: f32, cy: f32, r: f32, n: usize) {
    for i in 0..n {
        let a = i as f32 / n as f32 * std::f32::consts::TAU;
        out.push([cx + r * a.cos(), cy + r * a.sin()]);
    }
}

/// Outline of a projected ellipsoid (weak perspective at its centre).
fn ellipse_outline(view: &View, center: Vec3, axes: &Mat3, n: usize) -> (Vec<[f32; 2]>, Projected) {
    let c = view.project(center);
    // 2x3 projection of the axes, then the 2x2 shape matrix A = P P^T.
    let px = [axes.x.dot(view.right), axes.y.dot(view.right), axes.z.dot(view.right)];
    let py = [axes.x.dot(view.up), axes.y.dot(view.up), axes.z.dot(view.up)];
    let a = px[0] * px[0] + px[1] * px[1] + px[2] * px[2];
    let b = px[0] * py[0] + px[1] * py[1] + px[2] * py[2];
    let d = py[0] * py[0] + py[1] * py[1] + py[2] * py[2];
    // Eigen decomposition of [[a, b], [b, d]].
    let tr = 0.5 * (a + d);
    let det = (0.25 * (a - d) * (a - d) + b * b).sqrt();
    let l1 = (tr + det).max(1e-12);
    let l2 = (tr - det).max(1e-12);
    let (ex, ey) = if b.abs() > 1e-9 {
        let v = [l1 - d, b];
        let n = (v[0] * v[0] + v[1] * v[1]).sqrt();
        (v[0] / n, v[1] / n)
    } else if a >= d {
        (1.0, 0.0)
    } else {
        (0.0, 1.0)
    };
    let (r1, r2) = (l1.sqrt() * c.scale, l2.sqrt() * c.scale);
    let mut pts = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / n as f32 * std::f32::consts::TAU;
        let (s, co) = t.sin_cos();
        pts.push([c.x + ex * r1 * co - ey * r2 * s, c.y + ey * r1 * co + ex * r2 * s]);
    }
    (pts, c)
}

fn capsule_outline(view: &View, a: Vec3, ra: f32, b: Vec3, rb: f32) -> (Vec<[f32; 2]>, f32) {
    let pa = view.project(a);
    let pb = view.project(b);
    let mut pts = Vec::with_capacity(56);
    circle_pts(&mut pts, pa.x, pa.y, ra * pa.scale, 28);
    circle_pts(&mut pts, pb.x, pb.y, rb * pb.scale, 28);
    (convex_hull(pts), 0.5 * (pa.depth + pb.depth))
}

/// Visible part of a curve on the head surface (local unit-sphere points `u`).
fn head_curve(view: &View, center: Vec3, axes: &Mat3, us: impl Iterator<Item = Vec3>) -> Vec<Vec<[f32; 2]>> {
    // For axes = R * S, the surface normal at local u is R * (u / s).
    let sx = axes.x.length().max(1e-6);
    let sy = axes.y.length().max(1e-6);
    let sz = axes.z.length().max(1e-6);
    let mut runs = Vec::new();
    let mut cur: Vec<[f32; 2]> = Vec::new();
    for u in us {
        let p = center + axes.mul_vec(u);
        let n = axes.x * (u.x / (sx * sx)) + axes.y * (u.y / (sy * sy)) + axes.z * (u.z / (sz * sz));
        if n.dot(view.to_viewer(p)) > 0.02 {
            let q = view.project(p);
            cur.push([q.x, q.y]);
        } else if cur.len() > 1 {
            runs.push(std::mem::take(&mut cur));
        } else {
            cur.clear();
        }
    }
    if cur.len() > 1 {
        runs.push(cur);
    }
    runs
}

fn shade(fill: u8, t: f32) -> Rgba {
    let v = (fill as f32 * (1.0 - t) + 206.0 * t).round() as u8;
    [v, v, v, 255]
}

struct Group {
    depth: f32,
    prims: Vec<Prim>,
}

/// Build the drawing list for a posed body.
pub fn draw_body(body: &Body, view: &View, style: &Style) -> DrawList {
    let mut list = DrawList::default();
    let lw = style.line_width;
    let s = (body.fk.tip(Joint::Head).y - body.min_y()).max(0.5) / 1.7;

    // Floor decoration.
    match style.floor {
        FloorStyle::Grid => {
            let pelvis = body.fk.pos(Joint::Pelvis);
            let (cx, cz) = ((pelvis.x / 0.25).round() * 0.25, (pelvis.z / 0.25).round() * 0.25);
            let n = 8;
            let ext = n as f32 * 0.25;
            for i in -n..=n {
                let o = i as f32 * 0.25;
                let col: Rgba = if i == 0 { [175, 180, 190, 255] } else { [214, 217, 222, 255] };
                for (a, b) in [
                    (v3(cx - ext, 0.0, cz + o), v3(cx + ext, 0.0, cz + o)),
                    (v3(cx + o, 0.0, cz - ext), v3(cx + o, 0.0, cz + ext)),
                ] {
                    let mut pts = Vec::new();
                    for k in 0..=16 {
                        let p = view.project(a.lerp(b, k as f32 / 16.0));
                        if p.depth > 0.1 {
                            pts.push([p.x, p.y]);
                        }
                    }
                    if pts.len() > 1 {
                        list.prims.push(Prim {
                            pts,
                            fill: None,
                            stroke: Some(Stroke { width: lw * 0.45, color: col }),
                            closed: false,
                            joint: None,
                        });
                    }
                }
            }
            push_shadows(&mut list, body, view, style, s);
        }
        FloorStyle::Shadow => push_shadows(&mut list, body, view, style, s),
        FloorStyle::None => {}
    }

    // Body parts, painter's algorithm.
    let mut groups: Vec<Group> = body.parts.iter().map(|p| part_group(p, view, style)).collect();
    // The neck emerges from inside the rib cage: always paint it before the chest.
    let find = |j: Joint| body.parts.iter().position(|p| p.joint == j);
    if let (Some(n), Some(c)) = (find(Joint::Neck), find(Joint::Chest)) {
        groups[n].depth = groups[n].depth.max(groups[c].depth + 1e-3);
    }
    let (dmin, dmax) =
        groups.iter().fold((f32::INFINITY, f32::NEG_INFINITY), |(a, b), g| (a.min(g.depth), b.max(g.depth)));
    if style.depth_shading && dmax > dmin {
        for g in &mut groups {
            let t = ((g.depth - dmin) / (dmax - dmin)).clamp(0.0, 1.0);
            if let Some(p) = g.prims.first_mut()
                && let Some(f) = p.fill
                && p.joint != style.highlight
            {
                p.fill = Some(shade(f[0], t * 0.85));
            }
        }
    }
    groups.sort_by(|a, b| b.depth.total_cmp(&a.depth));
    let mut min = [f32::INFINITY; 2];
    let mut max = [f32::NEG_INFINITY; 2];
    for g in groups {
        for p in g.prims {
            for q in &p.pts {
                min = [min[0].min(q[0]), min[1].min(q[1])];
                max = [max[0].max(q[0]), max[1].max(q[1])];
            }
            list.prims.push(p);
        }
    }
    list.min = min;
    list.max = max;
    list
}

fn push_shadows(list: &mut DrawList, body: &Body, view: &View, style: &Style, s: f32) {
    let reach = 0.22 * s;
    for part in &body.parts {
        let h = part.volume.min_y();
        if h > reach {
            continue;
        }
        let k = (1.0 - (h / reach).max(0.0)).powf(1.5);
        let (center, axes) = match part.volume {
            Volume::Capsule { a, ra, b, rb } => {
                let fa = v3(a.x, 0.0, a.z);
                let fb = v3(b.x, 0.0, b.z);
                let along = fb - fa;
                let len = along.length();
                let dir = along.normalized_or(Vec3::X);
                let side = Vec3::Y.cross(dir);
                let r = ra.max(rb) * 1.1;
                (fa.lerp(fb, 0.5), Mat3::from_cols(dir * (len * 0.5 + r), Vec3::Y * 0.001, side * r))
            }
            Volume::Ellipsoid { center, axes } | Volume::Blob { a: (center, axes), .. } => {
                let flat = |v: Vec3| v3(v.x, 0.0, v.z) * 1.1;
                (v3(center.x, 0.0, center.z), Mat3::from_cols(flat(axes.x), flat(axes.y), flat(axes.z)))
            }
        };
        // Collapse to a floor ellipse: use the two largest horizontal extents.
        let ex = crate::body::extent(&axes, Vec3::X);
        let ez = crate::body::extent(&axes, Vec3::Z);
        if ex < 1e-4 && ez < 1e-4 {
            continue;
        }
        let (pts, _) = ellipse_outline(view, center, &axes, 32);
        let alpha = (k * if style.transparent { 60.0 } else { 46.0 }) as u8;
        list.prims.push(Prim { pts, fill: Some([60, 64, 72, alpha]), stroke: None, closed: true, joint: None });
    }
}

fn part_group(part: &Part, view: &View, style: &Style) -> Group {
    let hl = style.highlight == Some(part.joint);
    let stroke =
        Some(Stroke { width: style.line_width * if hl { 1.7 } else { 1.0 }, color: if hl { ACCENT } else { INK } });
    let fill = Some(if hl { [255, 226, 190, 255] } else { PAPER });
    match part.volume {
        Volume::Capsule { a, ra, b, rb } => {
            let (pts, depth) = capsule_outline(view, a, ra, b, rb);
            Group { depth, prims: vec![Prim { pts, fill, stroke, closed: true, joint: Some(part.joint) }] }
        }
        Volume::Blob { a, b } => {
            let (mut pa, ca) = ellipse_outline(view, a.0, &a.1, 48);
            let (pb, _) = ellipse_outline(view, b.0, &b.1, 40);
            pa.extend(pb);
            Group {
                depth: ca.depth,
                prims: vec![Prim { pts: convex_hull(pa), fill, stroke, closed: true, joint: Some(part.joint) }],
            }
        }
        Volume::Ellipsoid { center, axes } => {
            let (pts, c) = ellipse_outline(view, center, &axes, 48);
            let mut prims = vec![Prim { pts, fill, stroke, closed: true, joint: Some(part.joint) }];
            // Pull the head forward in the sort so it reliably covers the neck.
            let mut depth = c.depth;
            if part.is_head {
                depth -= 0.05 * axes.y.length() / 0.112;
                let guide = Stroke { width: style.line_width * 0.7, color: if hl { ACCENT } else { INK } };
                // Vertical centre line (front half, chin to crown).
                let meridian = (0..=40).map(|i| {
                    let t = (-80.0 + 160.0 * i as f32 / 40.0).to_radians();
                    v3(0.0, t.sin(), t.cos())
                });
                // Horizontal eye line slightly below the middle.
                let lat: f32 = -0.12;
                let eye_line = (0..=40).map(move |i| {
                    let t = (-95.0 + 190.0 * i as f32 / 40.0).to_radians();
                    v3(lat.cos() * t.sin(), lat.sin(), lat.cos() * t.cos())
                });
                for run in head_curve(view, center, &axes, meridian)
                    .into_iter()
                    .chain(head_curve(view, center, &axes, eye_line))
                {
                    prims.push(Prim {
                        pts: run,
                        fill: None,
                        stroke: Some(guide),
                        closed: false,
                        joint: Some(part.joint),
                    });
                }
            }
            Group { depth, prims }
        }
    }
}

/// Maps view units to pixels: `px = ox + x * scale`, `py = oy - y * scale`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame2 {
    pub ox: f32,
    pub oy: f32,
    pub scale: f32,
}

impl Frame2 {
    /// The camera's own framing: view y in [-1, 1] fills the height.
    pub fn camera(x: f32, y: f32, w: f32, h: f32) -> Frame2 {
        Frame2 { ox: x + w * 0.5, oy: y + h * 0.5, scale: h * 0.5 }
    }

    /// Fit the figure bounds into the rectangle with a relative margin.
    pub fn fit(list: &DrawList, x: f32, y: f32, w: f32, h: f32, margin: f32) -> Frame2 {
        let bw = (list.max[0] - list.min[0]).max(1e-3);
        let bh = (list.max[1] - list.min[1]).max(1e-3);
        if !bw.is_finite() || !bh.is_finite() {
            return Frame2::camera(x, y, w, h);
        }
        let scale = ((w * (1.0 - 2.0 * margin)) / bw).min((h * (1.0 - 2.0 * margin)) / bh);
        let cx = 0.5 * (list.min[0] + list.max[0]);
        let cy = 0.5 * (list.min[1] + list.max[1]);
        Frame2 { ox: x + w * 0.5 - cx * scale, oy: y + h * 0.5 + cy * scale, scale }
    }

    pub fn to_px(&self, p: [f32; 2]) -> [f32; 2] {
        [self.ox + p[0] * self.scale, self.oy - p[1] * self.scale]
    }

    pub fn from_px(&self, px: [f32; 2]) -> [f32; 2] {
        [(px[0] - self.ox) / self.scale, (self.oy - px[1]) / self.scale]
    }

    /// Stroke width in pixels for a relative stroke, given the output height.
    pub fn stroke_px(&self, rel: f32, frame_h: f32) -> f32 {
        // Strokes are relative to the frame height, not the zoom, so lines stay crisp.
        (rel * frame_h * 0.5 * 1.15).max(0.75)
    }
}

/// Camera that frames the body nicely from the given angles.
pub fn framing_camera(body: &Body, yaw: f32, pitch: f32, perspective: bool) -> Camera {
    let (lo, hi) = body.bounds();
    let c = lo.lerp(hi, 0.5);
    let size = (hi - lo).length().max(0.5);
    // A narrow field of view keeps the perspective distortion mild.
    let cam = Camera { yaw, pitch, distance: 1.0, target: c.to_array(), fov: 22.0, perspective };
    let half = (cam.fov.to_radians() * 0.5).tan();
    Camera { distance: (size * 0.75 / half).max(1.0), ..cam }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skeleton::{Pose, Proportions, Skeleton};

    #[test]
    fn projection_round_trip() {
        for perspective in [true, false] {
            let cam = Camera { perspective, ..Camera::default() };
            let v = View::new(&cam);
            let p = v3(0.3, 1.2, -0.4);
            let q = v.project(p);
            let back = v.unproject(q.x, q.y, q.depth);
            assert!((back - p).length() < 1e-4);
        }
    }

    #[test]
    fn front_view_shows_left_side_on_the_right() {
        let cam = Camera { yaw: 0.0, pitch: 0.0, ..Camera::default() };
        let v = View::new(&cam);
        assert!(v.project(v3(0.5, 0.85, 0.0)).x > 0.0);
    }

    #[test]
    fn hull_of_square_points() {
        let h = convex_hull(vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0], [0.5, 0.5]]);
        assert_eq!(h.len(), 4);
        assert!(point_in_convex(&h, 0.5, 0.5));
        assert!(!point_in_convex(&h, 1.5, 0.5));
    }

    #[test]
    fn draw_list_has_all_parts_and_picks() {
        let skel = Skeleton::new(Proportions::default());
        let body = Body::new(&skel, &Pose::rest(&skel));
        let cam = Camera { yaw: 0.0, pitch: 0.0, ..Camera::default() };
        let view = View::new(&cam);
        let list = draw_body(&body, &view, &Style::default());
        let filled = list.prims.iter().filter(|p| p.joint.is_some() && p.fill.is_some()).count();
        assert_eq!(filled, body.parts.len());
        assert!(list.prims.iter().all(|p| p.pts.iter().all(|q| q[0].is_finite() && q[1].is_finite())));
        let head = view.project(body.fk.pos(Joint::Head) + v3(0.0, 0.1, 0.0));
        assert_eq!(list.pick(head.x, head.y), Some(Joint::Head));
    }
}
