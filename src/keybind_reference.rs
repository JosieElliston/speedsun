//! The keybind reference: a picture of the keyboard showing what every key
//! would do right now, and a way to press keys with the mouse.
//!
//! Modeled on HSC1's `keybinds_reference.rs` — a unit grid of keys scaled to
//! the space available, each cell shrinking its text until it fits, so cells
//! legitimately end up with different text sizes. Every cell asks
//! [`Keybinds::preview`] what its variable would do, which means the reference
//! can't drift from what the bindings actually say.
//!
//! `ui` takes a plain `Ui` and doesn't care what contains it: today the hub
//! puts it in a floating window, and it could move into a dock or the sidebar
//! without changing anything here.

use eframe::egui;

use crate::keybinds::Keybinds;

/// gap between the key rects, in points, independent of the scale.
const KEY_PADDING: f32 = 2.0;
/// the smallest font a cell's action text shrinks to before giving up.
const MIN_FONT_SIZE: f32 = 5.0;

/// What one cell of the keyboard stands for.
#[derive(Clone, Copy)]
enum Cell {
    /// a key egui reports, bound to the `key_<name>` variable.
    Key(egui::Key),
    /// any other boolean variable, with the label to draw: the mouse buttons,
    /// and the modifier *state* variables. the modifier cells use `key_shift`
    /// rather than `key_shiftleft` because that's what guards are normally
    /// written against — holding one here should change the other cells.
    Var(&'static str, &'static str),
    /// a key egui never reports, drawn dead so the keyboard still looks right.
    Dead(&'static str),
    Gap,
}

const fn k(key: egui::Key) -> (Cell, f32) {
    (Cell::Key(key), 1.0)
}
const fn kw(key: egui::Key, width: f32) -> (Cell, f32) {
    (Cell::Key(key), width)
}
const fn var(name: &'static str, label: &'static str, width: f32) -> (Cell, f32) {
    (Cell::Var(name, label), width)
}
const fn dead(label: &'static str, width: f32) -> (Cell, f32) {
    (Cell::Dead(label), width)
}
const fn gap(width: f32) -> (Cell, f32) {
    (Cell::Gap, width)
}

/// A block of the keyboard, positioned in a grid whose unit is one key. Blocks
/// are laid out in this shared space so they keep their real spacing.
struct Area {
    rect: egui::Rect,
    rows: &'static [&'static [(Cell, f32)]],
}

/// the super key is cmd on macOS, where it's also what `key_command` reads.
const SUPER_LABEL: &str = if cfg!(target_os = "macos") {
    "⌘"
} else {
    "Super"
};

const FUNCTION_KEYS: Area = Area {
    rect: egui::Rect {
        min: egui::pos2(0.0, 0.0),
        max: egui::pos2(15.0, 1.0),
    },
    rows: &[&[
        k(egui::Key::Escape),
        gap(1.0),
        k(egui::Key::F1),
        k(egui::Key::F2),
        k(egui::Key::F3),
        k(egui::Key::F4),
        gap(0.5),
        k(egui::Key::F5),
        k(egui::Key::F6),
        k(egui::Key::F7),
        k(egui::Key::F8),
        gap(0.5),
        k(egui::Key::F9),
        k(egui::Key::F10),
        k(egui::Key::F11),
        k(egui::Key::F12),
    ]],
};

