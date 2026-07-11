use std::{
    collections::{HashSet, VecDeque},
    sync::Arc,
    time::{Duration, Instant},
};

use cgmath::{InnerSpace, Rotation, Rotation3};
use eframe::{
    egui::{self, mutex::Mutex},
    egui_wgpu,
};

use crate::{
    filters::{FaceColor, Filters},
    puzzle_state::*,
    render::{FrameInput, GpuRenderer, Vertex},
};

/// below this many frames per twist, animation progress is frame-indexed
/// instead of clock-based: each drawn frame advances by exactly 1/n_frames, so
/// the intermediate fractions are deterministic and dt jitter can neither skip
/// a twist's animation entirely nor change which fraction gets shown.
const FAST_MODE_MAX_FRAMES: f32 = 3.0;

/// opacity of the hovered twist gizmo face.
const GIZMO_ALPHA: f32 = 0.35;

fn ease(t: f32) -> f32 {
    // cosine interpolation
    0.5 - 0.5 * (t * std::f32::consts::PI).cos()
}

struct ActiveTwist {
    twist: Twist,
    /// indices into puzzle.pieces of the pieces the twist rotates.
    /// validated by twist_pieces() when the twist started.
    pieces: Vec<usize>,
    mode: AnimMode,
}
impl ActiveTwist {
    /// progress through the twist in [0, 1); >= 1 means finished.
    fn progress(&self, now: Instant, duration: f32) -> f32 {
        match self.mode {
            AnimMode::Frame { progress, .. } => progress,
            AnimMode::Time { start } => {
                now.saturating_duration_since(start).as_secs_f32() / duration
            }
        }
    }
}

/// the timing regime, chosen per twist when it starts and then frozen so a
/// single twist can't flicker between regimes.
#[derive(Clone, Copy)]
enum AnimMode {
    /// slow twists: dt-aware, sampled at real elapsed time. `start` may lie
    /// before "now" by carried-over time from the previous twist.
    Time { start: Instant },
    /// fast twists (~1-2 frames): each drawn frame adds 1/n_frames.
    Frame { progress: f32, n_frames: f32 },
}

/// tints the pieces that blocked a twist red, fading out.
struct BlockedFlash {
    /// indices into puzzle.pieces of the blocking pieces.
    pieces: Vec<usize>,
    start: Instant,
}
impl BlockedFlash {
    /// red tint strength in [0, 1]; 0 means expired.
    fn strength(&self, now: Instant, duration: f32) -> f32 {
        let t = now.saturating_duration_since(self.start).as_secs_f32() / duration;
        (1.0 - t).max(0.0)
    }
}

/// how a twist's progress begins: fresh from the queue, or carrying the
/// previous twist's overshoot so back-to-back twists keep a steady cadence.
enum AnimStart {
    Fresh,
    /// leftover fraction of a twist (can exceed 1 at very fast speeds).
    CarryFrames(f32),
    /// the instant the previous twist nominally finished.
    CarryTime(Instant),
}

