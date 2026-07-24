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

use eframe::egui::{self, collapsing_header::CollapsingState};

use crate::keybinds::{self, Keybinds};

/// gap between the key rects, in points, independent of the scale.
const KEY_PADDING: f32 = 2.0;

/// What one cell of the keyboard stands for.
#[derive(Debug, Clone, Copy)]
enum Cell {
    /// a key egui reports, bound to the `key_<name>` variable.
    Key(egui::Key),
    /// any other boolean variable, with the name to show for it: the mouse
    /// buttons, and the modifier *state* variables. the modifier cells use
    /// `key_shift` rather than `key_shiftleft` because that's what guards are
    /// normally written against — holding one here should change the others.
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
const fn var(variable: &'static str, label: &'static str, width: f32) -> (Cell, f32) {
    (Cell::Var(variable, label), width)
}
const fn dead(label: &'static str, width: f32) -> (Cell, f32) {
    (Cell::Dead(label), width)
}
const fn gap(width: f32) -> (Cell, f32) {
    (Cell::Gap, width)
}

/// the super key is cmd on macOS, which is also what `key_command` reads there.
const SUPER_LABEL: &str = if cfg!(target_os = "macos") {
    "Cmd"
} else {
    "Super"
};

/// A block of the keyboard, positioned in a grid whose unit is one key. Blocks
/// are laid out in this shared space so they keep their real spacing.
struct Area {
    rect: egui::Rect,
    rows: &'static [&'static [(Cell, f32)]],
}

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
        min: egui::pos2(0.0, 1.25),
        max: egui::pos2(15.0, 6.25),
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
            var("key_shift", "Shift", 2.25),
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
            var("key_shift", "Shift", 2.75),
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

