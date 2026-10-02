//! The interactive 3D viewport: mannequin drawing, joint handles, picking,
//! FK / IK dragging and camera orbit / pan / zoom.

use eframe::egui::{self, Color32, Pos2, Sense, Stroke};
use rust_pose_studio::body::{Body, handle_point};
use rust_pose_studio::posing::{aim_joint, ik_limb};
use rust_pose_studio::render::{Frame2, Framing, Style, View, draw_body};
use rust_pose_studio::skeleton::{Joint, forward};

use super::{Drag, PoseApp, paint, theme};

const HANDLE_R: f32 = 6.0;
const PICK_R: f32 = 11.0;

impl PoseApp {
    pub(super) fn viewport(&mut self, ui: &mut egui::Ui) {
        let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        self.view_size = rect.size();
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 6.0, Color32::WHITE);

        let frame = Frame2::camera(rect.left(), rect.top(), rect.width(), rect.height());
        let to_view = |p: Pos2| frame.from_px([p.x, p.y]);

        // --- interaction (uses the state from the previous frame's geometry) ---
        let body = Body::new(&self.skel, &self.pose);
        let view = View::new(&self.camera);
        let handles: Vec<(Joint, Pos2, f32)> = Joint::ALL
            .iter()
            .map(|&j| {
                let q = view.project(handle_point(&self.skel, &body.fk, j));
                let px = frame.to_px([q.x, q.y]);
                (j, Pos2::new(px[0], px[1]), q.depth)
            })
            .collect();
        let pointer = resp.hover_pos().or(resp.interact_pointer_pos());
        let handle_under = |p: Pos2| -> Option<(Joint, Pos2, f32)> {
            handles
                .iter()
                .filter(|(_, h, _)| h.distance(p) <= PICK_R)
                .min_by(|a, b| (a.1.distance(p) + a.2 * 2.0).total_cmp(&(b.1.distance(p) + b.2 * 2.0)))
                .copied()
        };

        // Picking uses the previous frame's drawing (same rectangle and camera framing).
        let empty = rust_pose_studio::render::DrawList::default();
        let cache = self.view_cache.take();
        let pick_list = cache.as_ref().map(|c| &c.list).unwrap_or(&empty);
        self.hovered = None;
        if self.drag.is_none()
            && let Some(p) = pointer
            && rect.contains(p)
        {
            let v = to_view(p);
            self.hovered = if self.show_handles { handle_under(p).map(|h| h.0) } else { None }
                .or_else(|| pick_list.pick(v[0], v[1]));
        }

        // Start drags.
        if resp.drag_started_by(egui::PointerButton::Primary) {
            // The drag is reported after the pointer moved a little: use where it was pressed.
            let start = ui.input(|i| i.pointer.press_origin()).or(resp.interact_pointer_pos()).unwrap_or(rect.center());
            if let Some((j, h, depth)) = if self.show_handles { handle_under(start) } else { None } {
                self.selected = Some(j);
                self.drag = Some(Drag::Joint { joint: j, depth, offset: h - start });
                self.current_preset = None;
            } else {
                self.drag = Some(Drag::Orbit);
            }
        } else if resp.drag_started_by(egui::PointerButton::Secondary)
            || resp.drag_started_by(egui::PointerButton::Middle)
        {
            self.drag = Some(Drag::Pan);
        }

        // Continue drags.
        if let Some(d) = self.drag {
            let delta = ui.input(|i| i.pointer.delta());
            match d {
                Drag::Orbit => {
                    self.camera.yaw = rust_pose_studio::math::wrap_deg(self.camera.yaw - delta.x * 0.4);
                    self.camera.pitch = (self.camera.pitch + delta.y * 0.4).clamp(-85.0, 89.0);
                }
                Drag::Pan => {
                    // Move the target in the view plane by the pointer delta.
                    let target = rust_pose_studio::math::Vec3::from_array(self.camera.target);
                    let depth = (target - view.eye).dot(view.fwd);
                    let a = view.unproject(0.0, 0.0, depth);
                    let dv = [delta.x / frame.scale, -delta.y / frame.scale];
                    let b = view.unproject(dv[0], dv[1], depth);
                    self.camera.target = (target - (b - a)).to_array();
                }
                Drag::Joint { joint, depth, offset } => {
                    if let Some(p) = resp.interact_pointer_pos() {
                        let v = to_view(p + offset);
                        let target = view.unproject(v[0], v[1], depth);
                        self.drag_joint(joint, target);
                    }
                }
            }
            if !ui.input(|i| i.pointer.any_down()) || resp.drag_stopped() {
                self.drag = None;
            }
        }

        // Clicks select.
        if resp.clicked()
            && let Some(p) = resp.interact_pointer_pos()
        {
            let v = to_view(p);
            self.selected = if self.show_handles { handle_under(p).map(|h| h.0) } else { None }
                .or_else(|| pick_list.pick(v[0], v[1]));
        }

