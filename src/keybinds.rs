use eframe::egui;

use crate::{
    commands::{Axis, Command, Origin, Rotation},
    puzzle_state::{Side, Twist},
};

/// What the input system gets to see when resolving bindings, beyond the raw
/// key events. Sized for the eventual unified keybinds/mousebinds system
/// (mouse buttons as keys, hovered gizmo as context — cf. HSC2's
/// 2026-01-21_keybind_brainstorming notes), so growing into that doesn't
/// re-plumb the call site.
pub struct InputContext {
    /// the layer twists apply to (the left-panel slider).
    pub layer: u8,
    /// the twist-gizmo face under the pointer, and whether its front faces
    /// the camera. unused by the placeholder table.
    #[expect(dead_code, reason = "context for the future unified mousebinds")]
    pub hovered_gizmo: Option<(Side, bool)>,
}

/// Placeholder keybind table — a hardcoded match, meant to be redone.
///
/// Keybinds are absolute: a face key always means that puzzle-space side, no
/// matter where the mouse has rotated the view. Only the Align command
/// (Space) re-bases the puzzle state onto the view, after which face keys
/// match what you see.
///
/// - `r u f l d b`: twist that face, CCW like left-click; Shift = CW.
/// - `x y z`: rotate the whole puzzle 90° about that axis (cuber sense:
///   plain `y` turns like a U twist); Shift inverts.
/// - `Space`: align — make the puzzle state agree with the view.
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
            let (side, axis) = match key {
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
                Key::R => (Some(Side::R), None),
                Key::L => (Some(Side::L), None),
                Key::U => (Some(Side::U), None),
                Key::D => (Some(Side::D), None),
                Key::F => (Some(Side::F), None),
                Key::B => (Some(Side::B), None),
                Key::X => (None, Some(Axis::X)),
                Key::Y => (None, Some(Axis::Y)),
                Key::Z => (None, Some(Axis::Z)),
                _ => continue,
            };

            if let Some(side) = side {
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
            if let Some(axis) = axis {
                let multiplicity = if modifiers.shift { -2 } else { 2 };
                commands.push(Command::Rotate {
                    rotation: Rotation::new(axis, multiplicity),
                    origin: Origin::User,
                });
            }
        }
        commands
    }
}
