//! Built-in preset poses.
//!
//! The 25 presets are original pose data inspired by common figure-drawing
//! reference pose types (kneeling, sitting, lying, crouching, all-fours...).
//! They are authored with a small builder that works in world space: the torso
//! is oriented with "up/forward" vectors, limbs are placed with two-bone IK
//! targets or bone directions, and the result is snapped onto the floor.
//! Because the presets are generated from the current skeleton they adapt to
//! the chosen body proportions.

use crate::body::{Body, snap_to_floor};
use crate::math::{Mat3, Vec3, v3};
use crate::posing::{ik_limb, orient, set_limb, set_world_rot};
use crate::skeleton::{Joint, Pose, Side, Skeleton, forward};

#[derive(Clone, Debug, PartialEq)]
pub struct Preset {
    pub name: &'static str,
    /// Suggested viewing angles for thumbnails / contact sheets.
    pub yaw: f32,
    pub pitch: f32,
    pub pose: Pose,
}

fn d(x: f32, y: f32, z: f32) -> Vec3 {
    v3(x, y, z).normalized()
}

/// World-space pose builder.
#[derive(Clone)]
struct B<'a> {
    s: &'a Skeleton,
    k: f32,
    pose: Pose,
}

impl<'a> B<'a> {
    fn new(s: &'a Skeleton) -> Self {
        B { s, k: s.scale(), pose: Pose::rest(s) }
    }
    fn root(&mut self, x: f32, y: f32, z: f32) -> &mut Self {
        self.pose.root = v3(x, y, z) * self.k;
        self
    }
    fn orient(&mut self, j: Joint, up: Vec3, fwd: Vec3) -> &mut Self {
        orient(self.s, &mut self.pose, j, up, fwd);
        self
    }
    /// Pelvis, waist and chest "up" directions sharing one facing direction.
    fn spine(&mut self, pelvis: Vec3, waist: Vec3, chest: Vec3, fwd: Vec3) -> &mut Self {
        self.orient(Joint::Pelvis, pelvis, fwd).orient(Joint::Waist, waist, fwd).orient(Joint::Chest, chest, fwd)
    }
    /// Head orientation; the neck takes the in-between direction.
    fn head(&mut self, up: Vec3, fwd: Vec3) -> &mut Self {
        let fk = forward(self.s, &self.pose);
        let c = fk.rot(Joint::Chest);
        let nu = (c.y + up.normalized()).normalized();
        let nf = (c.z + fwd.normalized()).normalized();
        self.orient(Joint::Neck, nu, nf).orient(Joint::Head, up, fwd)
    }
    fn pos(&self, j: Joint) -> Vec3 {
        forward(self.s, &self.pose).pos(j)
    }
    /// A point given in joint-local coordinates (reference-figure metres).
    fn at(&self, j: Joint, x: f32, y: f32, z: f32) -> Vec3 {
        let fk = forward(self.s, &self.pose);
        fk.pos(j) + fk.rot(j).mul_vec(v3(x, y, z) * self.k)
    }
    /// `base + offset` with the offset in reference-figure metres.
    fn off(&self, base: Vec3, x: f32, y: f32, z: f32) -> Vec3 {
        base + v3(x, y, z) * self.k
    }
    fn limb(&mut self, upper: Joint, du: Vec3, dl: Vec3) -> &mut Self {
        set_limb(self.s, &mut self.pose, upper, du, dl);
        self
    }
    fn ik(&mut self, hinge: Joint, target: Vec3, pole: Vec3) -> &mut Self {
        ik_limb(self.s, &mut self.pose, hinge, target, Some(pole));
        self
    }
    /// Foot: toes along `toe`, top of the foot towards `up`.
    fn foot(&mut self, j: Joint, toe: Vec3, up: Vec3) -> &mut Self {
        let z = toe.normalized();
        let y = up.reject(z).normalized_or(Vec3::Y);
        set_world_rot(self.s, &mut self.pose, j, Mat3::from_cols(y.cross(z), y, z));
        self
    }
    /// Hand: fingers along `finger`, palm facing `palm`.
    fn hand(&mut self, j: Joint, finger: Vec3, palm: Vec3) -> &mut Self {
        let y = -finger.normalized();
        // The back of the left hand is local +X, of the right hand local -X.
        let back = if j.side() == Side::Right { palm } else { -palm };
        let x = back.reject(y).normalized_or(Vec3::X);
        set_world_rot(self.s, &mut self.pose, j, Mat3::from_cols(x, y, x.cross(y)));
        self
    }
    /// Try `steps` values of a parameter in `[lo, hi]` and apply the one whose
    /// `apply` result (an error measure) is smallest.
    fn best(&mut self, lo: f32, hi: f32, steps: usize, apply: impl Fn(&mut B<'a>, f32) -> f32) -> &mut Self {
        let mut best = (f32::INFINITY, lo);
        for i in 0..=steps {
            let t = lo + (hi - lo) * i as f32 / steps as f32;
            let mut trial = self.clone();
            let err = apply(&mut trial, t);
            if err < best.0 {
                best = (err, t);
            }
        }
        apply(self, best.1);
        self
    }
    /// Lowest point of the given body parts.
    fn min_y_of(&self, joints: &[Joint]) -> f32 {
        let body = Body::new(self.s, &self.pose);
        body.parts.iter().filter(|p| joints.contains(&p.joint)).map(|p| p.volume.min_y()).fold(f32::INFINITY, f32::min)
    }
    fn done(&mut self) -> Pose {
        let mut p = self.pose.clone();
        snap_to_floor(self.s, &mut p);
        p
    }
}

use Joint::*;

/// All built-in presets for the given skeleton.
pub fn presets(s: &Skeleton) -> Vec<Preset> {
    let mut v = Vec::with_capacity(25);
    let mut add = |name: &'static str, yaw: f32, pitch: f32, pose: Pose| v.push(Preset { name, yaw, pitch, pose });

    // 1. Seiza: kneeling, sitting on the heels, hands on the thighs.
    {
        let mut b = B::new(s);
        b.root(0.0, 0.28, 0.0).spine(d(0.0, 1.0, 0.08), d(0.0, 1.0, 0.0), d(0.0, 1.0, -0.06), Vec3::Z);
        b.head(d(0.0, 1.0, 0.06), d(0.0, -0.08, 1.0));
        for (sx, th, sh, ft) in [(1.0, ThighL, ShinL, FootL), (-1.0, ThighR, ShinR, FootR)] {
            let knee = b.off(Vec3::ZERO, sx * 0.11, 0.05, 0.39);
            let hip = b.pos(th);
            b.ik(sh, b.off(Vec3::ZERO, sx * 0.055, 0.065, -0.02), knee - hip);
            b.foot(ft, d(-sx * 0.15, -0.3, -1.0), d(0.0, -1.0, -0.3));
        }
        for (sx, fa, ha) in [(1.0, ForearmL, HandL), (-1.0, ForearmR, HandR)] {
            b.ik(fa, b.off(Vec3::ZERO, sx * 0.14, 0.24, 0.14), d(sx * 0.4, 0.0, -1.0));
            b.hand(ha, d(-sx * 0.1, -0.4, 1.0), d(0.0, -1.0, 0.0));
        }
        add("Seiza (kneeling on heels)", 35.0, 10.0, b.done());
    }

    // 2. Kneeling, twisting to look back over the shoulder, one hand on the floor.
    {
        let mut b = B::new(s);
        b.root(0.0, 0.30, 0.0).orient(Pelvis, d(0.08, 1.0, 0.05), Vec3::Z);
        b.orient(Waist, d(0.12, 1.0, 0.0), d(0.3, 0.0, 1.0)).orient(Chest, d(0.2, 1.0, -0.05), d(0.8, 0.0, 0.6));
        b.head(d(0.05, 1.0, 0.0), d(0.9, -0.05, -0.35));
        for (sx, th, sh, ft) in [(1.0, ThighL, ShinL, FootL), (-1.0, ThighR, ShinR, FootR)] {
            let knee = b.off(Vec3::ZERO, sx * 0.12, 0.05, 0.38);
            let hip = b.pos(th);
            b.ik(sh, b.off(Vec3::ZERO, sx * 0.06, 0.065, -0.02), knee - hip);
            b.foot(ft, d(-sx * 0.15, -0.3, -1.0), d(0.0, -1.0, -0.3));
        }
        let sh = b.pos(UpperArmL);
        b.ik(ForearmL, v3(sh.x + 0.14 * b.k, 0.03 * b.k, sh.z - 0.12 * b.k), d(0.0, 0.0, -1.0));
        b.hand(HandL, d(0.4, 0.0, -1.0), d(0.0, -1.0, 0.0));
        b.ik(ForearmR, b.off(Vec3::ZERO, 0.10, 0.22, 0.24), d(-0.6, -0.3, -0.6));
        b.hand(HandR, d(0.4, -0.4, 1.0), d(0.0, -1.0, 0.0));
        add("Kneeling, looking back", 140.0, 10.0, b.done());
    }

    // 3. Lying on the right side, propped on the elbow, head resting on the hand.
    {
        let mut b = B::new(s);
        b.root(0.0, 0.165, 0.0).orient(Pelvis, d(-1.0, 0.0, 0.0), Vec3::Z);
        // Raise the chest until the right elbow (upper arm hanging almost straight
        // down from the shoulder) rests on the floor.
        let k = b.k;
        let upper_dir = d(0.07, -1.0, 0.33);
        b.best(10.0, 85.0, 150, |b, a| {
            let a = a.to_radians();
            b.orient(Waist, d(-1.0, 0.35 * a.sin(), 0.0), Vec3::Z).orient(Chest, d(-a.cos(), a.sin(), 0.0), Vec3::Z);
            (b.pos(UpperArmR).y + upper_dir.y * 0.29 * k - 0.036 * k).abs()
        });
        b.head(d(-0.45, 1.0, 0.0), d(0.1, -0.1, 1.0));
        let sh = b.pos(UpperArmR);
        let elbow = sh + upper_dir * (0.29 * k);
        let cheek = b.at(Head, -0.07, 0.0, 0.03);
        b.limb(UpperArmR, upper_dir, cheek - elbow);
        b.hand(HandR, d(-0.2, 1.0, 0.0), d(1.0, 0.0, 0.2));
        let hip_r = b.pos(ThighR);
        let hip_l = b.pos(ThighL);
        b.ik(ShinR, b.off(hip_r, 0.80, -0.02, 0.12), d(0.0, 0.0, 1.0));
        b.ik(ShinL, b.off(hip_l, 0.62, -0.15, 0.32), d(0.0, 0.0, 1.0));
        b.foot(FootR, d(0.25, 0.0, 1.0), d(-1.0, 0.0, 0.0)).foot(FootL, d(0.35, -0.2, 1.0), d(-1.0, 0.0, 0.0));
        b.ik(ForearmL, b.off(hip_l, 0.12, 0.04, 0.10), d(0.0, 0.3, -1.0));
        b.hand(HandL, d(1.0, -0.2, 0.2), d(0.0, -1.0, 0.0));
        add("Side-lying, propped on elbow", 10.0, 14.0, b.done());
    }

    // 4. Lying on the stomach on the elbows, chin in hands, lower legs up.
    {
        let mut b = B::new(s);
        b.root(0.0, 0.10, 0.0).orient(Pelvis, d(-1.0, 0.0, 0.0), -Vec3::Y);
        b.orient(Waist, d(-1.0, 0.32, 0.0), -Vec3::Y).orient(Chest, d(-0.8, 0.6, 0.0), -Vec3::Y);
        b.head(d(-0.25, 1.0, 0.0), d(-1.0, -0.15, 0.0));
        for (sz, fa, ha) in [(1.0, ForearmL, HandL), (-1.0, ForearmR, HandR)] {
            let chin = b.at(Head, sz * 0.05, -0.0, 0.07);
            b.ik(fa, chin, d(0.0, -1.0, sz * 0.3));
            b.hand(ha, d(0.2, 1.0, -sz * 0.2), d(-1.0, 0.0, 0.0));
        }
        b.limb(ThighL, d(1.0, -0.1, 0.06), d(0.35, 1.0, -0.12)).limb(ThighR, d(1.0, -0.1, -0.06), d(0.15, 1.0, 0.22));
        b.foot(FootL, d(0.2, 1.0, -0.1), d(-1.0, 0.1, 0.0)).foot(FootR, d(0.1, 1.0, 0.2), d(-1.0, 0.1, 0.0));
        add("Lying on stomach, legs up", 25.0, 12.0, b.done());
    }

    // 5. All fours (hands and knees).
    {
        let mut b = B::new(s);
        b.root(0.0, 0.47, 0.0).spine(d(0.0, 0.12, 1.0), d(0.0, 0.2, 1.0), d(0.0, 0.25, 1.0), -Vec3::Y);
        b.head(d(0.0, 0.7, 1.0), d(0.0, -1.0, 0.6));
        for (sx, th, sh, ft) in [(1.0, ThighL, ShinL, FootL), (-1.0, ThighR, ShinR, FootR)] {
            let hip = b.pos(th);
            b.ik(sh, v3(hip.x + sx * 0.01 * b.k, 0.055 * b.k, hip.z - 0.41 * b.k), d(0.0, -1.0, 1.0));
            b.foot(ft, d(0.0, -0.2, -1.0), d(0.0, -1.0, 0.2));
        }
        for (sx, fa, ha) in [(1.0, ForearmL, HandL), (-1.0, ForearmR, HandR)] {
            let sh = b.pos(fa.parent().unwrap());
            b.ik(fa, v3(sh.x + sx * 0.02 * b.k, 0.028 * b.k, sh.z + 0.02 * b.k), d(0.0, 0.0, -1.0));
            b.hand(ha, d(-sx * 0.1, -0.12, 1.0), d(0.0, -1.0, 0.0));
        }
        add("All fours", 70.0, 12.0, b.done());
    }

    // 6. Sitting with knees up, hugging the knees.
    {
        let mut b = B::new(s);
        b.root(0.0, 0.12, 0.0).spine(d(0.0, 1.0, -0.3), d(0.0, 1.0, 0.1), d(0.0, 1.0, 0.35), Vec3::Z);
        b.head(d(0.0, 1.0, 0.15), d(0.0, -0.2, 1.0));
        for (sx, th, sh, ft) in [(1.0, ThighL, ShinL, FootL), (-1.0, ThighR, ShinR, FootR)] {
            let _ = th;
            b.ik(sh, b.off(Vec3::ZERO, sx * 0.12, 0.075, 0.45), d(0.0, 1.0, 0.3));
            b.foot(ft, d(0.0, 0.0, 1.0), Vec3::Y);
        }
        b.ik(ForearmL, b.off(Vec3::ZERO, -0.03, 0.33, 0.40), d(1.0, 0.0, -0.2));
        b.hand(HandL, d(-1.0, 0.0, 0.0), d(0.0, 0.0, -1.0));
        b.ik(ForearmR, b.off(Vec3::ZERO, 0.03, 0.30, 0.41), d(-1.0, 0.0, -0.2));
        b.hand(HandR, d(1.0, 0.0, 0.0), d(0.0, 0.0, -1.0));
        add("Sitting, hugging knees", 40.0, 10.0, b.done());
    }

    // 7. Side-saddle sitting: legs folded to the right, leaning on the left hand.
    {
        let mut b = B::new(s);
        b.root(0.0, 0.12, 0.0).spine(d(0.06, 1.0, 0.0), d(0.18, 1.0, 0.0), d(0.12, 1.0, 0.0), d(0.15, 0.0, 1.0));
        b.head(d(-0.08, 1.0, 0.0), d(-0.25, -0.05, 1.0));
        // Thigh slope chosen so that the hip and the folded legs all rest on the floor.
        b.best(-0.3, 0.3, 60, |b, t| {
            b.limb(ThighL, d(0.2, t, 1.0), d(-1.0, -0.02, -0.3));
            b.limb(ThighR, d(-0.35, t + 0.04, 0.9), d(-0.55, -0.02, -1.0));
            (b.min_y_of(&[Pelvis]) - b.min_y_of(&[ThighL, ShinL, ShinR, ThighR])).abs()
        });
        b.foot(FootL, d(-1.0, 0.0, -0.4), d(0.0, 0.3, -1.0)).foot(FootR, d(-0.6, 0.0, -1.0), d(0.0, 0.3, -1.0));
        let sh = b.pos(UpperArmL);
        b.ik(ForearmL, v3(sh.x + 0.22 * b.k, 0.03 * b.k, sh.z - 0.06 * b.k), d(0.0, 0.0, -1.0));
        b.hand(HandL, d(0.6, -0.1, -0.4), d(0.0, -1.0, 0.0));
        let knee = b.pos(ShinL);
        b.ik(ForearmR, b.off(knee, -0.05, 0.10, -0.06), d(-1.0, -0.5, -0.2));
        b.hand(HandR, d(0.6, -0.4, 0.5), d(0.0, -1.0, 0.0));
        add("Side-saddle sitting", 20.0, 12.0, b.done());
    }

    // 8. Deep crouch on the balls of the feet, forearms on the knees.
    {
        let mut b = B::new(s);
        b.root(0.0, 0.30, 0.0).spine(d(0.0, 1.0, 0.25), d(0.0, 1.0, 0.35), d(0.0, 1.0, 0.45), Vec3::Z);
        b.head(d(0.0, 1.0, 0.1), d(0.0, -0.1, 1.0));
        for (sx, th, sh, ft) in [(1.0, ThighL, ShinL, FootL), (-1.0, ThighR, ShinR, FootR)] {
            let hip = b.pos(th);
            let knee = b.off(Vec3::ZERO, sx * 0.2, 0.47, 0.33);
            b.ik(sh, b.off(Vec3::ZERO, sx * 0.15, 0.14, 0.09), knee - hip);
            b.foot(ft, d(sx * 0.15, -0.55, 1.0), d(0.0, 1.0, 0.55));
        }
        for (sx, fa, ha) in [(1.0, ForearmL, HandL), (-1.0, ForearmR, HandR)] {
            let knee = b.pos(if sx > 0.0 { ShinL } else { ShinR });
            b.ik(fa, b.off(knee, -sx * 0.06, -0.06, 0.16), d(sx * 0.4, -0.2, -1.0));
            b.hand(ha, d(0.0, -1.0, 0.25), d(-sx, 0.0, 0.0));
        }
        add("Crouching on toes", 50.0, 8.0, b.done());
    }

    // 9. Cat stretch: knees under the hips, chest low, arms reaching forward.
    {
        let mut b = B::new(s);
        b.root(0.0, 0.47, 0.0).spine(d(0.0, -0.85, 1.0), d(0.0, -0.9, 1.0), d(0.0, -0.6, 1.0), -Vec3::Y);
        b.head(d(0.0, 0.25, 1.0), d(0.0, -1.0, 0.25));
        for (sx, th, sh, ft) in [(1.0, ThighL, ShinL, FootL), (-1.0, ThighR, ShinR, FootR)] {
            let hip = b.pos(th);
            b.ik(sh, v3(hip.x + sx * 0.01 * b.k, 0.055 * b.k, hip.z - 0.43 * b.k), d(0.0, -1.0, 1.0));
            b.foot(ft, d(0.0, -0.2, -1.0), d(0.0, -1.0, 0.2));
        }
        for (sx, fa, ha) in [(1.0, ForearmL, HandL), (-1.0, ForearmR, HandR)] {
            let sh = b.pos(fa.parent().unwrap());
            b.ik(fa, v3(sh.x + sx * 0.04 * b.k, 0.026 * b.k, sh.z + 0.6 * b.k), d(0.0, 1.0, 0.0));
            b.hand(ha, d(0.0, -0.1, 1.0), d(0.0, -1.0, 0.0));
        }
        add("Cat stretch", 80.0, 12.0, b.done());
    }

    // 10. Lying on the back, one knee raised, arms above the head.
    {
        let mut b = B::new(s);
        b.root(0.0, 0.10, 0.0).spine(d(-1.0, 0.0, 0.0), d(-1.0, 0.0, 0.0), d(-1.0, 0.05, 0.0), Vec3::Y);
        b.head(d(-1.0, 0.15, 0.0), d(0.15, 1.0, 0.25));
        b.limb(ThighL, d(1.0, -0.06, -0.06), d(1.0, -0.02, -0.02));
        let hip = b.pos(ThighR);
        b.ik(ShinR, v3(hip.x + 0.48 * b.k, 0.075 * b.k, hip.z + 0.03 * b.k), d(0.0, 1.0, 0.0));
        b.foot(FootL, d(0.2, 1.0, -0.15), d(-1.0, 0.2, 0.0)).foot(FootR, d(1.0, 0.0, 0.0), Vec3::Y);
        b.limb(UpperArmL, d(-0.85, -0.12, -0.5), d(-0.35, -0.05, 0.95));
        b.limb(UpperArmR, d(-0.8, -0.12, 0.55), d(-0.3, -0.05, -0.95));
        b.hand(HandL, d(-0.4, 0.0, 1.0), Vec3::Y).hand(HandR, d(-0.4, 0.0, -1.0), Vec3::Y);
        add("Lying on back, knee raised", 20.0, 25.0, b.done());
    }

    // 11. Tall kneeling with hands on the hips.
    {
        let mut b = B::new(s);
        b.root(0.0, 0.54, 0.0).spine(d(0.06, 1.0, 0.0), d(-0.03, 1.0, 0.0), d(-0.08, 1.0, -0.04), d(-0.1, 0.0, 1.0));
        b.head(d(0.0, 1.0, 0.0), d(-0.5, -0.05, 1.0));
        for (sx, th, sh, ft) in [(1.0, ThighL, ShinL, FootL), (-1.0, ThighR, ShinR, FootR)] {
            let hip = b.pos(th);
            b.ik(sh, v3(hip.x + sx * 0.02 * b.k, 0.06 * b.k, hip.z - 0.40 * b.k), d(0.0, -0.2, 1.0));
            b.foot(ft, d(0.0, -0.3, -1.0), d(0.0, -1.0, 0.3));
        }
        for (sx, fa, ha) in [(1.0, ForearmL, HandL), (-1.0, ForearmR, HandR)] {
            let side = b.at(Pelvis, sx * 0.16, 0.06, 0.0);
            b.ik(fa, side, d(sx, 0.0, -0.3));
            b.hand(ha, d(-sx * 0.3, -0.6, 1.0), d(-sx, 0.0, 0.0));
        }
        add("Tall kneeling, hands on hips", -35.0, 8.0, b.done());
    }

    // 12. Cross-legged sitting, hands on the knees.
    {
        let mut b = B::new(s);
        b.root(0.0, 0.12, 0.0).spine(d(0.0, 1.0, -0.05), d(0.0, 1.0, 0.03), d(0.0, 1.0, 0.05), Vec3::Z);
        b.head(d(0.08, 1.0, 0.0), d(0.0, -0.1, 1.0));
        b.ik(ShinL, b.off(Vec3::ZERO, -0.16, 0.06, 0.30), d(1.0, 0.25, 0.6));
        b.ik(ShinR, b.off(Vec3::ZERO, 0.17, 0.06, 0.13), d(-1.0, 0.25, 0.6));
        b.foot(FootL, d(-1.0, 0.0, -0.2), d(0.0, 0.3, 1.0)).foot(FootR, d(1.0, 0.0, -0.2), d(0.0, 0.3, 1.0));
        for (sx, fa, ha, sh) in [(1.0, ForearmL, HandL, ShinL), (-1.0, ForearmR, HandR, ShinR)] {
            let knee = b.pos(sh);
            b.ik(fa, b.off(knee, -sx * 0.03, 0.08, -0.06), d(sx, 0.0, -0.5));
            b.hand(ha, d(0.0, -0.6, 1.0), d(0.0, -1.0, 0.0));
        }
        add("Cross-legged", 25.0, 12.0, b.done());
    }

    // 13. Sitting with the legs out, leaning back on the hands, one knee up.
    {
        let mut b = B::new(s);
        b.root(0.0, 0.12, 0.0).spine(d(0.0, 1.0, -0.35), d(0.0, 1.0, -0.3), d(0.0, 1.0, -0.25), Vec3::Z);
        b.head(d(0.0, 1.0, 0.05), d(0.1, 0.0, 1.0));
        b.best(-0.2, 0.2, 80, |b, t| {
            b.limb(ThighL, d(0.05, t, 1.0), d(0.02, t, 1.0));
            b.foot(FootL, d(0.05, 1.0, 0.25), d(0.0, 0.25, -1.0));
            (b.min_y_of(&[Pelvis, ThighL]) - b.min_y_of(&[FootL])).abs()
        });
        let hip = b.pos(ThighR);
        let floor = b.min_y_of(&[Pelvis]);
        b.ik(ShinR, v3(hip.x - 0.04 * b.k, floor + 0.075 * b.k, hip.z + 0.6 * b.k), d(0.0, 1.0, 0.2));
        b.foot(FootR, Vec3::Z, Vec3::Y);
        for (sx, fa, ha) in [(1.0, ForearmL, HandL), (-1.0, ForearmR, HandR)] {
            let sh = b.pos(fa.parent().unwrap());
            b.ik(fa, v3(sh.x + sx * 0.05 * b.k, floor + 0.028 * b.k, sh.z - 0.14 * b.k), d(0.0, 0.0, 1.0));
            b.hand(ha, d(sx * 0.2, -0.12, -1.0), d(0.0, -1.0, 0.0));
        }
        add("Sitting, leaning back on hands", 60.0, 10.0, b.done());
    }

    // 14. Curled up on the left side.
    {
        let mut b = B::new(s);
        b.root(0.0, 0.15, 0.0).orient(Pelvis, d(-1.0, 0.0, 0.0), -Vec3::Z);
        let shin_l = d(1.0, 0.0, 0.1);
        let shin_r = d(1.0, -0.12, 0.0);
        b.limb(ThighL, d(-0.72, 0.0, -0.7), shin_l).limb(ThighR, d(-0.78, -0.12, -0.62), shin_r);
        b.foot(FootL, d(0.3, 0.0, -1.0), -shin_l).foot(FootR, d(0.3, -0.1, -1.0), -shin_r);
        // Tilt the curled spine so the shoulder and the hip both rest on the floor.
        b.best(-0.1, 0.5, 60, |b, t| {
            b.orient(Waist, d(-1.0, t * 0.5, -0.25), -Vec3::Z).orient(Chest, d(-0.92, t, -0.4), -Vec3::Z);
            b.limb(UpperArmL, d(-0.25, 0.0, -1.0), d(-0.9, 0.05, -0.2));
            (b.min_y_of(&[UpperArmL, Chest]) - b.min_y_of(&[Pelvis, ThighL])).abs()
        });
        b.head(d(-0.85, -0.05, -0.5), d(-0.45, -0.1, -1.0));
        b.limb(UpperArmR, d(0.15, -0.55, -0.85), d(-0.45, -0.45, -0.75));
        b.hand(HandL, d(-0.9, 0.05, -0.2), d(0.0, 1.0, 0.0)).hand(HandR, d(-0.3, -0.25, -1.0), d(0.0, -1.0, 0.0));
        add("Curled up on side", 190.0, 35.0, b.done());
    }

    // 15. Lying on the back with both knees up, a hand on the stomach.
    {
        let mut b = B::new(s);
        b.root(0.0, 0.10, 0.0).spine(d(-1.0, 0.0, 0.0), d(-1.0, 0.0, 0.0), d(-1.0, 0.05, 0.0), Vec3::Y);
        b.head(d(-1.0, 0.1, 0.0), d(0.1, 1.0, 0.6));
        for (sz, th, sh, ft) in [(-1.0, ThighL, ShinL, FootL), (1.0, ThighR, ShinR, FootR)] {
            let hip = b.pos(th);
            b.ik(sh, v3(hip.x + 0.5 * b.k, 0.075 * b.k, hip.z + sz * 0.06 * b.k), d(0.0, 1.0, sz * 0.15));
            b.foot(ft, d(1.0, 0.0, sz * 0.1), Vec3::Y);
        }
        b.limb(UpperArmL, d(0.9, -0.08, -0.35), d(1.0, -0.05, -0.15));
        b.hand(HandL, d(1.0, -0.1, 0.0), -Vec3::Y);
        let belly = b.at(Waist, 0.0, 0.05, 0.13);
        b.ik(ForearmR, belly, d(0.2, -0.2, 1.0));
        b.hand(HandR, d(0.1, 0.0, -1.0), -Vec3::Y);
        add("Lying on back, knees up", 30.0, 30.0, b.done());
    }

    // 16. Lying on the stomach, head turned, arms folded under the head.
    {
        let mut b = B::new(s);
        b.root(0.0, 0.10, 0.0).spine(Vec3::Z, d(0.0, 0.02, 1.0), d(0.0, 0.04, 1.0), -Vec3::Y);
        b.head(d(0.0, 0.25, 1.0), d(1.0, -0.35, 0.0));
        for (sx, ua) in [(1.0, UpperArmL), (-1.0, UpperArmR)] {
            b.limb(ua, d(sx * 0.45, -0.1, 0.9), d(-sx * 1.0, 0.0, 0.1));
        }
        b.hand(HandL, d(-1.0, 0.0, 0.1), -Vec3::Y).hand(HandR, d(1.0, 0.0, 0.1), -Vec3::Y);
        b.limb(ThighL, d(0.55, -0.08, -0.85), d(-0.25, -0.02, -1.0));
        b.limb(ThighR, d(-0.08, -0.06, -1.0), d(-0.02, -0.02, -1.0));
        b.foot(FootL, d(0.0, -0.3, -1.0), d(0.3, -1.0, 0.0)).foot(FootR, d(0.0, -0.3, -1.0), d(0.0, -1.0, 0.0));
        add("Lying on stomach, head on arms", 125.0, 28.0, b.done());
    }

    // 17. Kneeling on one knee, forearm resting on the raised knee.
    {
        let mut b = B::new(s);
        b.root(0.0, 0.52, 0.0).spine(d(0.0, 1.0, 0.05), d(0.0, 1.0, 0.2), d(0.0, 1.0, 0.38), Vec3::Z);
        b.head(d(0.0, 1.0, 0.0), d(0.0, -0.05, 1.0));
        let hip_l = b.pos(ThighL);
        b.ik(ShinL, v3(hip_l.x + 0.03 * b.k, 0.062 * b.k, hip_l.z + 0.46 * b.k), d(0.0, 1.0, 1.0));
        b.foot(FootL, Vec3::Z, Vec3::Y);
        let hip_r = b.pos(ThighR);
        b.ik(ShinR, v3(hip_r.x - 0.01 * b.k, 0.15 * b.k, hip_r.z - 0.40 * b.k), d(0.0, -1.0, 0.3));
        b.foot(FootR, d(0.0, -0.75, 0.45), d(0.0, 0.45, 0.75));
        let knee = b.pos(ShinL);
        b.ik(ForearmL, b.off(knee, -0.08, 0.07, 0.10), d(0.2, -1.0, -0.2));
        b.hand(HandL, d(-0.3, -0.6, 0.6), d(-1.0, 0.0, 0.0));
        let hip = b.at(Pelvis, -0.15, 0.03, 0.04);
        b.ik(ForearmR, hip, d(-1.0, 0.0, -0.3));
        b.hand(HandR, d(0.0, -0.6, 1.0), d(1.0, 0.0, 0.0));
        add("One-knee kneel", 60.0, 8.0, b.done());
    }

    // 18. Sitting, hugging one knee, the other leg folded flat.
    {
        let mut b = B::new(s);
        b.root(0.0, 0.12, 0.0).spine(d(0.0, 1.0, -0.15), d(0.05, 1.0, 0.15), d(0.12, 1.0, 0.35), Vec3::Z);
        b.head(d(0.35, 1.0, 0.2), d(-0.3, -0.25, 1.0));
        b.ik(ShinL, b.off(Vec3::ZERO, 0.13, 0.075, 0.36), d(0.0, 1.0, 0.3));
        b.foot(FootL, Vec3::Z, Vec3::Y);
        b.ik(ShinR, b.off(Vec3::ZERO, 0.0, 0.06, 0.22), d(-1.0, 0.1, 0.4));
        b.foot(FootR, d(1.0, 0.0, -0.1), d(0.0, 0.3, 1.0));
        let knee = b.pos(ShinL);
        b.ik(ForearmL, b.off(knee, 0.03, -0.08, 0.10), d(1.0, -0.2, -0.3));
        b.hand(HandL, d(-1.0, 0.0, 0.0), d(0.0, 0.0, -1.0));
        b.ik(ForearmR, b.off(knee, 0.05, -0.12, 0.10), d(-1.0, -0.2, -0.3));
        b.hand(HandR, d(1.0, 0.0, 0.0), d(0.0, 0.0, -1.0));
        add("Hugging one knee", 35.0, 12.0, b.done());
    }

    // 19. Low asymmetric crouch, one hand touching the floor, looking aside.
    {
        let mut b = B::new(s);
        b.root(0.0, 0.32, 0.0).spine(d(0.0, 1.0, 0.3), d(0.0, 1.0, 0.45), d(-0.1, 1.0, 0.55), d(-0.15, 0.0, 1.0));
        b.head(d(0.0, 1.0, 0.15), d(-0.9, -0.2, 0.5));
        let hip_l = b.pos(ThighL);
        b.ik(ShinL, b.off(Vec3::ZERO, 0.17, 0.06, 0.10), b.off(Vec3::ZERO, 0.22, 0.52, 0.30) - hip_l);
        b.foot(FootL, d(0.15, 0.0, 1.0), Vec3::Y);
        let hip_r = b.pos(ThighR);
        b.ik(ShinR, b.off(Vec3::ZERO, -0.16, 0.14, -0.12), b.off(Vec3::ZERO, -0.2, 0.2, 0.32) - hip_r);
        b.foot(FootR, d(-0.1, -0.6, 1.0), d(0.0, 1.0, 0.6));
        b.ik(ForearmR, b.off(Vec3::ZERO, -0.05, 0.03, 0.42), d(-0.5, 0.0, -1.0));
        b.hand(HandR, d(0.0, 0.0, 1.0), -Vec3::Y);
        let knee = b.pos(ShinL);
        b.ik(ForearmL, b.off(knee, -0.02, -0.08, 0.14), d(0.5, -0.3, -1.0));
        b.hand(HandL, d(0.0, -1.0, 0.2), d(-1.0, 0.0, 0.0));
        add("Low crouch, hand on floor", -30.0, 8.0, b.done());
    }

    // 20. Lying on the right side, head on the outstretched arm, top knee forward.
    {
        let mut b = B::new(s);
        b.root(0.0, 0.15, 0.0).orient(Pelvis, d(-1.0, 0.0, 0.0), Vec3::Z);
        b.limb(ThighR, d(1.0, -0.06, 0.12), d(1.0, -0.02, -0.12));
        b.limb(ThighL, d(0.35, -0.32, 0.9), d(0.95, -0.15, -0.25));
        b.foot(FootR, d(0.2, 0.0, 1.0), d(-1.0, 0.0, 0.0)).foot(FootL, d(0.3, -0.3, 1.0), d(-1.0, 0.0, 0.0));
        b.best(-0.1, 0.5, 60, |b, t| {
            b.orient(Waist, d(-1.0, t * 0.5, 0.0), Vec3::Z).orient(Chest, d(-1.0, t, 0.0), Vec3::Z);
            b.limb(UpperArmR, d(-1.0, 0.0, 0.08), d(-1.0, 0.0, 0.1));
            (b.min_y_of(&[UpperArmR, Chest]) - b.min_y_of(&[Pelvis, ThighR])).abs()
        });
        b.hand(HandR, d(-1.0, 0.0, 0.1), -Vec3::Y);
        b.head(d(-1.0, 0.25, 0.0), d(0.0, 0.0, 1.0));
        let chest = b.pos(Chest);
        let floor = b.min_y_of(&[Pelvis, ThighR]);
        b.ik(ForearmL, v3(chest.x - 0.02 * b.k, floor + 0.03 * b.k, chest.z + 0.32 * b.k), d(0.3, 1.0, 0.2));
        b.hand(HandL, d(-0.4, -0.1, 1.0), -Vec3::Y);
        add("Side-lying, head on arm", 10.0, 30.0, b.done());
    }

    // 21. Seated forward bend, reaching for the toes.
    {
        let mut b = B::new(s);
        b.root(0.0, 0.12, 0.0).spine(d(0.0, 0.85, 0.55), d(0.0, 0.6, 0.8), d(0.0, 0.42, 0.9), Vec3::Z);
        b.head(d(0.0, 0.45, 0.9), d(0.0, -0.9, 0.45));
        b.best(-0.2, 0.2, 80, |b, t| {
            for (sx, th, ft) in [(1.0, ThighL, FootL), (-1.0, ThighR, FootR)] {
                b.limb(th, d(sx * 0.07, t, 1.0), d(sx * 0.01, t, 1.0));
                b.foot(ft, d(sx * 0.1, 1.0, 0.15), d(0.0, 0.15, -1.0));
            }
            (b.min_y_of(&[Pelvis, ThighL]) - b.min_y_of(&[FootL])).abs()
        });
        for (sx, fa, ha, ft) in [(1.0, ForearmL, HandL, FootL), (-1.0, ForearmR, HandR, FootR)] {
            let foot = b.pos(ft);
            b.ik(fa, b.off(foot, sx * 0.02, 0.07, -0.12), d(sx, 0.4, 0.0));
            b.hand(ha, d(0.0, 0.1, 1.0), d(-sx, 0.0, 0.0));
        }
        add("Seated forward bend", 55.0, 15.0, b.done());
    }

    // 22. Kneeling, leaning back on the hands, looking up.
    {
        let mut b = B::new(s);
        b.root(0.0, 0.30, 0.0).spine(d(0.0, 1.0, -0.3), d(0.0, 1.0, -0.45), d(0.0, 1.0, -0.6), Vec3::Z);
        b.head(d(0.0, 1.0, -0.15), d(0.0, 0.45, 1.0));
        for (sx, th, sh, ft) in [(1.0, ThighL, ShinL, FootL), (-1.0, ThighR, ShinR, FootR)] {
            let knee = b.off(Vec3::ZERO, sx * 0.13, 0.05, 0.38);
            let hip = b.pos(th);
            b.ik(sh, b.off(Vec3::ZERO, sx * 0.08, 0.065, -0.03), knee - hip);
            b.foot(ft, d(-sx * 0.1, -0.3, -1.0), d(0.0, -1.0, -0.3));
        }
        for (sx, fa, ha) in [(1.0, ForearmL, HandL), (-1.0, ForearmR, HandR)] {
            let sh = b.pos(fa.parent().unwrap());
            b.ik(fa, v3(sh.x + sx * 0.08 * b.k, 0.03 * b.k, sh.z - 0.2 * b.k), d(0.0, 0.0, 1.0));
            b.hand(ha, d(sx * 0.2, 0.0, -1.0), -Vec3::Y);
        }
        add("Kneeling, leaning back", 70.0, 8.0, b.done());
    }

    // 23. Crawling: on all fours, one arm reaching forward, looking ahead.
    {
        let mut b = B::new(s);
        b.root(0.0, 0.47, 0.0).spine(d(0.03, 0.12, 1.0), d(0.0, 0.18, 1.0), d(-0.05, 0.2, 1.0), d(0.1, -1.0, 0.0));
        b.head(d(0.0, 1.0, 0.55), d(0.0, -0.3, 1.0));
        let hip_l = b.pos(ThighL);
        b.ik(ShinL, v3(hip_l.x + 0.02 * b.k, 0.055 * b.k, hip_l.z - 0.36 * b.k), d(0.0, -1.0, 1.0));
        let hip_r = b.pos(ThighR);
        b.ik(ShinR, v3(hip_r.x - 0.03 * b.k, 0.055 * b.k, hip_r.z - 0.22 * b.k), d(0.0, -1.0, 1.0));
        b.foot(FootL, d(0.0, -0.2, -1.0), d(0.0, -1.0, 0.2)).foot(FootR, d(0.0, -0.1, -1.0), d(0.0, -1.0, 0.1));
        let sh_l = b.pos(UpperArmL);
        b.ik(ForearmL, v3(sh_l.x + 0.03 * b.k, 0.035 * b.k, sh_l.z + 0.22 * b.k), d(0.0, 0.5, -1.0));
        b.hand(HandL, d(0.0, 0.0, 1.0), -Vec3::Y);
        let sh_r = b.pos(UpperArmR);
        b.ik(ForearmR, v3(sh_r.x - 0.03 * b.k, 0.035 * b.k, sh_r.z - 0.02 * b.k), d(0.0, 0.0, -1.0));
        b.hand(HandR, d(0.1, 0.0, 1.0), -Vec3::Y);
        add("Crawling, reaching forward", -60.0, 12.0, b.done());
    }

    // 24. Supine twist: knees dropped to one side, arms out.
    {
        let mut b = B::new(s);
        b.root(0.0, 0.10, 0.0).orient(Pelvis, d(-1.0, 0.0, 0.0), d(0.0, 0.55, 0.85));
        b.orient(Waist, d(-1.0, 0.0, 0.0), d(0.0, 0.85, 0.5)).orient(Chest, d(-1.0, 0.0, 0.0), Vec3::Y);
        b.head(d(-1.0, 0.1, 0.0), d(0.0, 0.65, -0.75));
        b.best(-0.4, 0.4, 80, |b, t| {
            b.limb(ThighR, d(-0.25, t, 1.0), d(1.0, -0.05, 0.1));
            b.limb(ThighL, d(-0.15, t + 0.3, 1.0), d(1.0, -0.1, 0.15));
            (b.min_y_of(&[Chest, Pelvis]) - b.min_y_of(&[ThighR, ShinR])).abs()
        });
        b.foot(FootR, d(0.2, 0.0, 1.0), d(-1.0, 0.0, 0.0)).foot(FootL, d(0.2, -0.2, 1.0), d(-1.0, 0.0, 0.0));
        b.limb(UpperArmL, d(0.05, -0.05, -1.0), d(0.0, 0.0, -1.0));
        b.limb(UpperArmR, d(-0.45, -0.05, 1.0), d(-0.4, 0.0, 1.0));
        b.hand(HandL, d(0.0, 0.0, -1.0), Vec3::Y).hand(HandR, d(-0.4, 0.0, 1.0), Vec3::Y);
        add("Supine twist", 10.0, 72.0, b.done());
    }

    // 25. Reclining on both elbows, ankles crossed.
    {
        let mut b = B::new(s);
        b.root(0.0, 0.11, 0.0).orient(Pelvis, d(-1.0, 0.0, 0.0), Vec3::Y);
        // Raise the chest until the elbows (under the shoulders) reach the floor level of the hips.
        b.best(0.0, 1.2, 120, |b, t| {
            b.orient(Waist, d(-1.0, 0.35 * t, 0.0), Vec3::Y).orient(Chest, d(-0.8, 0.6 * t + 0.05, 0.0), Vec3::Y);
            for (sz, ua) in [(-1.0, UpperArmL), (1.0, UpperArmR)] {
                b.limb(ua, d(-0.3, -1.0, sz * 0.2), d(1.0, -0.02, sz * 0.12));
            }
            (b.min_y_of(&[ForearmL, UpperArmL]) - b.min_y_of(&[Pelvis])).abs()
        });
        b.head(d(-0.25, 1.0, 0.0), d(1.0, 0.2, 0.0));
        for (sz, ha) in [(-1.0, HandL), (1.0, HandR)] {
            b.hand(ha, d(1.0, -0.05, sz * 0.1), -Vec3::Y);
        }
        b.best(-0.2, 0.2, 80, |b, t| {
            b.limb(ThighL, d(1.0, t, 0.02), d(1.0, t + 0.02, 0.08));
            b.limb(ThighR, d(1.0, t + 0.03, -0.02), d(1.0, t + 0.06, -0.08));
            b.foot(FootL, d(0.35, 1.0, 0.25), d(-1.0, 0.35, 0.0)).foot(FootR, d(0.35, 1.0, -0.25), d(-1.0, 0.35, 0.0));
            (b.min_y_of(&[Pelvis]) - b.min_y_of(&[FootL, FootR, ShinL, ShinR])).abs()
        });
        add("Reclining on elbows", 25.0, 14.0, b.done());
    }

    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::body::Body;
    use crate::skeleton::{BodyType, Proportions};

    #[test]
    fn twenty_five_valid_presets() {
        for props in [
            Proportions::default(),
            Proportions { height: 1.5, head_size: 1.3, body_type: BodyType::Curvy, ..Proportions::default() },
            Proportions {
                height: 1.95,
                shoulder_width: 1.3,
                hip_width: 0.8,
                body_type: BodyType::Slim,
                ..Proportions::default()
            },
        ] {
            let s = Skeleton::new(props);
            let list = presets(&s);
            assert_eq!(list.len(), 25);
            let mut names: Vec<_> = list.iter().map(|p| p.name).collect();
            names.sort();
            names.dedup();
            assert_eq!(names.len(), 25, "names are unique");
            for p in &list {
                assert!(p.pose.is_finite(), "{} has NaNs", p.name);
                let body = Body::new(&s, &p.pose);
                // Resting on the floor: lowest point at y = 0, nothing below it.
                assert!(body.min_y().abs() < 1e-3, "{} min_y {}", p.name, body.min_y());
                for j in [Joint::HandL, Joint::HandR, Joint::FootL, Joint::FootR, Joint::Head] {
                    assert!(body.fk.tip(j).y > -0.02, "{} {:?} below the floor", p.name, j);
                    assert!(body.fk.pos(j).y > -0.02, "{} {:?} below the floor", p.name, j);
                }
                // Not floating: something touches the floor and the figure is compact.
                let (lo, hi) = body.bounds();
                assert!(hi.y < 2.4 && (hi - lo).length() < 3.0, "{} too large", p.name);
            }
        }
    }

    #[test]
    fn presets_are_distinct() {
        let s = Skeleton::new(Proportions::default());
        let list = presets(&s);
        for (i, a) in list.iter().enumerate() {
            for b in &list[i + 1..] {
                let diff: f32 =
                    a.pose.rot.iter().flatten().zip(b.pose.rot.iter().flatten()).map(|(x, y)| (x - y).abs()).sum();
                assert!(diff > 60.0, "{} and {} are too similar", a.name, b.name);
            }
        }
    }
}
