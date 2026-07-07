use cgmath::{Rotation as _, Rotation3};
use eframe::egui;
use itertools::Itertools;

use crate::puzzle::*;

pub fn show(ui: &mut egui::Ui, rect: egui::Rect, camera: &Camera, puzzle: &MixupCube) {
    // let (rect, _response) = ui.take_available_space();

    let painter = ui.painter_at(rect);
    let rect_center = rect.center();
    const MARGIN: f32 = 0.9;
    let rect_half_size = rect.size().min_elem() / 4.0 * MARGIN;

    // you're supposed to have a per-pixel depth buffer.
    // sorting is a optimization to avoid overdraw,
    // but isn't correct on its own.

    let sticker_projs = puzzle
        .pieces
        .iter()
        .flat_map(|piece| piece.stickers.iter().zip(std::iter::repeat(piece)))
        .filter(|(sticker, _)| camera.show_internals || sticker.side.is_some())
        .map(|(sticker, piece)| {
            let mut verts = Vec::new();
            let mut total_depth = 0.0;
            for &v in &sticker.verts {
                let (p, d) = camera.proj(piece.rot * v);
                verts.push(rect_center + p * rect_half_size);
                total_depth += d;
            }
            let depth = total_depth / verts.len() as f32;
            (sticker, verts, depth)
        })
        .sorted_by(|(_, _, d1), (_, _, d2)| d2.partial_cmp(d1).unwrap())
        .map(|(sticker, verts, _d)| (sticker, verts))
        .collect_vec();

    for (sticker, pos) in sticker_projs {
        let color = sticker
            .side
            .map_or(egui::Color32::GRAY, |side| side.color());
        painter.add(egui::epaint::PathShape {
            points: pos.into_iter().collect(),
            closed: true,
            fill: color,
            stroke: egui::epaint::PathStroke::new(1.0, egui::Color32::BLACK),
        });
    }
}

pub struct Camera {
    rot: Rot,
    show_internals: bool,
    // /// explode pieces away from the origin
    // piece_explode: f32,
    // /// shrink to sticker centroid
    // sticker_scale: f32,
    // outlines
}
impl Camera {
    pub fn new() -> Self {
        Self {
            rot: Rot::from_angle_x(cgmath::Deg(20.0)) * Rot::from_angle_y(cgmath::Deg(-30.0)),
            show_internals: true,
        }
    }

    /// returns (egui::Pos2, depth)
    fn proj(&self, pos: Vec3) -> (egui::Vec2, f32) {
        let pos = self.rot.rotate_vector(pos);
        let depth = -pos.z;
        let pos = egui::Vec2::new(pos.x, -pos.y);
        (pos, depth)
    }

    pub fn drag(&mut self, dv: egui::Vec2, sensitivity: f32) {
        let dx = cgmath::Deg(dv.x * sensitivity);
        let dy = cgmath::Deg(dv.y * sensitivity);
        let rot_x = Rot::from_angle_x(dy);
        let rot_y = Rot::from_angle_y(dx);
        self.rot = rot_y * rot_x * self.rot;
    }
}