pub struct PuzzleView {
    puzzle: Arc<Mutex<PuzzleState>>,
    /// view rotation (dragged with the mouse). in view space, not puzzle space.
    rot: Rot,
    active_twist: Option<ActiveTwist>,
    twist_queue: VecDeque<Twist>,
    blocked_flash: Option<BlockedFlash>,
    /// seconds the pieces blocking a rejected twist stay tinted red.
    blocked_flash_duration: f32,
    /// seconds per twist animation.
    twist_duration: f32,
    /// indices into puzzle.pieces of the pieces selected by shift-clicking.
    selected_pieces: HashSet<usize>,
    show_internal_stickers: bool,
    /// cull back-facing triangles. must be off to see sticker backs once
    /// sticker_shrink < 1.0 opens gaps into the pieces.
    backface_culling: bool,
    /// scale of the whole puzzle within the rect (replaces the old MARGIN).
    puzzle_scale: f32,
    /// move pieces away from the origin; 1.0 is assembled.
    piece_explode: f32,
    /// shrink each sticker toward its own centroid; 1.0 is full size.
    sticker_shrink: f32,
    /// position of the puzzle center within the rect, in [-1, 1]. 0 is centered;
    /// +1 projects the center to the right / top edge (exact under orthographic).
    horizontal_align: f32,
    vertical_align: f32,
    /// sticker outline width in physical pixels; 0 disables outlines.
    outline_width: f32,
    /// draw all gizmo faces, not just the hovered one.
    show_gizmos: bool,
    /// shrink each twist-gizmo face toward its center; 1.0 is the full face.
    gizmo_shrink: f32,
    /// when clicking the back of a gizmo face, input the twist as seen from
    /// the camera (mirrored) instead of as defined from outside the face.
    reverse_backface_twists: bool,
    /// the wgpu pipelines and render targets the frame is drawn with.
    gpu: GpuRenderer,
}
impl PuzzleView {
    pub fn new(puzzle: Arc<Mutex<PuzzleState>>, render_state: egui_wgpu::RenderState) -> Self {
        Self {
            puzzle,
            rot: Rot::from_angle_x(cgmath::Deg(20.0)) * Rot::from_angle_y(cgmath::Deg(-30.0)),
            active_twist: None,
            twist_queue: VecDeque::new(),
            blocked_flash: None,
            blocked_flash_duration: 0.4,
            twist_duration: 0.15,
            selected_pieces: HashSet::new(),
            show_internal_stickers: true,
            backface_culling: true,
            puzzle_scale: 1.0,
            piece_explode: 1.0,
            sticker_shrink: 1.0,
            horizontal_align: 0.0,
            vertical_align: 0.0,
            outline_width: 1.0,
            show_gizmos: false,
            gizmo_shrink: 1.0,
            reverse_backface_twists: true,
            gpu: GpuRenderer::new(render_state),
        }
    }

    // fn sticker_of_pos(&self, pos: egui::Pos2) -> Option<PieceId> {}
    // fn piece_of_pos(&self, pos: egui::Pos2) -> Option<PieceId> {}