const MAIN_KEYS: Area = Area {
    rect: egui::Rect {
        min: egui::pos2(0.0, 1.5),
        max: egui::pos2(15.0, 6.5),
    },
    rows: &[
        &[
            k(egui::Key::Backtick),
            k(egui::Key::Num1),
            k(egui::Key::Num2),
            k(egui::Key::Num3),
            k(egui::Key::Num4),
            k(egui::Key::Num5),
            k(egui::Key::Num6),
            k(egui::Key::Num7),
            k(egui::Key::Num8),
            k(egui::Key::Num9),
            k(egui::Key::Num0),
            k(egui::Key::Minus),
            k(egui::Key::Equals),
            kw(egui::Key::Backspace, 2.0),
        ],
        &[
            kw(egui::Key::Tab, 1.5),
            k(egui::Key::Q),
            k(egui::Key::W),
            k(egui::Key::E),
            k(egui::Key::R),
            k(egui::Key::T),
            k(egui::Key::Y),
            k(egui::Key::U),
            k(egui::Key::I),
            k(egui::Key::O),
            k(egui::Key::P),
            k(egui::Key::OpenBracket),
            k(egui::Key::CloseBracket),
            kw(egui::Key::Backslash, 1.5),
        ],
        &[
            dead("Caps", 1.75),
            k(egui::Key::A),
            k(egui::Key::S),
            k(egui::Key::D),
            k(egui::Key::F),
            k(egui::Key::G),
            k(egui::Key::H),
            k(egui::Key::J),
            k(egui::Key::K),
            k(egui::Key::L),
            k(egui::Key::Semicolon),
            k(egui::Key::Quote),
            kw(egui::Key::Enter, 2.25),
        ],
        &[
            var("key_shift", "⇧", 2.25),
            k(egui::Key::Z),
            k(egui::Key::X),
            k(egui::Key::C),
            k(egui::Key::V),
            k(egui::Key::B),
            k(egui::Key::N),
            k(egui::Key::M),
            k(egui::Key::Comma),
            k(egui::Key::Period),
            k(egui::Key::Slash),
            var("key_shift", "⇧", 2.75),
        ],
        &[
            var("key_ctrl", "Ctrl", 1.25),
            var("key_command", SUPER_LABEL, 1.25),
            var("key_alt", "Alt", 1.25),
            kw(egui::Key::Space, 6.25),
            var("key_alt", "Alt", 1.25),
            var("key_command", SUPER_LABEL, 1.25),
            dead("Menu", 1.25),
            var("key_ctrl", "Ctrl", 1.25),
        ],
    ],
};

/// mousebinds are keybinds, so the mouse buttons get cells like everything
/// else. what they show depends on where the pointer really is.
const MOUSE_BUTTONS: Area = Area {
    rect: egui::Rect {
        min: egui::pos2(15.25, 1.5),
        max: egui::pos2(18.25, 2.5),
    },
    rows: &[&[
        var("mouse_left", "LMB", 1.0),
        var("mouse_middle", "MMB", 1.0),
        var("mouse_right", "RMB", 1.0),
    ]],
};

const ARROW_KEYS: Area = Area {
    rect: egui::Rect {
        min: egui::pos2(15.25, 4.5),
        max: egui::pos2(18.25, 6.5),
    },
    rows: &[
        &[gap(1.0), k(egui::Key::ArrowUp)],
        &[
            k(egui::Key::ArrowLeft),
            k(egui::Key::ArrowDown),
            k(egui::Key::ArrowRight),
        ],
    ],
};

/// The reference's own settings. The bindings themselves live in
/// [`Keybinds`]; this is just how they're drawn.
pub struct KeybindReference {
    pub open: bool,
    show_function_keys: bool,
    /// multiplier on the button text size that a cell's text shrinks down from.
    max_font_size: f32,
}
impl Default for KeybindReference {
    fn default() -> Self {
        Self {
            open: false,
            show_function_keys: false,
            max_font_size: 1.4,
        }
    }
}
impl KeybindReference {
    pub fn ui(&mut self, ui: &mut egui::Ui, keybinds: &mut Keybinds) {
        let mut areas = vec![&MAIN_KEYS, &MOUSE_BUTTONS, &ARROW_KEYS];
        if self.show_function_keys {
            areas.push(&FUNCTION_KEYS);
        }
        let Some(total) = areas.iter().map(|area| area.rect).reduce(egui::Rect::union) else {
            return;
        };

        // one key is a whole button tall at minimum, and an integer scale
        // keeps the rows from drifting apart by a fraction of a pixel.
        let min_scale = ui.spacing().interact_size.y + KEY_PADDING * 2.0;
        let scale = (ui.available_width() / total.width())
            .max(min_scale)
            .round();
        let (_id, rect) = ui.allocate_space(total.size() * scale);
        let origin = rect.min - total.min.to_vec2() * scale;

        for area in areas {
            let mut cursor = area.rect.min.to_vec2() * scale;
            for row in area.rows {
                for &(cell, width) in *row {
                    let size = egui::vec2(width, 1.0) * scale;
                    let cell_rect =
                        egui::Rect::from_min_size(origin + cursor, size).shrink(KEY_PADDING);
                    self.cell(ui, keybinds, cell, cell_rect);
                    cursor.x += size.x;
                }
                cursor.x = area.rect.left() * scale;
                cursor.y += scale;
            }
        }

        ui.collapsing("settings", |ui| {
            ui.checkbox(&mut self.show_function_keys, "function keys");
            ui.add(egui::Slider::new(&mut self.max_font_size, 0.5..=3.0).text("max text size"));
        });
    }