        // Zoom.
        if resp.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                self.camera.distance = (self.camera.distance * (-scroll * 0.0015).exp()).clamp(0.8, 40.0);
            }
        }

        self.view_cache = cache;

        // --- drawing (fresh geometry after the edits above) ---
        let body = Body::new(&self.skel, &self.pose);
        let view = View::new(&self.camera);
        let ppp = ui.ctx().pixels_per_point();
        let key = super::ViewKey {
            pose: self.pose.clone(),
            props: self.props,
            camera: self.camera,
            rect: [rect.left(), rect.top(), rect.width(), rect.height()],
            ppp,
            floor: self.floor,
            highlight: self.selected,
        };
        if self.view_cache.as_ref().is_none_or(|c| c.key != key) {
            let style = Style { floor: self.floor, highlight: self.selected, ..Style::default() };
            let (list, _) = draw_body(&body, &view, &style, Framing::Camera, key.rect, ppp);
            let mut tex = self.view_cache.take().and_then(|c| c.tex);
            paint::mask_texture(ui.ctx(), &list, &mut tex);
            self.view_cache = Some(super::ViewCache { key, list, tex });
        }
        if let Some(c) = &self.view_cache {
            paint::draw(&painter, &c.list, &frame, rect.height(), c.tex.as_ref());
        }

        if self.show_handles {
            let fk = forward(&self.skel, &self.pose);
            // Bone lines for the selected chain make the hierarchy visible.
            if let Some(j) = self.selected {
                let a = view.project(fk.pos(j));
                let b = view.project(handle_point(&self.skel, &fk, j));
                let pa = frame.to_px([a.x, a.y]);
                let pb = frame.to_px([b.x, b.y]);
                painter
                    .line_segment([Pos2::new(pa[0], pa[1]), Pos2::new(pb[0], pb[1])], Stroke::new(2.0, theme::ACCENT));
            }
            let mut hs: Vec<(Joint, Pos2, f32)> = Joint::ALL
                .iter()
                .map(|&j| {
                    let q = view.project(handle_point(&self.skel, &fk, j));
                    let px = frame.to_px([q.x, q.y]);
                    (j, Pos2::new(px[0], px[1]), q.depth)
                })
                .collect();
            hs.sort_by(|a, b| b.2.total_cmp(&a.2));
            for (j, p, _) in hs {
                let ik = self.ik && j.is_hinge();
                let base = if j == Joint::Pelvis {
                    Color32::from_rgb(150, 60, 200)
                } else if ik {
                    theme::IK_HANDLE
                } else {
                    theme::HANDLE
                };
                let selected = self.selected == Some(j);
                let hovered = self.hovered == Some(j);
                let r = if hovered || selected { HANDLE_R + 2.0 } else { HANDLE_R };
                let fill = if selected { theme::ACCENT } else { base.gamma_multiply(if hovered { 1.0 } else { 0.8 }) };
                if ik {
                    let pts = vec![
                        p + egui::vec2(0.0, -r - 1.0),
                        p + egui::vec2(r + 1.0, 0.0),
                        p + egui::vec2(0.0, r + 1.0),
                        p + egui::vec2(-r - 1.0, 0.0),
                    ];
                    painter.add(egui::Shape::convex_polygon(pts, fill, Stroke::new(1.5, Color32::WHITE)));
                } else {
                    painter.circle(p, r, fill, Stroke::new(1.5, Color32::WHITE));
                }
            }
        }

        // Overlay: selection name and hints.
        let label = match (self.hovered, self.selected) {
            (Some(h), _) if self.drag.is_none() => {
                format!("{}{}", h.label(), if self.ik && h.is_hinge() { "  · IK" } else { "" })
            }
            (_, Some(s)) => s.label().to_string(),
            _ => String::from("Click a joint handle or body part to select it"),
        };
        painter.text(
            rect.left_top() + egui::vec2(10.0, 8.0),
            egui::Align2::LEFT_TOP,
            label,
            egui::FontId::proportional(14.0),
            Color32::from_gray(60),
        );
        painter.text(
            rect.right_top() + egui::vec2(-10.0, 8.0),
            egui::Align2::RIGHT_TOP,
            format!("{}  ·  yaw {:.0}°  pitch {:.0}°", self.pose_name, self.camera.yaw, self.camera.pitch),
            egui::FontId::proportional(13.0),
            Color32::from_gray(110),
        );

        if self.drag.is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        } else if self.hovered.is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
    }

    fn drag_joint(&mut self, joint: Joint, target: rust_pose_studio::math::Vec3) {
        match joint {
            Joint::Pelvis => {
                let fk = forward(&self.skel, &self.pose);
                self.pose.root += target - fk.pos(Joint::Pelvis);
            }
            j if self.ik && j.is_hinge() => {
                ik_limb(&self.skel, &mut self.pose, j, target, None);
            }
            j => aim_joint(&self.skel, &mut self.pose, j, target),
        }
    }
}
