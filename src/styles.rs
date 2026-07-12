//! sticker styles: the style set (shared into filters via `Rc`) and the
//! styles sidebar tab that edits it.

use std::{cell::RefCell, rc::Rc};

use eframe::egui::{self, Color32};

// hovered_format: StickerFormatBuilder {
//     outline_color: Some(Color32::WHITE),
//     outline_width: Some(0.1),
//     sticker_scale: None,
//     sticker_opacity: None,
// },
// clicked_format: StickerFormatBuilder {
//     outline_color: Some(Color32::LIGHT_GRAY),
//     outline_width: Some(0.1),
//     sticker_scale: None,
//     sticker_opacity: None,
// },
// gripped_format: StickerFormatBuilder {
//     outline_color: Some(Color32::GRAY),
//     outline_width: Some(0.05),
//     sticker_scale: None,
//     sticker_opacity: None,
// },
// default_filter_format: StickerFormat {
//     outline_color: Color32::BLACK,
//     outline_width: 0.05,
//     sticker_scale: 0.7,
//     sticker_opacity: 0.5,
// },
// default_no_filter_format: StickerFormat {
//     outline_color: Color32::BLACK,
//     outline_width: 0.05,
//     sticker_scale: 1.0,
//     sticker_opacity: 1.0,
// },

#[derive(Debug, Clone, PartialEq)]
pub enum FaceColor {
    Sticker,
    Fixed(Color32),
}

#[derive(Debug, Clone)]
pub struct CompleteStyle {
    pub face_opacity: f32,
    pub face_color: FaceColor,

    pub outline_opacity: f32,
    pub outline_size: f32,
    pub outline_color: Color32,
}
impl CompleteStyle {
    /// what an unfiltered puzzle looks like.
    /// the initial value of the builtin "basic" style,
    /// which completes any fields the fallback chain leaves unset.
    pub const DEFAULT: Self = CompleteStyle {
        face_opacity: 1.0,
        face_color: FaceColor::Sticker,
        outline_opacity: 1.0,
        outline_size: 1.0,
        outline_color: Color32::BLACK,
    };

