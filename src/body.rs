//! Mannequin body volumes (capsules and ellipsoids) attached to the skeleton,
//! plus floor snapping.

use crate::math::{Mat3, Vec3, v3};
use crate::skeleton::{BodyType, Fk, Joint, Pose, Skeleton, forward};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Volume {
    /// Tapered capsule: sphere `ra` at `a`, sphere `rb` at `b` and their hull.
    Capsule { a: Vec3, ra: f32, b: Vec3, rb: f32 },
    /// Ellipsoid `center + axes * u` for unit vectors `u` (columns = semi-axes).
    Ellipsoid { center: Vec3, axes: Mat3 },
    /// Convex hull of two ellipsoids (used for the rib cage + shoulder girdle).
    Blob { a: (Vec3, Mat3), b: (Vec3, Mat3) },
}

impl Volume {
    /// Lowest world Y of the volume.
    pub fn min_y(&self) -> f32 {
        match *self {
            Volume::Capsule { a, ra, b, rb } => (a.y - ra).min(b.y - rb),
            Volume::Ellipsoid { center, axes } => center.y - extent(&axes, Vec3::Y),
            Volume::Blob { a, b } => (a.0.y - extent(&a.1, Vec3::Y)).min(b.0.y - extent(&b.1, Vec3::Y)),
        }
    }
    pub fn center(&self) -> Vec3 {
        match *self {
            Volume::Capsule { a, b, .. } => a.lerp(b, 0.5),
            Volume::Ellipsoid { center, .. } => center,
            Volume::Blob { a, .. } => a.0,
        }
    }
    fn translate(&mut self, d: Vec3) {
        match self {
            Volume::Capsule { a, b, .. } => {
                *a += d;
                *b += d;
            }
            Volume::Ellipsoid { center, .. } => *center += d,
            Volume::Blob { a, b } => {
                a.0 += d;
                b.0 += d;
            }
        }
    }
}

