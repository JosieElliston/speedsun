use std::{sync::Arc, time::Instant};

use cgmath::{Rotation, Rotation3};
use eframe::egui::{self, mutex::Mutex};
use euc::{Pipeline, buffer::Buffer2d, rasterizer};

use crate::puzzle_state::*;

pub struct PuzzleView {
    puzzle: Arc<Mutex<PuzzleState>>,
    /// view rotation (dragged with the mouse). in view space, not puzzle space.
    rot: Rot,
    twist: Option<Anim<Twist>>,
    // selected_pieces: Vec<PieceId>,
    show_internal_stickers: bool,
    /// cull back-facing triangles. must be off to see sticker backs once
    /// sticker_scale < 1.0 opens gaps into the pieces.
    backface_culling: bool,
    /// scale of the whole puzzle within the rect (replaces the old MARGIN).
    puzzle_scale: f32,
    /// move pieces away from the origin; 1.0 is assembled.
    piece_explode: f32,
    /// shrink each sticker toward its own centroid; 1.0 is full size.
    sticker_scale: f32,
    /// position of the puzzle center within the rect, in [-1, 1]. 0 is centered;
    /// +1 projects the center to the right / top edge (exact under orthographic).
    horizontal_align: f32,
    vertical_align: f32,
    /// the software-rendered frame, re-uploaded to the GPU each draw.
    texture: Option<egui::TextureHandle>,
}
impl PuzzleView {
    pub fn new(puzzle: Arc<Mutex<PuzzleState>>) -> Self {
        Self {
            puzzle,
            rot: Rot::from_angle_x(cgmath::Deg(20.0)) * Rot::from_angle_y(cgmath::Deg(-30.0)),
            twist: None,
            show_internal_stickers: true,
            backface_culling: true,
            puzzle_scale: 1.0,
            piece_explode: 1.0,
            sticker_scale: 1.0,
            horizontal_align: 0.0,
            vertical_align: 0.0,
            texture: None,
        }
    }

    // fn sticker_of_pos(&self, pos: egui::Pos2) -> Option<PieceId> {}
    // fn piece_of_pos(&self, pos: egui::Pos2) -> Option<PieceId> {}

    /// view controls.
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        ui.checkbox(&mut self.show_internal_stickers, "internal stickers");
        ui.checkbox(&mut self.backface_culling, "backface culling");
        ui.add(egui::Slider::new(&mut self.puzzle_scale, 0.0..=2.0).text("puzzle scale"));
        ui.add(egui::Slider::new(&mut self.piece_explode, 1.0..=3.0).text("piece explode"));
        ui.add(egui::Slider::new(&mut self.sticker_scale, 0.0..=1.0).text("sticker scale"));
        ui.add(egui::Slider::new(&mut self.horizontal_align, -1.0..=1.0).text("horizontal align"));
        ui.add(egui::Slider::new(&mut self.vertical_align, -1.0..=1.0).text("vertical align"));
    }

    pub fn drag(&mut self, dv: egui::Vec2, sensitivity: f32) {
        let dx = cgmath::Deg(dv.x * sensitivity);
        let dy = cgmath::Deg(dv.y * sensitivity);
        let rot_x = Rot::from_angle_x(dy);
        let rot_y = Rot::from_angle_y(dx);
        self.rot = rot_y * rot_x * self.rot;
    }

    /// Project a puzzle-space point to clip space (`[x, y, z, w]`, orthographic so
    /// `w = 1`): apply the view rotation and alignment, and map z to a normalized
    /// depth where smaller means nearer, matching `euc`'s `IfLessWrite` test.
    fn project(&self, puzzle_pos: Vec3, sx: f32, sy: f32) -> [f32; 4] {
        let pos = self.rot.rotate_vector(puzzle_pos);
        // Widen the depth range with piece_explode so exploded-out pieces (which
        // reach ~explode*sqrt(3) from the origin) stay inside the depth buffer.
        let r = 3.0 * self.piece_explode.max(1.0);
        let z = 0.5 - pos.z / (2.0 * r);
        [
            pos.x * sx + self.horizontal_align,
            pos.y * sy + self.vertical_align,
            z,
            1.0,
        ]
    }

    pub fn draw(&mut self, painter: &egui::Painter, _now: Instant) {
        let puzzle = self.puzzle.lock();

        let rect = painter.clip_rect();
        let ctx = painter.ctx();

        // Render into a buffer sized in physical pixels so the result is crisp
        // regardless of the display's scale factor.
        let ppp = ctx.pixels_per_point();
        let w = ((rect.width() * ppp).round() as usize).clamp(1, 4096);
        let h = ((rect.height() * ppp).round() as usize).clamp(1, 4096);

        // The puzzle is rendered orthographically. `sx`/`sy` scale world units to
        // NDC, keeping the puzzle square in a non-square viewport (matching the
        // old `size().min_elem()` behavior).
        let min = w.min(h) as f32;
        let sx = min * self.puzzle_scale / (2.0 * w as f32);
        let sy = min * self.puzzle_scale / (2.0 * h as f32);

        // Correct visibility needs a per-pixel depth buffer, not a per-sticker
        // depth sort: sorting can't resolve interpenetrating or cyclically
        // overlapping polygons. We rasterize every sticker triangle on the CPU
        // with `euc`, which does per-pixel depth testing, then blit the result.
        let mut vertices: Vec<Vertex> = Vec::new();
        for piece in &puzzle.pieces {
            // The piece's centroid (puzzle space, before the view rotation) is
            // the direction it explodes along.
            let mut sum = Vec3::new(0.0, 0.0, 0.0);
            let mut count = 0.0;
            for sticker in &piece.stickers {
                for &v in &sticker.verts {
                    sum += v;
                    count += 1.0;
                }
            }
            let piece_centroid = if count > 0.0 {
                piece.rot * (sum / count)
            } else {
                Vec3::new(0.0, 0.0, 0.0)
            };
            let explode = piece_centroid * (self.piece_explode - 1.0);

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

                // The sticker's centroid (local space) is the point sticker_scale
                // shrinks it toward.
                let ssum = sticker
                    .verts
                    .iter()
                    .fold(Vec3::new(0.0, 0.0, 0.0), |acc, &v| acc + v);
                let s_centroid = ssum / sticker.verts.len() as f32;

                let clip: Vec<[f32; 4]> = sticker
                    .verts
                    .iter()
                    .map(|&v| {
                        let scaled = s_centroid + (v - s_centroid) * self.sticker_scale;
                        self.project(piece.rot * scaled + explode, sx, sy)
                    })
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
        // Backface culling is a compile-time type param in euc, so branch over the
        // two monomorphizations. All stickers are wound counterclockwise as seen
        // from outside their piece, so back faces are the ones hidden inside.
        if self.backface_culling {
            StickerPipeline.draw::<rasterizer::Triangles<_, rasterizer::BackfaceCullingEnabled>, _>(
                &vertices,
                &mut color_buf,
                Some(&mut depth_buf),
            );
        } else {
            StickerPipeline
                .draw::<rasterizer::Triangles<_, rasterizer::BackfaceCullingDisabled>, _>(
                    &vertices,
                    &mut color_buf,
                    Some(&mut depth_buf),
                );
        }

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