    /// the same style with every field `Some`.
    pub fn to_partial(&self) -> PartialStyle {
        PartialStyle {
            face_opacity: Some(self.face_opacity),
            face_color: Some(self.face_color.clone()),
            outline_opacity: Some(self.outline_opacity),
            outline_size: Some(self.outline_size),
            outline_color: Some(self.outline_color),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PartialStyle {
    /// lives in [0.0, 1.0].
    pub face_opacity: Option<f32>,
    pub face_color: Option<FaceColor>,

    /// lives in [0.0, 1.0],
    pub outline_opacity: Option<f32>,
    /// multiplier on the view's outline width; lives in [0.0, ???],
    pub outline_size: Option<f32>,
    pub outline_color: Option<Color32>,
}
impl PartialStyle {
    pub const NONE: Self = PartialStyle {
        face_opacity: None,
        face_color: None,
        outline_opacity: None,
        outline_size: None,
        outline_color: None,
    };

    pub fn or(&self, rhs: &Self) -> PartialStyle {
        Self {
            face_opacity: self.face_opacity.or(rhs.face_opacity),
            face_color: self.face_color.clone().or(rhs.face_color.clone()),
            outline_opacity: self.outline_opacity.or(rhs.outline_opacity),
            outline_size: self.outline_size.or(rhs.outline_size),
            outline_color: self.outline_color.or(rhs.outline_color),
        }
    }

    pub fn unwrap_or(&self, default: &CompleteStyle) -> CompleteStyle {
        CompleteStyle {
            face_opacity: self.face_opacity.unwrap_or(default.face_opacity),
            face_color: self
                .face_color
                .clone()
                .unwrap_or(default.face_color.clone()),
            outline_opacity: self.outline_opacity.unwrap_or(default.outline_opacity),
            outline_size: self.outline_size.unwrap_or(default.outline_size),
            outline_color: self.outline_color.unwrap_or(default.outline_color),
        }
    }
}

#[derive(Debug, Clone)]
pub struct BasicStyle(pub CompleteStyle);

#[derive(Debug, Clone)]
pub struct HoveredStyle(pub PartialStyle);

#[derive(Debug, Clone)]
pub struct SelectedStyle(pub PartialStyle);

#[derive(Debug, Clone)]
pub struct UserStyle {
    pub name: String,
    pub style: PartialStyle,
}

#[derive(Debug, Clone)]
pub enum FilterStyle {
    Literal(PartialStyle),
    Basic(Rc<RefCell<BasicStyle>>),
    Hovered(Rc<RefCell<HoveredStyle>>),
    Selected(Rc<RefCell<SelectedStyle>>),
    User(Rc<RefCell<UserStyle>>),
}
impl FilterStyle {
    pub fn name(&self) -> String {
        match self {
            FilterStyle::Literal(_) => "literal".to_string(),
            FilterStyle::Basic(_) => "basic".to_string(),
            FilterStyle::Hovered(_) => "hovered".to_string(),
            FilterStyle::Selected(_) => "selected".to_string(),
            FilterStyle::User(s) => s.borrow().name.clone(),
        }
    }

    pub fn style(&self) -> PartialStyle {
        match self {
            FilterStyle::Literal(s) => s.clone(),
            FilterStyle::Basic(s) => s.borrow().0.to_partial(),
            FilterStyle::Hovered(s) => s.borrow().0.clone(),
            FilterStyle::Selected(s) => s.borrow().0.clone(),
            FilterStyle::User(s) => s.borrow().style.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Styles {
    pub basic: Rc<RefCell<BasicStyle>>,
    pub hovered: Rc<RefCell<HoveredStyle>>,
    pub selected: Rc<RefCell<SelectedStyle>>,
    pub user: Vec<Rc<RefCell<UserStyle>>>,
}
impl Styles {
    pub fn get(&self, idx: &StyleIdx) -> FilterStyle {
        match idx {
            StyleIdx::Basic => FilterStyle::Basic(Rc::clone(&self.basic)),
            StyleIdx::Hovered => FilterStyle::Hovered(Rc::clone(&self.hovered)),
            StyleIdx::Selected => FilterStyle::Selected(Rc::clone(&self.selected)),
            StyleIdx::User(i) => FilterStyle::User(Rc::clone(&self.user[*i])),
        }
    }
}
impl Default for Styles {
    fn default() -> Self {
        Self {
            basic: Rc::new(RefCell::new(BasicStyle(CompleteStyle::DEFAULT))),
            hovered: Rc::new(RefCell::new(HoveredStyle(PartialStyle {
                face_opacity: None,
                face_color: None,
                outline_opacity: Some(1.0),
                outline_size: Some(5.0),
                outline_color: Some(Color32::WHITE),
            }))),
            selected: Rc::new(RefCell::new(SelectedStyle(PartialStyle {
                face_opacity: None,
                face_color: None,
                outline_opacity: Some(1.0),
                outline_size: Some(5.0),
                outline_color: Some(Color32::from_rgb(230, 230, 230)),
            }))),
            user: vec![
                Rc::new(RefCell::new(UserStyle {
                    name: "hidden".to_string(),
                    style: PartialStyle {
                        face_opacity: Some(0.05),
                        face_color: Some(FaceColor::Sticker),
                        outline_opacity: None,
                        outline_size: None,
                        outline_color: None,
                    },
                })),
                Rc::new(RefCell::new(UserStyle {
                    name: "half hidden".to_string(),
                    style: PartialStyle {
                        face_opacity: Some(0.2),
                        face_color: Some(FaceColor::Sticker),
                        outline_opacity: None,
                        outline_size: None,
                        outline_color: None,
                    },
                })),
                Rc::new(RefCell::new(UserStyle {
                    name: "invisible".to_string(),
                    style: PartialStyle {
                        face_opacity: Some(0.0),
                        face_color: None,
                        outline_opacity: None,
                        outline_size: None,
                        outline_color: None,
                    },
                })),
                Rc::new(RefCell::new(UserStyle {
                    name: "gray".to_string(),
                    style: PartialStyle {
                        face_opacity: Some(0.2),
                        face_color: Some(FaceColor::Fixed(Color32::GRAY)),
                        outline_opacity: None,
                        outline_size: None,
                        outline_color: None,
                    },
                })),
            ],
        }
    }
}

/// which style the style editor section edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StyleIdx {
    Basic,
    Hovered,
    Selected,
    User(usize),
}
impl StyleIdx {
    pub const BUILTIN: [StyleIdx; 3] = [StyleIdx::Basic, StyleIdx::Hovered, StyleIdx::Selected];
}

/// the styles component: owns the style set (filters hold `Rc` handles into
/// it) and draws the styles sidebar tab (builtin list, user list, editor for
/// the selected style).
pub struct StyleEditor {
    pub styles: Styles,
    /// the style shown in the editor section.
    selected: StyleIdx,
}
impl Default for StyleEditor {
    fn default() -> Self {
        Self {
            styles: Styles::default(),
            selected: StyleIdx::Basic,
        }
    }
}
impl StyleEditor {
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        ui.heading("styles");
        ui.separator();
        self.ui_builtin_styles(ui);
        ui.separator();
        self.ui_user_styles(ui);
        ui.separator();
        self.ui_edit_styles(ui);
    }

    fn ui_builtin_styles(&mut self, ui: &mut egui::Ui) {
        ui.strong("builtin styles");
        for style_idx in StyleIdx::BUILTIN {
            let style = self.styles.get(&style_idx);
            if no_rename_button(
                ui,
                &style.name(),
                "builtin, so unrenamable",
                self.selected == style_idx,
            ) {
                self.selected = style_idx;
            }
        }
    }

    fn ui_user_styles(&mut self, ui: &mut egui::Ui) {
        ui.strong("user styles");
        let mut remove = None;
        for (user_style_idx, style) in self.styles.user.iter().enumerate() {
            ui.push_id(("user_style", user_style_idx), |ui| {
                let mut style = style.borrow_mut();
                ui.horizontal(|ui| {
                    if rename_button(
                        ui,
                        &mut style.name,
                        self.selected == StyleIdx::User(user_style_idx),
                    ) {
                        self.selected = StyleIdx::User(user_style_idx);
                    }
                    if ui
                        .button("🗑️")
                        .on_hover_text(format!("delete {}", style.name))
                        .clicked()
                    {
                        remove = Some(user_style_idx);
                    }
                });
            });
        }
        if let Some(user_style_idx) = remove {
            self.styles.user.remove(user_style_idx);
            if let StyleIdx::User(selected_user_style_idx) = self.selected
                && selected_user_style_idx > self.styles.user.len()
            {
                self.selected = if self.styles.user.is_empty() {
                    StyleIdx::Basic
                } else {
                    StyleIdx::User(
                        self.styles
                            .user
                            .len()
                            .checked_sub(1)
                            .expect("we just checked that it's not empty"),
                    )
                };
            }
        }
        if ui
            .button("+ style")
            .on_hover_text("append a copy of the selected style")
            .clicked()
        {
            let copy = match self.selected {
                StyleIdx::Basic => UserStyle {
                    name: "basic copy".to_string(),
                    style: self.styles.basic.borrow().0.to_partial(),
                },
                StyleIdx::Hovered => UserStyle {
                    name: "hovered copy".to_string(),
                    style: self.styles.hovered.borrow().0.clone(),
                },
                StyleIdx::Selected => UserStyle {
                    name: "selected copy".to_string(),
                    style: self.styles.selected.borrow().0.clone(),
                },
                StyleIdx::User(i) => UserStyle {
                    name: format!("{} copy", self.styles.user[i].borrow().name),
                    style: self.styles.user[i].borrow().style.clone(),
                },
            };
            self.styles.user.push(Rc::new(RefCell::new(copy)));
        }
    }

    fn ui_edit_styles(&mut self, ui: &mut egui::Ui) {
        match self.selected {
            StyleIdx::Basic => {
                let mut style = self.styles.basic.borrow_mut();
                ui.strong("basic");
                ui_complete_style(ui, &mut style.0);
            }
            StyleIdx::Hovered => {
                let mut style = self.styles.hovered.borrow_mut();
                ui.strong("hovered");
                ui_partial_style(ui, &mut style.0);
            }
            StyleIdx::Selected => {
                let mut style = self.styles.selected.borrow_mut();
                ui.strong("selected");
                ui_partial_style(ui, &mut style.0);
            }
            StyleIdx::User(i) => {
                let mut style = self.styles.user[i].borrow_mut();
                ui.strong(&style.name);
                ui_partial_style(ui, &mut style.style);
            }
        }
    }
}

const RENAME_BUTTON_MIN_WIDTH: f32 = 110.0;

/// a `rename_button` for a fixed name: same look, but no renaming.
/// returns whether it was clicked.
pub fn no_rename_button(ui: &mut egui::Ui, name: &str, hover: &str, selected: bool) -> bool {
    // grow atoms on both sides center the text regardless of the surrounding
    // layout's alignment, matching how `rename_button` reads in its header.
    let mut button = egui::Button::new((egui::Atom::grow(), name, egui::Atom::grow()))
        .min_size(egui::vec2(RENAME_BUTTON_MIN_WIDTH, 0.0));
    button = button.selected(selected);
    ui.add(button).on_hover_text(hover).clicked()
}

/// a name as a wide clickable button; right click renames it inline.
/// returns whether it was left-clicked.
pub fn rename_button(ui: &mut egui::Ui, name: &mut String, selected: bool) -> bool {
    let renaming_id = ui.id().with("renaming");
    let fresh_id = renaming_id.with("fresh");
    let renaming = ui
        .data(|d| d.get_temp::<bool>(renaming_id))
        .unwrap_or(false);
    if renaming {
        let response =
            ui.add(egui::TextEdit::singleline(name).desired_width(RENAME_BUTTON_MIN_WIDTH));
        if ui.data(|d| d.get_temp::<bool>(fresh_id)).unwrap_or(false) {
            response.request_focus();
            ui.data_mut(|d| d.remove::<bool>(fresh_id));
        } else if response.lost_focus() {
            ui.data_mut(|d| d.remove::<bool>(renaming_id));
        }
        false
    } else {
        let mut button =
            egui::Button::new(name.as_str()).min_size(egui::vec2(RENAME_BUTTON_MIN_WIDTH, 0.0));
        button = button.selected(selected);
        let response = ui.add(button).on_hover_text("right click to rename");
        if response.secondary_clicked() {
            ui.data_mut(|d| d.insert_temp(renaming_id, true));
            ui.data_mut(|d| d.insert_temp(fresh_id, true));
        }
        response.clicked()
    }
}

/// one optional (checkbox-gated) field of a `PartialStyle`.
fn opt_row<T: Clone>(
    ui: &mut egui::Ui,
    label: &str,
    opt: &mut Option<T>,
    default: T,
    widget: impl FnOnce(&mut egui::Ui, &mut T),
) {
    ui.horizontal(|ui| {
        let mut on = opt.is_some();
        if ui.checkbox(&mut on, label).changed() {
            *opt = on.then(|| default.clone());
        }
        if let Some(v) = opt.as_mut() {
            widget(ui, v);
        }
    });
}

fn face_color_ui(ui: &mut egui::Ui, face_color: &mut FaceColor) {
    egui::ComboBox::from_id_salt("face_color")
        .selected_text(match face_color {
            FaceColor::Sticker => "sticker",
            FaceColor::Fixed(_) => "fixed",
        })
        .show_ui(ui, |ui| {
            let is_sticker = matches!(face_color, FaceColor::Sticker);
            if ui.selectable_label(is_sticker, "sticker").clicked() {
                *face_color = FaceColor::Sticker;
            }
            if ui.selectable_label(!is_sticker, "fixed").clicked() && is_sticker {
                *face_color = FaceColor::Fixed(Color32::GRAY);
            }
        });
    if let FaceColor::Fixed(color) = face_color {
        ui.color_edit_button_srgba(color);
    }
}

pub fn ui_partial_style(ui: &mut egui::Ui, style: &mut PartialStyle) {
    opt_row(ui, "face opacity", &mut style.face_opacity, 1.0, |ui, v| {
        ui.add(egui::Slider::new(v, 0.0..=1.0));
    });
    opt_row(
        ui,
        "face color",
        &mut style.face_color,
        FaceColor::Sticker,
        face_color_ui,
    );
    opt_row(
        ui,
        "outline opacity",
        &mut style.outline_opacity,
        1.0,
        |ui, v| {
            ui.add(egui::Slider::new(v, 0.0..=1.0));
        },
    );
    opt_row(ui, "outline size", &mut style.outline_size, 1.0, |ui, v| {
        ui.add(egui::Slider::new(v, 0.0..=4.0));
    });
    opt_row(
        ui,
        "outline color",
        &mut style.outline_color,
        Color32::BLACK,
        |ui, v| {
            ui.color_edit_button_srgba(v);
        },
    );
}

/// like `ui_partial_style`, but for a complete style: every field always set.
fn ui_complete_style(ui: &mut egui::Ui, style: &mut CompleteStyle) {
    ui.horizontal(|ui| {
        ui.label("face opacity");
        ui.add(egui::Slider::new(&mut style.face_opacity, 0.0..=1.0));
    });
    ui.horizontal(|ui| {
        ui.label("face color");
        face_color_ui(ui, &mut style.face_color);
    });
    ui.horizontal(|ui| {
        ui.label("outline opacity");
        ui.add(egui::Slider::new(&mut style.outline_opacity, 0.0..=1.0));
    });
    ui.horizontal(|ui| {
        ui.label("outline size");
        ui.add(egui::Slider::new(&mut style.outline_size, 0.0..=4.0));
    });
    ui.horizontal(|ui| {
        ui.label("outline color");
        ui.color_edit_button_srgba(&mut style.outline_color);
    });
}