/// Half extent of an ellipsoid with the given axes along unit direction `n`.
pub fn extent(axes: &Mat3, n: Vec3) -> f32 {
    let a = axes.x.dot(n);
    let b = axes.y.dot(n);
    let c = axes.z.dot(n);
    (a * a + b * b + c * c).sqrt()
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Part {
    pub joint: Joint,
    pub volume: Volume,
    /// Draw face-direction guide lines (the head).
    pub is_head: bool,
}

/// A posed mannequin: FK result plus its body volumes, in world space.
#[derive(Clone, Debug)]
pub struct Body {
    pub fk: Fk,
    pub parts: Vec<Part>,
}

struct Dims {
    limb: f32,
    thigh: f32,
    hip: f32,
    waist: f32,
    chest: f32,
}

fn dims(t: BodyType) -> Dims {
    match t {
        BodyType::Slim => Dims { limb: 0.84, thigh: 0.84, hip: 0.9, waist: 0.86, chest: 0.9 },
        BodyType::Average => Dims { limb: 1.0, thigh: 1.0, hip: 1.0, waist: 1.0, chest: 1.0 },
        BodyType::Curvy => Dims { limb: 1.04, thigh: 1.16, hip: 1.16, waist: 0.93, chest: 1.0 },
    }
}

impl Body {
    pub fn new(skel: &Skeleton, pose: &Pose) -> Body {
        let fk = forward(skel, pose);
        let parts = build_parts(skel, &fk);
        Body { fk, parts }
    }

    /// Lowest point of the whole body.
    pub fn min_y(&self) -> f32 {
        self.parts.iter().map(|p| p.volume.min_y()).fold(f32::INFINITY, f32::min)
    }

    pub fn translate(&mut self, d: Vec3) {
        self.fk.translate(d);
        for p in &mut self.parts {
            p.volume.translate(d);
        }
    }

    /// Axis-aligned bounds (approximate, from volume centres +- size).
    pub fn bounds(&self) -> (Vec3, Vec3) {
        let mut lo = v3(f32::INFINITY, f32::INFINITY, f32::INFINITY);
        let mut hi = -lo;
        for p in &self.parts {
            let (c, r) = match p.volume {
                Volume::Capsule { a, ra, b, rb } => {
                    for (q, r) in [(a, ra), (b, rb)] {
                        lo = v3(lo.x.min(q.x - r), lo.y.min(q.y - r), lo.z.min(q.z - r));
                        hi = v3(hi.x.max(q.x + r), hi.y.max(q.y + r), hi.z.max(q.z + r));
                    }
                    continue;
                }
                Volume::Ellipsoid { center, axes } => {
                    (center, v3(extent(&axes, Vec3::X), extent(&axes, Vec3::Y), extent(&axes, Vec3::Z)))
                }
                Volume::Blob { a, b } => {
                    for (c, m) in [a, b] {
                        let r = v3(extent(&m, Vec3::X), extent(&m, Vec3::Y), extent(&m, Vec3::Z));
                        lo = v3(lo.x.min(c.x - r.x), lo.y.min(c.y - r.y), lo.z.min(c.z - r.z));
                        hi = v3(hi.x.max(c.x + r.x), hi.y.max(c.y + r.y), hi.z.max(c.z + r.z));
                    }
                    continue;
                }
            };
            lo = v3(lo.x.min(c.x - r.x), lo.y.min(c.y - r.y), lo.z.min(c.z - r.z));
            hi = v3(hi.x.max(c.x + r.x), hi.y.max(c.y + r.y), hi.z.max(c.z + r.z));
        }
        (lo, hi)
    }
}

fn build_parts(skel: &Skeleton, fk: &Fk) -> Vec<Part> {
    use Joint::*;
    let s = skel.scale();
    let p = skel.props;
    let d = dims(p.body_type);
    let hs = p.head_size;
    let mut parts = Vec::with_capacity(20);
    let ell = |j: Joint, c: Vec3, half: Vec3| {
        let r = fk.rot(j);
        Volume::Ellipsoid { center: fk.pos(j) + r.mul_vec(c * s), axes: r.scale_cols(half * s) }
    };
    let cap = |j: Joint, ra: f32, rb: f32| Volume::Capsule { a: fk.pos(j), ra: ra * s, b: fk.tip(j), rb: rb * s };
    let mut push = |joint: Joint, volume: Volume| parts.push(Part { joint, volume, is_head: joint == Head });

    push(Pelvis, ell(Pelvis, v3(0.0, -0.02, -0.01), v3(0.148 * p.hip_width * d.hip, 0.108, 0.10 * d.hip.sqrt())));
    push(
        Waist,
        ell(
            Waist,
            v3(0.0, 0.065, 0.0),
            v3(0.122 * d.waist * (0.5 + 0.5 * p.shoulder_width.min(p.hip_width)), 0.095, 0.083 * d.waist),
        ),
    );
    // Rib cage merged with a flat shoulder girdle that bridges to the shoulder joints.
    let as_pair = |v: Volume| match v {
        Volume::Ellipsoid { center, axes } => (center, axes),
        _ => unreachable!(),
    };
    let ribs = as_pair(ell(
        Chest,
        v3(0.0, 0.12, 0.006),
        v3(0.148 * p.shoulder_width.sqrt() * d.chest, 0.165, 0.102 * d.chest),
    ));
    let girdle =
        as_pair(ell(Chest, v3(0.0, 0.2, -0.01), v3(0.175 * p.shoulder_width * d.chest.sqrt(), 0.055, 0.075 * d.chest)));
    push(Chest, Volume::Blob { a: ribs, b: girdle });
    push(
        Neck,
        Volume::Capsule {
            a: fk.pos(Neck) - fk.rot(Neck).mul_vec(v3(0.0, 0.02, 0.0) * s),
            ra: 0.043 * s * d.limb,
            b: fk.tip(Neck) + fk.rot(Neck).mul_vec(v3(0.0, 0.01, 0.0) * s),
            rb: 0.040 * s * d.limb,
        },
    );
    push(Head, ell(Head, v3(0.0, 0.102, 0.012) * hs, v3(0.077, 0.112, 0.094) * hs));
    for (ua, fa, ha, th, sh, ft) in
        [(UpperArmL, ForearmL, HandL, ThighL, ShinL, FootL), (UpperArmR, ForearmR, HandR, ThighR, ShinR, FootR)]
    {
        push(ua, cap(ua, 0.047 * d.limb, 0.036 * d.limb));
        push(fa, cap(fa, 0.035 * d.limb, 0.026 * d.limb));
        push(ha, ell(ha, v3(0.0, -0.082, 0.004), v3(0.021, 0.086, 0.044) * d.limb.sqrt()));
        push(th, cap(th, 0.074 * d.thigh, 0.050 * d.limb));
        push(sh, cap(sh, 0.049 * d.limb, 0.031 * d.limb));
        push(ft, ell(ft, v3(0.0, -0.040, 0.05), v3(0.042, 0.031, 0.11)));
    }
    parts
}

/// Move the pose vertically so that the lowest body point rests on the floor.
pub fn snap_to_floor(skel: &Skeleton, pose: &mut Pose) {
    let body = Body::new(skel, pose);
    let m = body.min_y();
    // Ignore float noise so repeated snapping is idempotent (keeps undo history clean).
    if m.is_finite() && m.abs() > 1e-4 {
        pose.root.y -= m;
    }
}

/// World position of the drag handle that controls `j`.
///
/// The pelvis handle sits on the pelvis itself (it translates the figure), the head
/// handle on the face, every other handle at the tip of the joint's bone.
pub fn handle_point(skel: &Skeleton, fk: &Fk, j: Joint) -> Vec3 {
    match j {
        Joint::Pelvis => fk.pos(j),
        Joint::Head => {
            let hs = skel.props.head_size * skel.scale();
            fk.pos(j) + fk.rot(j).mul_vec(v3(0.0, 0.10, 0.105) * hs)
        }
        _ => fk.tip(j),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skeleton::Proportions;

    #[test]
    fn rest_pose_stands_on_floor() {
        let skel = Skeleton::new(Proportions::default());
        let body = Body::new(&skel, &Pose::rest(&skel));
        assert!(body.min_y().abs() < 0.01, "{}", body.min_y());
        let mut pose = Pose::rest(&skel);
        pose.root.y += 0.5;
        snap_to_floor(&skel, &mut pose);
        assert!(Body::new(&skel, &pose).min_y().abs() < 1e-4);
        // Snapping an already grounded pose must not change it (undo history relies on it).
        let before = pose.clone();
        snap_to_floor(&skel, &mut pose);
        assert_eq!(before, pose);
    }

    #[test]
    fn ellipsoid_extent() {
        let axes = Mat3::IDENTITY.scale_cols(v3(1.0, 2.0, 3.0));
        assert!((extent(&axes, Vec3::Y) - 2.0).abs() < 1e-6);
        let r = Mat3::rot_x(std::f32::consts::FRAC_PI_2).mul(&axes);
        assert!((extent(&r, Vec3::Y) - 3.0).abs() < 1e-5);
    }
}
