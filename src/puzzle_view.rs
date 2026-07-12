use std::{collections::HashSet, time::Instant};

use cgmath::{InnerSpace, Rotation, Rotation3};
use eframe::{egui, egui_wgpu};

use crate::{
    commands::{Command, Origin},
    filters::Filters,
    puzzle_state::*,
    render::{FrameInput, GpuRenderer, Vertex},
    simulation::{PuzzleSimulation, ease},
    styles::{FaceColor, Styles},
};

/// opacity of the hovered twist gizmo face.
const GIZMO_ALPHA: f32 = 0.35;

/// what the pointer is over this frame, computed by `interact` and consumed
/// by `draw` (and by the hub as keybind context).
pub struct Hover {
    /// the gizmo face under the pointer, and whether its front (outward)
    /// side faces the camera.
    pub gizmo: Option<(Side, bool)>,
    /// the piece under the pointer while shift is held (pick mode).
    pub piece: Option<usize>,
}

/// an in-flight view snap (an Align command's residual heading to identity),
/// slerped over the twist duration.
struct SnapAnim {
    from: Rot,
    to: Rot,
    start: Instant,
}

/// The view: camera orientation, view settings, per-view selection, and the
/// renderer. Reads the simulation; never writes it (twists and selection
/// changes are emitted as commands for the hub to route).
pub struct PuzzleView {
    /// view rotation (dragged with the mouse). in view space, not puzzle space.
    rot: Rot,
    /// animated snap of `rot` toward an Align/Rotate target.
    snap: Option<SnapAnim>,
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
    /// seconds per twist animation (also paces view snaps).
    pub twist_duration: f32,
    /// seconds the pieces blocking a rejected twist stay tinted red.
    pub blocked_flash_duration: f32,
    /// the wgpu pipelines and render targets the frame is drawn with.
    gpu: GpuRenderer,
}
impl PuzzleView {
    pub fn new(render_state: egui_wgpu::RenderState) -> Self {
        Self {
            rot: Rot::from_angle_x(cgmath::Deg(20.0)) * Rot::from_angle_y(cgmath::Deg(-30.0)),
            snap: None,
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
            twist_duration: 0.15,
            blocked_flash_duration: 0.4,
            gpu: GpuRenderer::new(render_state),
        }
    }

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
        if dv == egui::Vec2::ZERO {
            return;
        }
        // dragging cancels a snap; `rot` already holds the last drawn
        // interpolation, so the view continues from where it looks.
        self.snap = None;
        let dx = cgmath::Deg(dv.x * sensitivity);
        let dy = cgmath::Deg(dv.y * sensitivity);
        let rot_x = Rot::from_angle_x(dy);
        let rot_y = Rot::from_angle_y(dx);
        self.rot = rot_y * rot_x * self.rot;
    }

    /// advance the snap animation; `rot` is the drawn orientation afterward.
    fn tick_camera(&mut self, now: Instant) {
        if let Some(snap) = &self.snap {
            let t = now.saturating_duration_since(snap.start).as_secs_f32()
                / self.twist_duration.max(1e-4);
            if t >= 1.0 {
                self.rot = snap.to;
                self.snap = None;
            } else {
                self.rot = snap.from.slerp(snap.to, ease(t));
            }
        }
    }

    fn snap_to(&mut self, to: Rot, now: Instant) {
        let from = self.rot;
        let mut to = to.normalize();
        // quaternion double cover: pick the representative on `from`'s
        // hemisphere so slerp takes the short way.
        if from.dot(to) < 0.0 {
            to = -to;
        }
        self.snap = Some(SnapAnim {
            from,
            to,
            start: now,
        });
    }

    /// The view half of the Align command: strip the axis-aligned part of
    /// the view orientation (returned, for the simulation to bake into the
    /// puzzle state), keep only the sub-90° residual, and snap that to
    /// identity. Visually net-zero at this instant; only the residual
    /// animates away.
    pub fn align(&mut self, now: Instant) -> Rot {
        let a = nearest_alignment(self.rot);
        self.rot = self.rot * a.invert();
        self.snap_to(Rot::new(1.0, 0.0, 0.0, 0.0), now);
        a
    }

    /// compose `rot` (puzzle-space) onto the view without touching the
    /// puzzle state: the cosmetic compensation that keeps undoing/redoing an
    /// Align visually net-zero.
    pub fn rotate_view(&mut self, rot: Rot) {
        self.rot = self.rot * rot;
        if let Some(snap) = &mut self.snap {
            snap.from = snap.from * rot;
            snap.to = snap.to * rot;
        }
    }

    pub fn toggle_selection(&mut self, piece_idx: usize) {
        if !self.selected_pieces.remove(&piece_idx) {
            self.selected_pieces.insert(piece_idx);
        }
    }

    pub fn clear_selection(&mut self) {
        self.selected_pieces.clear();
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

    /// physical-pixel buffer size and world-to-NDC scales for a viewport
    /// rect. `sx`/`sy` keep the puzzle square in a non-square viewport.
    fn viewport(&self, rect: egui::Rect, ppp: f32) -> (usize, usize, f32, f32) {
        // at least 1: egui can hand out a transiently empty rect mid-resize.
        let w = ((rect.width() * ppp).round() as usize).max(1);
        let h = ((rect.height() * ppp).round() as usize).max(1);
        let min = w.min(h) as f32;
        let sx = min * self.puzzle_scale / (2.0 * w as f32);
        let sy = min * self.puzzle_scale / (2.0 * h as f32);
        (w, h, sx, sy)
    }

    /// Interpret pointer input: gizmo hover/clicks (twist input) and
    /// shift-hover/clicks (piece picking and selection). Emits commands
    /// instead of mutating; the returned `Hover` feeds `draw` and the
    /// keybinds' input context. Also advances the camera snap animation.
    pub fn interact(
        &mut self,
        sim: &PuzzleSimulation,
        response: &egui::Response,
        layer: u8,
        now: Instant,
    ) -> (Vec<Command>, Hover) {
        self.tick_camera(now);
        let mut commands = Vec::new();

        let ctx = &response.ctx;
        let rect = response.rect;
        let (_, _, sx, sy) = self.viewport(rect, ctx.pixels_per_point());

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
        // The hub routes these to the simulation before its tick, so the
        // twist still starts on the frame of the press.
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
                commands.push(Command::Twist {
                    twist: Twist {
                        side,
                        layer,
                        multiplicity,
                    },
                    origin: Origin::User,
                });
            }
        }

        // with shift held, pick the piece under the pointer (nearest clip-space
        // depth among the sticker triangles containing it), so its hovered
        // style can be applied when drawing. filtered-out pieces stay
        // pickable; picking ignores their visibility.
        let anim = sim.anim(now, self.twist_duration);
        let pick_pos = if shift { pointer_clip } else { None };
        let hovered_piece: Option<usize> = pick_pos.and_then(|(u, v)| {
            let mut pick_best: Option<(usize, f32)> = None;
            for (piece_idx, piece) in sim.puzzle().pieces.iter().enumerate() {
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
        // shift-clicking the background clears the selection. the hub routes
        // these back to this view before draw, so the change is reflected in
        // this frame's styles.
        if shift && response.clicked() {
            commands.push(match hovered_piece {
                Some(piece_idx) => Command::TogglePieceSelection(piece_idx),
                None => Command::ClearSelection,
            });
        }

        (
            commands,
            Hover {
                gizmo: gizmo_hover,
                piece: hovered_piece,
            },
        )
    }

    /// Render the puzzle: read-only against the simulation, filters, and
    /// styles.
    #[expect(
        clippy::too_many_arguments,
        reason = "the main puzzle render is the deliberate many-component reader"
    )]
    pub fn draw(
        &mut self,
        sim: &PuzzleSimulation,
        filters: &Filters,
        styles: &Styles,
        hover: &Hover,
        layer: u8,
        painter: &egui::Painter,
        now: Instant,
    ) {
        let rect = painter.clip_rect();
        let ctx = painter.ctx();

        // Render into a buffer sized in physical pixels so the result is crisp
        // regardless of the display's scale factor.
        let (w, h, sx, sy) = self.viewport(rect, ctx.pixels_per_point());

        // Red tint mask for the pieces that blocked a rejected twist.
        let flash = sim.flash(now, self.blocked_flash_duration);
        // Partial rotation of the animating layer's pieces.
        let anim = sim.anim(now, self.twist_duration);

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

        for (piece_idx, piece) in sim.puzzle().pieces.iter().enumerate() {
            let piece_rot = match &anim {
                Some((mask, anim_rot)) if mask[piece_idx] => anim_rot * piece.rot,
                _ => piece.rot,
            };
            let flashed = matches!(&flash, Some((mask, _)) if mask[piece_idx]);

            let hovered = hover.piece == Some(piece_idx);
            let selected = self.selected_pieces.contains(&piece_idx);
            let style = filters.style_of_state(styles, piece, hovered, selected);
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
            hover.gizmo.iter().map(|&(side, _)| side).collect()
        };
        let mut gizmo_vertices: Vec<Vertex> = Vec::new();
        for side in gizmo_faces {
            let blocked = sim
                .puzzle()
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
            let hovered = hover.gizmo.is_some_and(|(s, _)| s == side);
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

/// the axis-aligned orientation (one of the cube's 24 rotations) nearest to
/// `rot`, on `rot`'s hemisphere of the quaternion double cover.
fn nearest_alignment(rot: Rot) -> Rot {
    let mut best: Option<(Rot, f32)> = None;
    // Rx^i * Ry^j * Rz^k over 90° steps covers all 24 orientations.
    for i in 0..4 {
        for j in 0..4 {
            for k in 0..4 {
                let q = Rot::from_angle_x(cgmath::Deg(90.0 * i as f32))
                    * Rot::from_angle_y(cgmath::Deg(90.0 * j as f32))
                    * Rot::from_angle_z(cgmath::Deg(90.0 * k as f32));
                let d = rot.dot(q);
                let (q, d) = if d < 0.0 { (-q, -d) } else { (q, d) };
                if best.is_none_or(|(_, best_d)| d > best_d) {
                    best = Some((q, d));
                }
            }
        }
    }
    best.expect("the loop always runs").0
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

#[cfg(test)]
mod tests {
    use super::*;

    /// every 90°-step Euler product, i.e. the candidates nearest_alignment
    /// searches.
    fn euler_grid() -> impl Iterator<Item = Rot> {
        (0..4).flat_map(|i| {
            (0..4).flat_map(move |j| {
                (0..4).map(move |k| {
                    Rot::from_angle_x(cgmath::Deg(90.0 * i as f32))
                        * Rot::from_angle_y(cgmath::Deg(90.0 * j as f32))
                        * Rot::from_angle_z(cgmath::Deg(90.0 * k as f32))
                })
            })
        })
    }

    #[test]
    fn euler_grid_covers_the_24_cube_orientations() {
        let mut distinct: Vec<Rot> = Vec::new();
        for q in euler_grid() {
            if !distinct.iter().any(|p| p.dot(q).abs() > 1.0 - 1e-4) {
                distinct.push(q);
            }
        }
        assert_eq!(distinct.len(), 24);
    }

    #[test]
    fn nearest_alignment_snaps_small_wiggles_back() {
        // a small view-space wiggle on top of any axis-aligned orientation
        // must snap back to that orientation.
        let wiggle = Rot::from_angle_x(cgmath::Deg(9.0)) * Rot::from_angle_y(cgmath::Deg(-7.0));
        for q in euler_grid() {
            let snapped = nearest_alignment(wiggle * q);
            assert!(
                snapped.dot(q).abs() > 1.0 - 1e-4,
                "wiggled {q:?} snapped to {snapped:?}"
            );
        }
    }
}