// the extra blocks sit *below* the main one rather than beside it: the scale
// comes from the available width, so anything to the right would shrink every
// key on the keyboard.
const ARROW_KEYS: Area = Area {
    rect: egui::Rect {
        min: egui::pos2(0.0, 6.5),
        max: egui::pos2(3.0, 8.5),
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

/// mousebinds are keybinds, so the mouse buttons get cells like everything
/// else. what they show depends on where the pointer really is.
const MOUSE_BUTTONS: Area = Area {
    rect: egui::Rect {
        min: egui::pos2(3.5, 6.5),
        max: egui::pos2(6.5, 7.5),
    },
    rows: &[&[
        var("mouse_left", "LMB", 1.0),
        var("mouse_middle", "MMB", 1.0),
        var("mouse_right", "RMB", 1.0),
    ]],
};

/// The reference's own settings. The bindings themselves live in
/// [`Keybinds`]; this is just how they're drawn.
pub struct KeybindReference {
    pub open: bool,
    show_key_names: bool,
    show_function_keys: bool,
    show_arrow_keys: bool,
    show_mouse_buttons: bool,
}
impl Default for KeybindReference {
    fn default() -> Self {
        Self {
            open: false,
            show_key_names: false,
            show_function_keys: false,
            show_arrow_keys: true,
            show_mouse_buttons: true,
        }
    }
}
impl KeybindReference {
    /// Drawn in the keybinds tab rather than in the reference itself: the
    /// window is for reading the keyboard, not for configuring it.
    pub fn settings_ui(&mut self, ui: &mut egui::Ui) {
        let state = CollapsingState::load_with_default_open(
            ui.ctx(),
            ui.make_persistent_id("reference_settings"),
            false,
        );
        state
            .show_header(ui, |ui| {
                ui.checkbox(&mut self.open, "keybind reference");
            })
            .body(|ui| {
                ui.checkbox(&mut self.show_key_names, "key names");
                ui.checkbox(&mut self.show_function_keys, "function keys");
                ui.checkbox(&mut self.show_arrow_keys, "arrow keys");
                ui.checkbox(&mut self.show_mouse_buttons, "mouse buttons");
            });
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, keybinds: &mut Keybinds) {
        let mut areas = vec![&MAIN_KEYS];
        if self.show_function_keys {
            areas.push(&FUNCTION_KEYS);
        }
        if self.show_arrow_keys {
            areas.push(&ARROW_KEYS);
        }
        if self.show_mouse_buttons {
            areas.push(&MOUSE_BUTTONS);
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
    }

    /// Draw one key: what it would do, in its folder's color, plus its own
    /// name in the corner if that's turned on. The name is clutter on every
    /// cell at once, so it's off by default and always in the tooltip.
    fn cell(&self, ui: &mut egui::Ui, keybinds: &mut Keybinds, cell: Cell, rect: egui::Rect) {
        let (variable, label) = match cell {
            Cell::Gap => return,
            Cell::Dead(label) => (None, label.to_string()),
            Cell::Var(variable, label) => (Some(variable.to_string()), label.to_string()),
            Cell::Key(key) => (Some(keybinds::key_variable(key)), key_label(ui, key)),
        };

        let preview = variable
            .as_deref()
            .and_then(|variable| keybinds.preview(variable, None));
        // clicked down in the reference and pressed for real look the same,
        // because to every binding they are the same.
        let down = variable
            .as_deref()
            .is_some_and(|variable| keybinds.is_down(variable));

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
        let widget = if response.hovered() {
            &visuals.widgets.hovered
        } else if variable.is_some() {
            &visuals.widgets.inactive
        } else {
            &visuals.widgets.noninteractive
        };
        // the folder's color is the key's background, which is the only way to
        // see it at a glance across a whole keyboard.
        let fill = match &preview {
            Some(preview) => preview.color,
            None => widget.bg_fill,
        };
        let text_color = contrasting_text(fill);
        let stroke = if down {
            egui::Stroke::new(2.0, DOWN_OUTLINE)
        } else {
            widget.bg_stroke
        };
        let painter = ui.painter();
        painter.rect(
            rect,
            widget.corner_radius,
            fill,
            stroke,
            egui::StrokeKind::Inside,
        );

        // the name in the corner, small and dim, when it's asked for. it
        // shrinks to the key's width rather than spilling out of it.
        let name_height = if self.show_key_names {
            let size = (rect.height() * 0.28).clamp(5.0, 11.0);
            let galley = autosized(ui, &label, egui::vec2(rect.width() - 6.0, size), size);
            let height = galley.size().y;
            ui.painter().galley(
                rect.left_top() + egui::vec2(3.0, 1.0),
                galley,
                text_color.gamma_multiply(0.6),
            );
            height
        } else {
            0.0
        };

        if let Some(preview) = &preview {
            // the name takes its bite out of the top, so the notation stays
            // centered in what's left.
            let room = rect.size() - egui::vec2(6.0, 6.0 + name_height);
            let galley = autosized(ui, &preview.label, room, room.y);
            let center = rect.center() + egui::vec2(0.0, name_height / 2.0);
            ui.painter()
                .galley(center - galley.size() / 2.0, galley, text_color);
        }

        if response.clicked()
            && let Some(variable) = &variable
        {
            keybinds.toggle(variable);
        }

        response.on_hover_ui(|ui| {
            match &variable {
                Some(variable) => {
                    ui.monospace(variable);
                }
                None => {
                    ui.monospace(&label);
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
            ui.weak("click to toggle");
        });
    }
}

/// Fit the text to the cell, like HSC1 does — a `Rw3/2'` needs more room than
/// an `x`, and it's better for them to differ in size than for the long one to
/// spill. Small is fine; there's no size at which it gives up and draws a dot
/// instead, since a tiny `Rw3/2'` still tells you more than a bullet.
///
/// Measures once and scales, rather than stepping down a point at a time: this
/// runs for every key on the keyboard, every frame. `max_size` is the size to
/// draw at when the text already fits.
fn autosized(
    ui: &egui::Ui,
    text: &str,
    room: egui::Vec2,
    max_size: f32,
) -> std::sync::Arc<egui::Galley> {
    let painter = ui.painter();
    let probe_size = max_size.max(1.0);
    let probe = painter.layout_no_wrap(
        text.to_owned(),
        egui::FontId::proportional(probe_size),
        egui::Color32::PLACEHOLDER,
    );
    let (width, height) = (probe.size().x, probe.size().y);
    if width <= 0.0 || height <= 0.0 {
        return probe;
    }
    let scale = (room.x / width).min(room.y / height).min(1.0);
    if scale >= 1.0 {
        return probe;
    }
    painter.layout_no_wrap(
        text.to_owned(),
        egui::FontId::proportional((probe_size * scale).max(1.0)),
        egui::Color32::PLACEHOLDER,
    )
}

/// What to call a key on its cell: a symbol where there is one, and a short
/// word otherwise — a key is one square, so `Escape` has to be `Esc`.
///
/// egui's bundled fonts don't have every symbol worth wanting (no `⌫`, `⏎` or
/// `⇧`) and a missing glyph draws as nothing at all, so each symbol carries
/// the word to use when the font can't render it.
fn key_label(ui: &egui::Ui, key: egui::Key) -> String {
    let (symbol, word) = match key {
        egui::Key::Backspace => ("⌫", "Bksp"),
        egui::Key::Enter => ("⏎", "Enter"),
        egui::Key::Tab => ("⇥", "Tab"),
        egui::Key::Escape => ("Esc", "Esc"),
        egui::Key::Quote => ("'", "'"),
        // egui's own symbols (the arrows, the punctuation) are ones it draws
        // itself, so they're safe; its names are the fallback.
        _ => (key.symbol_or_name(), key.name()),
    };
    let font = egui::TextStyle::Body.resolve(ui.style());
    if ui.fonts_mut(|fonts| fonts.has_glyphs(&font, symbol)) {
        symbol.to_string()
    } else {
        word.to_string()
    }
}

/// Outline on a key that's down. White reads against every folder color;
/// matching it to the text color made a black outline on a light key look like
/// the cell had merely shrunk.
const DOWN_OUTLINE: egui::Color32 = egui::Color32::WHITE;

/// Black or white, whichever is readable on this background. Uses the WCAG
/// relative luminance and its crossover point, where the contrast ratio
/// against white equals the one against black.
fn contrasting_text(background: egui::Color32) -> egui::Color32 {
    fn linear(srgb: u8) -> f32 {
        let c = srgb as f32 / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }
    let luminance = 0.2126 * linear(background.r())
        + 0.7152 * linear(background.g())
        + 0.0722 * linear(background.b());
    // sqrt(1.05 * 0.05) - 0.05
    const CROSSOVER: f32 = 0.1791;
    if luminance > CROSSOVER {
        egui::Color32::BLACK
    } else {
        egui::Color32::WHITE
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
        keybinds.toggle("key_shift");
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

    /// a key is one square, so its name has to be an abbreviation. long ones
    /// now shrink to fit rather than spilling, but shrinking `Backspace` into
    /// a key is not the same as labeling it.
    #[test]
    fn key_names_are_short() {
        let ctx = egui::Context::default();
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            for area in [&FUNCTION_KEYS, &MAIN_KEYS, &MOUSE_BUTTONS, &ARROW_KEYS] {
                for row in area.rows {
                    for (cell, _) in *row {
                        let label = match cell {
                            Cell::Key(key) => key_label(ui, *key),
                            Cell::Var(_, label) | Cell::Dead(label) => label.to_string(),
                            Cell::Gap => continue,
                        };
                        assert!(label.chars().count() <= 5, "{cell:?} is labeled {label:?}");
                    }
                }
            }
        });
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
                        Cell::Var(variable, _) => variable.to_string(),
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
