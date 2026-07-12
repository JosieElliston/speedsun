use cgmath::{InnerSpace, Rotation};
use eframe::egui;

use crate::{
    commands::{Axis, Command, Origin, Rotation as PuzzleRotation},
    puzzle_state::{Rot, Side, Twist, Vec3},
};

/// What the input system gets to see when resolving bindings, beyond the raw
/// key events. Sized for the eventual unified keybinds/mousebinds system
/// (mouse buttons as keys, hovered gizmo as context — cf. HSC2's
/// 2026-01-21_keybind_brainstorming notes), so growing into that doesn't
/// re-plumb the call site.
pub struct InputContext {
    /// the axis-aligned orientation the view is at (or snapping toward);
    /// view-space face keys resolve to puzzle-space sides through this.
    pub alignment: Rot,
    /// the layer twists apply to (the left-panel slider).
    pub layer: u8,
    /// the twist-gizmo face under the pointer, and whether its front faces
    /// the camera. unused by the placeholder table.
    #[expect(dead_code, reason = "context for the future unified mousebinds")]
    pub hovered_gizmo: Option<(Side, bool)>,
}

/// Placeholder keybind table — a hardcoded match, meant to be redone.
///
/// - `r u f l d b`: twist the face that currently *looks* like that side
///   (resolved through the view alignment), CCW like left-click; Shift = CW.
/// - `x y z`: rotate the whole puzzle 90° about the axis that currently looks
///   like R/U/F; Shift inverts.
/// - `Space`: align the view to the nearest axis-aligned orientation.
/// - `Cmd+Z` / `Cmd+Shift+Z`: undo / redo.
pub struct Keybinds;
impl Keybinds {
    pub fn collect(&self, ctx: &egui::Context, input: &InputContext) -> Vec<Command> {
        let mut commands = Vec::new();
        // never twist while the user is typing in a text field.
        if ctx.egui_wants_keyboard_input() {
            return commands;
        }
        let events = ctx.input(|i| i.events.clone());
        for event in events {
            let egui::Event::Key {
                key,
                pressed: true,
                repeat: false,
                modifiers,
                ..
            } = event
            else {
                continue;
            };

            use egui::Key;
            // view-space faces/axes named by the keys; resolved to puzzle
            // space below.
            let face = |side: Side| Some(side);
            let (view_side, rotation_axis) = match key {
                Key::Z if modifiers.command => {
                    commands.push(if modifiers.shift {
                        Command::Redo
                    } else {
                        Command::Undo
                    });
                    continue;
                }
                Key::Space => {
                    commands.push(Command::Align);
                    continue;
                }
                Key::R => (face(Side::R), None),
                Key::L => (face(Side::L), None),
                Key::U => (face(Side::U), None),
                Key::D => (face(Side::D), None),
                Key::F => (face(Side::F), None),
                Key::B => (face(Side::B), None),
                Key::X => (None, Some(Side::R)),
                Key::Y => (None, Some(Side::U)),
                Key::Z => (None, Some(Side::F)),
                _ => continue,
            };

            if let Some(view_side) = view_side {
                let side = puzzle_side_toward(input.alignment, view_side.plane());
                // plain = CCW (like the `'` buttons and left-click).
                let multiplicity = if modifiers.shift { 1 } else { -1 };
                commands.push(Command::Twist {
                    twist: Twist {
                        side,
                        layer: input.layer,
                        multiplicity,
                    },
                    origin: Origin::User,
                });
            }
            if let Some(view_axis_side) = rotation_axis {
                let side = puzzle_side_toward(input.alignment, view_axis_side.plane());
                let (axis, sign) = side_axis(side);
                // 90° in the same sense as a twist of that face; Shift inverts.
                let multiplicity = sign * 2 * if modifiers.shift { -1 } else { 1 };
                commands.push(Command::Rotate {
                    rotation: PuzzleRotation::new(axis, multiplicity),
                    origin: Origin::User,
                });
            }
        }
        commands
    }
}

/// the puzzle-space side that currently points closest to the view-space
/// direction `view_dir`, given the (snapped) view orientation.
fn puzzle_side_toward(alignment: Rot, view_dir: Vec3) -> Side {
    Side::ALL
        .into_iter()
        .max_by(|a, b| {
            let da = alignment.rotate_vector(a.plane()).dot(view_dir);
            let db = alignment.rotate_vector(b.plane()).dot(view_dir);
            da.total_cmp(&db)
        })
        .expect("Side::ALL is non-empty")
}

/// a side as a signed axis.
fn side_axis(side: Side) -> (Axis, i8) {
    match side {
        Side::R => (Axis::X, 1),
        Side::L => (Axis::X, -1),
        Side::U => (Axis::Y, 1),
        Side::D => (Axis::Y, -1),
        Side::F => (Axis::Z, 1),
        Side::B => (Axis::Z, -1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn face_keys_follow_view_rotation() {
        // at the identity view, every face key maps to itself.
        let id = Rot::new(1.0, 0.0, 0.0, 0.0);
        for side in Side::ALL {
            assert_eq!(puzzle_side_toward(id, side.plane()), side);
        }
        // after a whole-puzzle y rotation (90° about +Y, the same sense as a
        // U twist: F goes to L, R comes to the front), the face that *looks*
        // front is the old R — so the F key must twist R.
        let y = PuzzleRotation::new(Axis::Y, 2).quat();
        assert_eq!(puzzle_side_toward(y, Side::F.plane()), Side::R);
        assert_eq!(puzzle_side_toward(y, Side::L.plane()), Side::F);
        assert_eq!(puzzle_side_toward(y, Side::U.plane()), Side::U);
    }
}
