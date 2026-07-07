use std::{sync::Arc, time::Instant};

use cgmath::{Rotation, Rotation3};
use eframe::egui::{self, mutex::Mutex};
use itertools::Itertools;

use crate::puzzle_state::*;

pub struct PuzzleView {
    puzzle: Arc<Mutex<PuzzleState>>,
    pub cam: Camera,
    twist: Option<Anim<Twist>>,
    // selected_pieces: Vec<PieceId>,
    show_internal_stickers: bool,
    // /// explode pieces away from the origin.
    // piece_explode: f32,
    // /// shrink to sticker centroid.
    // sticker_scale: f32,
    // outlines,
}
impl PuzzleView {
    pub fn new(puzzle: Arc<Mutex<PuzzleState>>) -> Self {
        Self {
            puzzle,
            cam: Camera::new(),
            twist: None,
            show_internal_stickers: true,
            // piece_explode: 1.0,
            // sticker_scale: 1.0,
        }
    }

    // fn sticker_of_pos(&self, pos: egui::Pos2) -> Option<PieceId> {}
    // fn piece_of_pos(&self, pos: egui::Pos2) -> Option<PieceId> {}

    pub fn draw(&self, painter: &egui::Painter, now: Instant) {
        let puzzle = self.puzzle.lock();

        let rect = painter.clip_rect();
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
            .filter(|(sticker, _)| self.show_internal_stickers || sticker.side.is_some())
            .map(|(sticker, piece)| {
                let mut verts = Vec::new();
                let mut total_depth = 0.0;
                for &v in &sticker.verts {
                    let (p, d) = self.cam.proj(piece.rot * v);
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
}

// except i want to have both "exactly 1/2 frames, with this far along each frame"
// and "use this easing curve, properly synced with the frame rate"
struct Anim<T> {
    t: T,
    start: Instant,
}

pub struct Camera {
    rot: Rot,
}
impl Camera {
    pub fn new() -> Self {
        Self {
            rot: Rot::from_angle_x(cgmath::Deg(20.0)) * Rot::from_angle_y(cgmath::Deg(-30.0)),
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
