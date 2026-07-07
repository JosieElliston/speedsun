use std::{sync::Arc, time::Instant};

use cgmath::{Rotation, Rotation3};
use eframe::egui::{self, mutex::Mutex};
use euc::{buffer::Buffer2d, rasterizer, Pipeline};

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
    /// the software-rendered frame, re-uploaded to the GPU each draw.
    texture: Option<egui::TextureHandle>,
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
            texture: None,
        }
    }

    // fn sticker_of_pos(&self, pos: egui::Pos2) -> Option<PieceId> {}
    // fn piece_of_pos(&self, pos: egui::Pos2) -> Option<PieceId> {}

    pub fn draw(&mut self, painter: &egui::Painter, _now: Instant) {
        let puzzle = self.puzzle.lock();

        let rect = painter.clip_rect();
        let ctx = painter.ctx();

        // Render into a buffer sized in physical pixels so the result is crisp
        // regardless of the display's scale factor.
        let ppp = ctx.pixels_per_point();
        let w = ((rect.width() * ppp).round() as usize).clamp(1, 4096);
        let h = ((rect.height() * ppp).round() as usize).clamp(1, 4096);

        // The puzzle is rendered orthographically. `sx`/`sy` map world units to
        // NDC while keeping the puzzle square in a non-square viewport (matching
        // the old `size().min_elem()` behaviour) with a margin around the edges.
        // TODO: have puzzle_scale instead
        const MARGIN: f32 = 0.9;
        let min = w.min(h) as f32;
        let sx = min * MARGIN / (2.0 * w as f32);
        let sy = min * MARGIN / (2.0 * h as f32);

        // Correct visibility needs a per-pixel depth buffer, not a per-sticker
        // depth sort: sorting can't resolve interpenetrating or cyclically
        // overlapping polygons. We rasterize every sticker triangle on the CPU
        // with `euc`, which does per-pixel depth testing, then blit the result.
        let mut vertices: Vec<Vertex> = Vec::new();
        for piece in &puzzle.pieces {
            for sticker in &piece.stickers {
                if !(self.show_internal_stickers || sticker.side.is_some()) {
                    continue;
                }
                let color = sticker
                    .side
                    .map_or(egui::Color32::GRAY, |side| side.color());
                let rgb = (
                    color.r() as f32 / 255.0,
                    color.g() as f32 / 255.0,
                    color.b() as f32 / 255.0,
                );
                let clip: Vec<[f32; 4]> = sticker
                    .verts
                    .iter()
                    .map(|&v| self.cam.proj_clip(piece.rot * v, sx, sy))
                    .collect();
                // Fan-triangulate the (convex) sticker polygon.
                for i in 1..clip.len().saturating_sub(1) {
                    vertices.push((clip[0], rgb));
                    vertices.push((clip[i], rgb));
                    vertices.push((clip[i + 1], rgb));
                }
            }
        }

        let mut color_buf = Buffer2d::new([w, h], egui::Color32::TRANSPARENT);
        let mut depth_buf = Buffer2d::new([w, h], 1.0f32);
        StickerPipeline.draw::<rasterizer::Triangles<_, rasterizer::BackfaceCullingDisabled>, _>(
            &vertices,
            &mut color_buf,
            Some(&mut depth_buf),
        );

        let image = egui::ColorImage::new([w, h], color_buf.as_ref().to_vec());
        let options = egui::TextureOptions::NEAREST;
        match &mut self.texture {
            Some(tex) => tex.set(image, options),
            None => self.texture = Some(ctx.load_texture("puzzle", image, options)),
        }
        let texture_id = self.texture.as_ref().unwrap().id();

        let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
        painter.image(texture_id, rect, uv, egui::Color32::WHITE);
    }
}

/// A clip-space position plus an (r, g, b) color carried to the fragment stage.
type Vertex = ([f32; 4], (f32, f32, f32));

/// CPU rasterization pipeline for the flat-shaded stickers. Depth testing uses
/// `euc`'s default `IfLessWrite` strategy against the depth buffer.
struct StickerPipeline;
impl Pipeline for StickerPipeline {
    type Vertex = Vertex;
    type VsOut = (f32, f32, f32);
    type Pixel = egui::Color32;

    #[inline(always)]
    fn vert(&self, (pos, rgb): &Self::Vertex) -> ([f32; 4], Self::VsOut) {
        (*pos, *rgb)
    }

    #[inline(always)]
    fn frag(&self, (r, g, b): &Self::VsOut) -> Self::Pixel {
        egui::Color32::from_rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
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

    /// Project a world-space point to clip space (`[x, y, z, w]`, orthographic so
    /// `w = 1`). `sx`/`sy` scale x/y into NDC; z is a normalized depth in `[0, 1]`
    /// where smaller means nearer, matching `euc`'s `IfLessWrite` depth test.
    fn proj_clip(&self, pos: Vec3, sx: f32, sy: f32) -> [f32; 4] {
        let pos = self.rot.rotate_vector(pos);
        // Cube corners reach |coord| ~= sqrt(3); R = 2 keeps z comfortably in [0, 1].
        const R: f32 = 2.0;
        let z = 0.5 - pos.z / (2.0 * R);
        [pos.x * sx, pos.y * sy, z, 1.0]
    }

    pub fn drag(&mut self, dv: egui::Vec2, sensitivity: f32) {
        let dx = cgmath::Deg(dv.x * sensitivity);
        let dy = cgmath::Deg(dv.y * sensitivity);
        let rot_x = Rot::from_angle_x(dy);
        let rot_y = Rot::from_angle_y(dx);
        self.rot = rot_y * rot_x * self.rot;
    }
}