    /// view controls.
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        ui.checkbox(&mut self.show_internal_stickers, "internal stickers");
        ui.checkbox(&mut self.backface_culling, "backface culling");
        ui.add(egui::Slider::new(&mut self.puzzle_scale, 0.0..=2.0).text("puzzle scale"));
        ui.add(egui::Slider::new(&mut self.piece_explode, 1.0..=2.0).text("piece explode"));
        ui.add(egui::Slider::new(&mut self.sticker_shrink, 0.0..=1.0).text("sticker shrink"));
        ui.add(egui::Slider::new(&mut self.horizontal_align, -1.0..=1.0).text("horizontal align"));
        ui.add(egui::Slider::new(&mut self.vertical_align, -1.0..=1.0).text("vertical align"));
        ui.add(egui::Slider::new(&mut self.outline_width, 0.0..=4.0).text("outline width"));
        ui.checkbox(&mut self.show_gizmos, "show all gizmos");
        ui.add(egui::Slider::new(&mut self.gizmo_shrink, 0.0..=1.0).text("gizmo shrink"));
        ui.checkbox(&mut self.reverse_backface_twists, "reverse backface twists");
        ui.add(
            egui::Slider::new(&mut self.twist_duration, 0.01..=1.0)
                .logarithmic(true)
                .text("twist duration"),
        );
        ui.add(
            egui::Slider::new(&mut self.blocked_flash_duration, 0.05..=2.0)
                .logarithmic(true)
                .text("blocked flash duration"),
        );
    }

    pub fn drag(&mut self, dv: egui::Vec2, sensitivity: f32) {
        let dx = cgmath::Deg(dv.x * sensitivity);
        let dy = cgmath::Deg(dv.y * sensitivity);
        let rot_x = Rot::from_angle_x(dy);
        let rot_y = Rot::from_angle_y(dx);
        self.rot = rot_y * rot_x * self.rot;
    }

    /// queue a twist; it animates and is applied to the puzzle when the
    /// animation finishes. blocked twists are dropped when they would start.
    pub fn push_twist(&mut self, twist: Twist) {
        self.twist_queue.push_back(twist);
    }

    /// per-frame animation tick: advance the active twist by one drawn frame,
    /// apply it to the puzzle once finished, and chain into the queue.
    fn advance_twist(&mut self, puzzle: &mut PuzzleState, now: Instant, stable_dt: f32) {
        match &mut self.active_twist {
            None => self.start_next_twist(puzzle, AnimStart::Fresh, now, stable_dt),
            Some(active) => {
                // Time mode derives progress from the clock instead.
                if let AnimMode::Frame { progress, n_frames } = &mut active.mode {
                    *progress += 1.0 / *n_frames;
                }
            }
        }

        // Apply finished twists. Loops because at very fast speeds
        // (n_frames < 1) several twists can complete in one drawn frame.
        loop {
            let Some(active) = &self.active_twist else {
                return;
            };
            let p = active.progress(now, self.twist_duration);
            if p < 1.0 {
                return;
            }
            let twist = active.twist;
            let carry = match active.mode {
                AnimMode::Frame { progress, .. } => AnimStart::CarryFrames(progress - 1.0),
                AnimMode::Time { start } => {
                    AnimStart::CarryTime(start + Duration::from_secs_f32(self.twist_duration))
                }
            };
            puzzle
                .twist(twist)
                .expect("twist was validated when its animation started");
            self.active_twist = None;
            self.start_next_twist(puzzle, carry, now, stable_dt);
        }
    }

    fn start_next_twist(
        &mut self,
        puzzle: &PuzzleState,
        start: AnimStart,
        now: Instant,
        stable_dt: f32,
    ) {
        while let Some(twist) = self.twist_queue.pop_front() {
            let pieces = match puzzle.twist_pieces(twist) {
                Ok(pieces) => pieces,
                // blocked: drop the twist and flash the blocking pieces.
                Err(e) => {
                    self.blocked_flash = Some(BlockedFlash {
                        pieces: e.blocked,
                        start: now,
                    });
                    continue;
                }
            };
            let n_frames = self.twist_duration / stable_dt.clamp(1e-4, 1.0);
            let mode = if n_frames < FAST_MODE_MAX_FRAMES {
                let progress = match start {
                    // this drawn frame is the twist's first frame.
                    AnimStart::Fresh | AnimStart::CarryTime(_) => 1.0 / n_frames,
                    // the previous twist's overshoot already includes this
                    // frame's share.
                    AnimStart::CarryFrames(carry) => carry,
                };
                AnimMode::Frame { progress, n_frames }
            } else {
                let start = match start {
                    // backdate by one frame so the first drawn frame already
                    // shows motion (matching Frame mode's 1/n_frames start);
                    // starting at rest reads as input lag.
                    AnimStart::Fresh | AnimStart::CarryFrames(_) => {
                        now - Duration::from_secs_f32(stable_dt)
                    }
                    AnimStart::CarryTime(t) => t,
                };
                AnimMode::Time { start }
            };
            self.active_twist = Some(ActiveTwist {
                twist,
                pieces,
                mode,
            });
            return;
        }
    }

    /// one face of the uncut cube, used as a twist-input gizmo. The exploded
    /// puzzle (in cubeshape) is bounded by the cube scaled to
    /// [-piece_explode, piece_explode]^3, so the whole face scales with it;
    /// gizmo_shrink then shrinks the face toward its center.
    fn gizmo_quad(&self, side: Side) -> [Vec3; 4] {
        let n = side.plane();
        let t1 = if n.x.abs() > 0.5 {
            Vec3::unit_y()
        } else {
            Vec3::unit_x()
        };
        let t2 = if n.z.abs() > 0.5 {
            Vec3::unit_y()
        } else {
            Vec3::unit_z()
        };
        let c = n * self.piece_explode;
        let s = self.piece_explode * self.gizmo_shrink;
        [
            c + (t1 + t2) * s,
            c + (-t1 + t2) * s,
            c + (-t1 - t2) * s,
            c + (t1 - t2) * s,
        ]
    }

    /// hit-test the pointer (at view-space ray coordinates `xv`, `yv`) against
    /// the gizmo faces. returns the nearest hit face and whether its front
    /// (outward) side faces the camera.
    fn gizmo_hit(&self, xv: f32, yv: f32) -> Option<(Side, bool)> {
        let mut best: Option<(Side, bool, f32)> = None;
        for side in Side::ALL {
            let vs = self.gizmo_quad(side).map(|q| self.rot.rotate_vector(q));
            // 2D point-in-convex-quad, accepting either winding.
            let mut sign = 0.0f32;
            let mut inside = true;
            for i in 0..4 {
                let a = vs[i];
                let b = vs[(i + 1) % 4];
                let cross = (b.x - a.x) * (yv - a.y) - (b.y - a.y) * (xv - a.x);
                if cross == 0.0 {
                    continue;
                }
                if sign == 0.0 {
                    sign = cross.signum();
                } else if cross.signum() != sign {
                    inside = false;
                    break;
                }
            }
            if !inside {
                continue;
            }
            let n = self.rot.rotate_vector(side.plane());
            if n.z.abs() < 1e-6 {
                // edge-on
                continue;
            }
            // view-space z where the orthographic ray meets the face plane;
            // larger z is nearer to the camera.
            let z = (n.dot(vs[0]) - n.x * xv - n.y * yv) / n.z;
            let front = n.z > 0.0;
            if best.is_none_or(|(_, _, best_z)| z > best_z) {
                best = Some((side, front, z));
            }
        }
        best.map(|(side, front, _)| (side, front))
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

    pub fn draw(
        &mut self,
        painter: &egui::Painter,
        response: &egui::Response,
        layer: u8,
        filters: &Filters,
        now: Instant,
    ) {
        // Clone the Arc so the lock guard doesn't borrow self (advance_twist
        // needs &mut self while the puzzle is locked).
        let puzzle = Arc::clone(&self.puzzle);
        let mut puzzle = puzzle.lock();

        let rect = painter.clip_rect();
        let ctx = painter.ctx();

        // Render into a buffer sized in physical pixels so the result is crisp
        // regardless of the display's scale factor.
        let ppp = ctx.pixels_per_point();
        // at least 1: euc divides by the buffer size, and egui can hand out a
        // transiently empty rect mid-resize.
        let w = ((rect.width() * ppp).round() as usize).max(1);
        let h = ((rect.height() * ppp).round() as usize).max(1);

        // The puzzle is rendered orthographically. `sx`/`sy` scale world units to
        // NDC, keeping the puzzle square in a non-square viewport (matching the
        // old `size().min_elem()` behavior).
        let min = w.min(h) as f32;
        let sx = min * self.puzzle_scale / (2.0 * w as f32);
        let sy = min * self.puzzle_scale / (2.0 * h as f32);

        // pointer position in clip space, if it's over the viewport.
        let pointer_clip: Option<(f32, f32)> = if rect.width() > 0.0 && rect.height() > 0.0 {
            response.hover_pos().map(|p| {
                (
                    2.0 * (p.x - rect.left()) / rect.width() - 1.0,
                    1.0 - 2.0 * (p.y - rect.top()) / rect.height(),
                )
            })
        } else {
            None
        };
        // shift switches the mouse from twist input to piece selection.
        let shift = ctx.input(|i| i.modifiers.shift);

        // ---- twist gizmo input ----
        let gizmo_hover: Option<(Side, bool)> = if shift {
            None
        } else {
            pointer_clip.and_then(|(u, v)| {
                if sx.abs() < 1e-9 || sy.abs() < 1e-9 {
                    return None;
                }
                // clip space -> the view-space orthographic ray (x, y).
                let xv = (u - self.horizontal_align) / sx;
                let yv = (v - self.vertical_align) / sy;
                self.gizmo_hit(xv, yv)
            })
        };
        // Handle presses before advance_twist so the twist starts this frame.
        if let Some((side, front)) = gizmo_hover {
            // twist on mouse down (not click) for a snappier feel; dragging the
            // background still rotates the view. left: CCW (like the `'`
            // buttons); right: CW.
            let (primary, secondary) = ctx.input(|i| {
                (
                    i.pointer.button_pressed(egui::PointerButton::Primary),
                    i.pointer.button_pressed(egui::PointerButton::Secondary),
                )
            });
            let multiplicity = if primary {
                Some(-1)
            } else if secondary {
                Some(1)
            } else {
                None
            };
            if let Some(mut multiplicity) = multiplicity {
                if !front && self.reverse_backface_twists {
                    multiplicity = -multiplicity;
                }
                self.push_twist(Twist {
                    side,
                    layer,
                    multiplicity,
                });
            }
        }

        let stable_dt = ctx.input(|i| i.stable_dt);
        self.advance_twist(&mut puzzle, now, stable_dt);

        if let Some(flash) = &self.blocked_flash
            && flash.strength(now, self.blocked_flash_duration) <= 0.0
        {
            self.blocked_flash = None;
        }
        // Red tint mask for the pieces that blocked a rejected twist.
        let flash: Option<(Vec<bool>, f32)> = self.blocked_flash.as_ref().map(|flash| {
            let mut mask = vec![false; puzzle.pieces.len()];
            for &i in &flash.pieces {
                mask[i] = true;
            }
            (mask, flash.strength(now, self.blocked_flash_duration))
        });

        // Partial rotation of the animating layer's pieces. The angle formula
        // must match PuzzleState::twist exactly so progress 1 converges to the
        // applied state.
        let anim: Option<(Vec<bool>, Rot)> = self.active_twist.as_ref().map(|active| {
            let p = ease(active.progress(now, self.twist_duration));
            let angle = -active.twist.multiplicity as f32 * std::f32::consts::FRAC_PI_4 * p;
            let rot = Rot::from_axis_angle(active.twist.side.plane(), cgmath::Rad(angle));
            let mut mask = vec![false; puzzle.pieces.len()];
            for &i in &active.pieces {
                mask[i] = true;
            }
            (mask, rot)
        });

        // Correct visibility needs a per-pixel depth buffer, not a per-sticker
        // depth sort: sorting can't resolve interpenetrating or cyclically
        // overlapping polygons. Every sticker triangle is rendered with wgpu
        // (depth-tested) into a texture that's then drawn as an egui image.
        let mut vertices: Vec<Vertex> = Vec::new();
        // stickers whose filter style makes them translucent; rendered as a
        // stylized-transparency overlay instead of into the base pass.
        let mut overlay_vertices: Vec<Vertex> = Vec::new();
        // pure-red copies of the blocked pieces' triangles, overlaid on top of
        // the normally-drawn scene with stylized transparency.
        let mut flash_vertices: Vec<Vertex> = Vec::new();

        // with shift held, pick the piece under the pointer (nearest clip-space
        // depth among the sticker triangles containing it) in a pre-pass, so
        // its hovered style can be applied in the normal draw order below.
        // filtered-out pieces stay pickable; picking ignores their visibility.
        let pick_pos = if shift { pointer_clip } else { None };
        let hovered_piece: Option<usize> = pick_pos.and_then(|(u, v)| {
            let mut pick_best: Option<(usize, f32)> = None;
            for (piece_idx, piece) in puzzle.pieces.iter().enumerate() {
                let piece_rot = match &anim {
                    Some((mask, anim_rot)) if mask[piece_idx] => anim_rot * piece.rot,
                    _ => piece.rot,
                };
                let explode = (piece_rot * piece.explode_dir()) * (self.piece_explode - 1.0);
                for sticker in &piece.stickers {
                    if !(self.show_internal_stickers || sticker.side.is_some()) {
                        continue;
                    }
                    let s_centroid = sticker.centroid().unwrap();
                    let clip: Vec<[f32; 4]> = sticker
                        .verts
                        .iter()
                        .map(|&vt| {
                            let scaled = s_centroid + (vt - s_centroid) * self.sticker_shrink;
                            self.project(piece_rot * scaled + explode, sx, sy)
                        })
                        .collect();
                    for i in 1..clip.len().saturating_sub(1) {
                        if let Some(z) =
                            bary_z(clip[0], clip[i], clip[i + 1], u, v, self.backface_culling)
                            && pick_best.is_none_or(|(_, best_z)| z < best_z)
                        {
                            pick_best = Some((piece_idx, z));
                        }
                    }
                }
            }
            pick_best.map(|(piece_idx, _)| piece_idx)
        });

        // shift-click toggles the hovered piece's membership in the selection;
        // shift-clicking the background clears the selection. handled before
        // rendering so the change is reflected in this frame's styles.
        if shift && response.clicked() {
            match hovered_piece {
                Some(piece_idx) => {
                    if !self.selected_pieces.remove(&piece_idx) {
                        self.selected_pieces.insert(piece_idx);
                    }
                }
                None => self.selected_pieces.clear(),
            }
        }

        for (piece_idx, piece) in puzzle.pieces.iter().enumerate() {
            let piece_rot = match &anim {
                Some((mask, anim_rot)) if mask[piece_idx] => anim_rot * piece.rot,
                _ => piece.rot,
            };
            let flashed = matches!(&flash, Some((mask, _)) if mask[piece_idx]);

            let hovered = hovered_piece == Some(piece_idx);
            let selected = self.selected_pieces.contains(&piece_idx);
            let style = filters.style_of_state(piece, hovered, selected);
            let face_a = style.face_opacity.clamp(0.0, 1.0);
            let outline_a = style.outline_opacity.clamp(0.0, 1.0);
            let outline_w = self.outline_width * style.outline_size;
            let outline_rgba = [
                style.outline_color.r() as f32 / 255.0,
                style.outline_color.g() as f32 / 255.0,
                style.outline_color.b() as f32 / 255.0,
                outline_a,
            ];
            // a face_opacity-0 piece with a visible outline draws as wireframe.
            let visible = face_a > 0.0 || (outline_a > 0.0 && outline_w > 0.0);

            // Explode moves each piece along the sum of its colored stickers'
            // face normals (rotated to the current orientation), which in
            // cubeshape keeps each side's stickers coplanar as the puzzle
            // explodes.
            let explode = (piece_rot * piece.explode_dir()) * (self.piece_explode - 1.0);

            for sticker in &piece.stickers {
                if !(self.show_internal_stickers || sticker.side.is_some()) {
                    continue;
                }
                let color = match &style.face_color {
                    FaceColor::Sticker => sticker
                        .side
                        .map_or(egui::Color32::GRAY, |side| side.color()),
                    FaceColor::Fixed(color) => *color,
                };
                let face_rgba = [
                    color.r() as f32 / 255.0,
                    color.g() as f32 / 255.0,
                    color.b() as f32 / 255.0,
                    face_a,
                ];

                // The sticker's centroid (local space) is the point sticker_shrink
                // shrinks it toward. A sticker with no vertices draws nothing.
                let s_centroid = sticker.centroid().unwrap();

                let clip: Vec<[f32; 4]> = sticker
                    .verts
                    .iter()
                    .map(|&v| {
                        let scaled = s_centroid + (v - s_centroid) * self.sticker_shrink;
                        self.project(piece_rot * scaled + explode, sx, sy)
                    })
                    .collect();

                // blocked pieces flash even where filters hide them: the flash
                // is what explains why the twist was rejected.
                if flashed {
                    const RED: Rgba = [1.0, 0.0, 0.0, 1.0];
                    const BLACK: Rgba = [0.0, 0.0, 0.0, 1.0];
                    push_fan(
                        &mut flash_vertices,
                        &clip,
                        RED,
                        BLACK,
                        self.outline_width,
                        w,
                        h,
                    );
                }
                if !visible {
                    continue;
                }

                // a fully opaque face renders opaque pixels regardless of the
                // outline's opacity (the outline blends toward the face).
                let target = if face_a >= 1.0 {
                    &mut vertices
                } else {
                    &mut overlay_vertices
                };
                push_fan(target, &clip, face_rgba, outline_rgba, outline_w, w, h);
            }
        }

        // Twist gizmos: transparent blue on top of everything, or red where
        // the twist a face inputs is currently blocked. Normally only the
        // hovered face is drawn; show_gizmos draws all of them, with the
        // unhovered ones dimmed.
        let gizmo_faces: Vec<Side> = if self.show_gizmos {
            Side::ALL.to_vec()
        } else {
            gizmo_hover.iter().map(|&(side, _)| side).collect()
        };
        let mut gizmo_vertices: Vec<Vertex> = Vec::new();
        for side in gizmo_faces {
            let blocked = puzzle
                .twist_pieces(Twist {
                    side,
                    layer,
                    multiplicity: 1,
                })
                .is_err();
            let mut rgb = if blocked {
                (1.0, 0.0, 0.0)
            } else {
                (0.3, 0.5, 1.0)
            };
            let hovered = gizmo_hover.is_some_and(|(s, _)| s == side);
            if !hovered {
                const DIM: f32 = 0.4;
                rgb = (rgb.0 * DIM, rgb.1 * DIM, rgb.2 * DIM);
            }
            let clip: Vec<[f32; 4]> = self
                .gizmo_quad(side)
                .iter()
                .map(|&v| self.project(v, sx, sy))
                .collect();
            let face = [rgb.0, rgb.1, rgb.2, 1.0];
            const BLACK: Rgba = [0.0, 0.0, 0.0, 1.0];
            push_fan(
                &mut gizmo_vertices,
                &clip,
                face,
                BLACK,
                self.outline_width,
                w,
                h,
            );
        }

        let texture_id = self.gpu.render(&FrameInput {
            size: [w as u32, h as u32],
            backface_culling: self.backface_culling,
            base: &vertices,
            translucent: &overlay_vertices,
            flash: &flash_vertices,
            flash_strength: flash.as_ref().map_or(0.0, |(_, strength)| *strength),
            gizmos: &gizmo_vertices,
            gizmo_strength: GIZMO_ALPHA,
        });

        let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
        painter.image(texture_id, rect, uv, egui::Color32::WHITE);
    }
}