    /// draw one key: its name in the corner, what it would do in the middle.
    fn cell(&self, ui: &mut egui::Ui, keybinds: &mut Keybinds, cell: Cell, rect: egui::Rect) {
        let (variable, label) = match cell {
            Cell::Gap => return,
            Cell::Dead(label) => (None, label.to_string()),
            Cell::Var(name, label) => (Some(name.to_string()), label.to_string()),
            Cell::Key(key) => (
                Some(format!("key_{}", format!("{key:?}").to_lowercase())),
                key_label(key),
            ),
        };

        let preview = variable
            .as_deref()
            .and_then(|variable| keybinds.preview(variable, None));
        let held = variable
            .as_deref()
            .is_some_and(|variable| keybinds.is_held(variable));

        // the position is part of the id because a variable can appear on more
        // than one key (both shifts).
        let id = ui.id().with((
            "cell",
            &variable,
            rect.min.x.to_bits(),
            rect.min.y.to_bits(),
        ));
        let sense = if variable.is_some() {
            egui::Sense::click()
        } else {
            egui::Sense::hover()
        };
        let response = ui.interact(rect, id, sense);

        let visuals = ui.visuals();
        let widget = if held {
            &visuals.widgets.active
        } else if response.hovered() {
            &visuals.widgets.hovered
        } else if variable.is_some() {
            &visuals.widgets.inactive
        } else {
            &visuals.widgets.noninteractive
        };
        let fill = if held {
            // a key the reference is holding down, as opposed to one the mouse
            // happens to be over.
            egui::Color32::DARK_GREEN
        } else {
            widget.bg_fill
        };
        let corner = widget.corner_radius;
        let stroke = widget.bg_stroke;
        let painter = ui.painter();
        painter.rect(rect, corner, fill, stroke, egui::StrokeKind::Inside);

        // the key's own name, small and dim in the corner: the action is what
        // you read, the name is how you find the key.
        let name_font = egui::FontId::proportional((rect.height() * 0.3).clamp(5.0, 11.0));
        painter.text(
            rect.left_top() + egui::vec2(2.0, 1.0),
            egui::Align2::LEFT_TOP,
            &label,
            name_font,
            visuals.weak_text_color(),
        );

        if let Some(preview) = &preview {
            let room = rect.size() - egui::vec2(4.0, 4.0);
            let galley = autosized(ui, &preview.label, room, self.max_font_size);
            let pos = rect.center() - galley.size() / 2.0 + egui::vec2(0.0, rect.height() * 0.1);
            ui.painter().galley(pos, galley, preview.color);
        }

        if response.clicked()
            && let Some(variable) = &variable
        {
            keybinds.toggle_held(variable);
        }

        response.on_hover_ui(|ui| {
            ui.heading(&label);
            match &variable {
                Some(variable) => {
                    ui.monospace(variable);
                }
                None => {
                    ui.weak("egui doesn't report this key");
                    return;
                }
            }
            match &preview {
                Some(preview) => {
                    ui.colored_label(preview.color, &preview.location);
                    match &preview.error {
                        Some(error) => {
                            ui.colored_label(ui.visuals().error_fg_color, error);
                        }
                        None => {
                            ui.strong(&preview.label);
                        }
                    }
                }
                None => {
                    ui.weak("unbound");
                }
            }
            ui.weak(if held {
                "click to release"
            } else {
                "click to hold down"
            });
        });
    }
}