/// straight-alpha [r, g, b, a], each in [0, 1].
type Rgba = [f32; 4];

/// Fan-triangulate a convex clip-space polygon into `out`, attaching edge
/// attributes for outline drawing: component k, interpolated across the
/// triangle, is the pixel-space distance to the triangle edge opposite vertex
/// k — but only where that edge is a boundary edge of the polygon. Interior
/// fan diagonals get a huge constant instead so they never darken.
fn push_fan(
    out: &mut Vec<Vertex>,
    clip: &[[f32; 4]],
    face: Rgba,
    outline: Rgba,
    outline_width: f32,
    w: usize,
    h: usize,
) {
    /// far enough that outline shading never triggers.
    const FAR: f32 = 1e6;
    // Distances must be measured in the space the fragment shader shades in;
    // the y-flip of the NDC->pixel map doesn't matter for distances.
    let px =
        |c: &[f32; 4]| egui::vec2((c[0] + 1.0) * 0.5 * w as f32, (c[1] + 1.0) * 0.5 * h as f32);
    let n = clip.len();
    for i in 1..n.saturating_sub(1) {
        let (a, b, c) = (clip[0], clip[i], clip[i + 1]);
        let (pa, pb, pc) = (px(&a), px(&b), px(&c));
        let (ab, ac, bc) = (pb - pa, pc - pa, pc - pb);
        let area2 = (ab.x * ac.y - ab.y * ac.x).abs();
        // Vertex-to-opposite-edge heights in pixels.
        let ha = area2 / bc.length().max(1e-6);
        let hb = area2 / ac.length().max(1e-6);
        let hc = area2 / ab.length().max(1e-6);
        // Triangle edge (b, c) is always a polygon boundary edge; (a, b) only
        // in the first fan triangle, (a, c) only in the last.
        let (e1a, e1b, e1c) = if i + 1 == n - 1 {
            (0.0, hb, 0.0)
        } else {
            (FAR, FAR, FAR)
        };
        let (e2a, e2b, e2c) = if i == 1 {
            (0.0, 0.0, hc)
        } else {
            (FAR, FAR, FAR)
        };
        out.push(Vertex {
            pos: a,
            face,
            outline,
            width_edges: [outline_width, ha, e1a, e2a],
        });
        out.push(Vertex {
            pos: b,
            face,
            outline,
            width_edges: [outline_width, 0.0, e1b, e2b],
        });
        out.push(Vertex {
            pos: c,
            face,
            outline,
            width_edges: [outline_width, 0.0, e1c, e2c],
        });
    }
}

/// depth of clip-space point (u, v) inside the clip-space triangle (a, b, c),
/// or None if it's outside. `cull_backface` rejects back-facing (negative
/// signed area) triangles, matching the GPU's backface culling so hidden
/// faces aren't pickable when they aren't drawn.
fn bary_z(
    a: [f32; 4],
    b: [f32; 4],
    c: [f32; 4],
    u: f32,
    v: f32,
    cull_backface: bool,
) -> Option<f32> {
    // 2x the triangle's signed area; euc culls where this is negative.
    let denom = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1]);
    if denom.abs() < 1e-12 || (cull_backface && denom < 0.0) {
        return None;
    }
    let wa = ((b[1] - c[1]) * (u - c[0]) + (c[0] - b[0]) * (v - c[1])) / denom;
    let wb = ((c[1] - a[1]) * (u - c[0]) + (a[0] - c[0]) * (v - c[1])) / denom;
    let wc = 1.0 - wa - wb;
    (wa >= 0.0 && wb >= 0.0 && wc >= 0.0).then(|| wa * a[2] + wb * b[2] + wc * c[2])
}