/// Shrink the text until it fits the cell, like HSC1 does — a `Rw3/2'` needs
/// more room than an `x`, and it's better for them to differ in size than for
/// the long one to spill.
fn autosized(
    ui: &egui::Ui,
    text: &str,
    room: egui::Vec2,
    max_font_size: f32,
) -> std::sync::Arc<egui::Galley> {
    let mut size = (egui::TextStyle::Button.resolve(ui.style()).size * max_font_size).round();
    loop {
        let font = egui::FontId::proportional(size);
        let galley = ui
            .painter()
            .layout_no_wrap(text.to_owned(), font, egui::Color32::PLACEHOLDER);
        if size <= MIN_FONT_SIZE || (galley.size().x <= room.x && galley.size().y <= room.y) {
            return galley;
        }
        size -= 1.0;
    }
}

fn key_label(key: egui::Key) -> String {
    match key {
        egui::Key::Backspace => "⌫".to_string(),
        egui::Key::Enter => "⏎".to_string(),
        egui::Key::Escape => "Esc".to_string(),
        egui::Key::Quote => "'".to_string(),
        _ => key.symbol_or_name().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// a pass per key per frame, all of it painted by hand: run it headlessly
    /// so a panic in the layout can't wait for someone to open the window.
    #[test]
    fn the_reference_draws() {
        let mut reference = KeybindReference {
            open: true,
            show_function_keys: true,
            ..KeybindReference::default()
        };
        let mut keybinds = Keybinds::default();
        keybinds.toggle_held("key_shift");
        let ctx = egui::Context::default();
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            reference.ui(ui, &mut keybinds);
        });
    }

    /// the layout is a hand-typed table of key widths, and a keyboard row that
    /// doesn't add up puts every key after it in the wrong place.
    #[test]
    fn every_row_fits_its_block() {
        for area in [&FUNCTION_KEYS, &MAIN_KEYS, &MOUSE_BUTTONS, &ARROW_KEYS] {
            for row in area.rows {
                let width: f32 = row.iter().map(|(_, width)| width).sum();
                assert!(
                    width <= area.rect.width() + 1e-4,
                    "a row is {width} wide in a block {} wide",
                    area.rect.width()
                );
            }
            assert!(area.rows.len() as f32 <= area.rect.height() + 1e-4);
        }
        // the main block is a real keyboard: every row is exactly full.
        for row in MAIN_KEYS.rows {
            let width: f32 = row.iter().map(|(_, width)| width).sum();
            assert!(
                (width - MAIN_KEYS.rect.width()).abs() < 1e-4,
                "row is {width} wide, not {}",
                MAIN_KEYS.rect.width()
            );
        }
    }

    /// every key cell names a variable the keybind system can actually
    /// resolve; a typo here would silently show an empty key forever.
    #[test]
    fn every_cell_names_a_real_variable() {
        for area in [&FUNCTION_KEYS, &MAIN_KEYS, &MOUSE_BUTTONS, &ARROW_KEYS] {
            for row in area.rows {
                for (cell, _) in *row {
                    let variable = match cell {
                        Cell::Key(key) => format!("key_{}", format!("{key:?}").to_lowercase()),
                        Cell::Var(name, _) => name.to_string(),
                        Cell::Dead(_) | Cell::Gap => continue,
                    };
                    // unbound is fine; unknown is not.
                    assert!(
                        crate::keybinds::is_builtin(&variable),
                        "no builtin variable named `{variable}`"
                    );
                }
            }
        }
    }
}
