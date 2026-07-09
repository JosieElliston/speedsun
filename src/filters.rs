use std::{cell::RefCell, collections::HashSet, rc::Rc};

use eframe::egui::{self, Color32, collapsing_header::CollapsingState};

use crate::puzzle_state::{Piece, Side};

pub use style::*;
mod style {
    use super::*;

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
                    outline_size: Some(2.0),
                    outline_color: Some(Color32::WHITE),
                }))),
                selected: Rc::new(RefCell::new(SelectedStyle(PartialStyle {
                    face_opacity: None,
                    face_color: None,
                    outline_opacity: Some(1.0),
                    outline_size: Some(2.0),
                    outline_color: Some(Color32::LIGHT_GRAY),
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
}

// ref [milo's hsc filter tool](https://milojacquet.com/hscfilter)
// Each line should have one filter of the form `filter-name: filter-expression`. The filter expression can have the following syntax:
// 1, 2, 3, 4: pieces with 1, 2, 3, or 4 stickers
//     there isn't a difference between 1R and R1
//     but there is a difference between 1!R and !1R
// R, O, Y, G, B, V, N, W: pieces with Red, Orange, Yellow, Green, Blue, Violet, piNk, or White stickers
// XZ: the intersection of X and Z
// !X: the complement of X
// !XZ: the intersection of the complements of X and Z (arbitrary arity)
// X+Z: The union of X and Z
// (X): grouping
// ^: the filter from the previous line

// TODO: piece types
// TODO: bit sets
#[derive(Debug, Clone, Default)]
pub struct PieceSetTerm {
    pub must_have: HashSet<Side>,
    pub cant_have: HashSet<Side>,
}
impl PieceSetTerm {
    fn contains(&self, piece: &Piece) -> bool {
        let has = |side: &Side| piece.stickers.iter().any(|s| s.side == Some(*side));
        self.must_have.iter().all(has) && !self.cant_have.iter().any(has)
    }

    /// milo-syntax-ish summary, e.g. "RU!FL".
    fn label(&self) -> String {
        let mut must_have = String::new();
        for side in Side::ALL {
            if self.must_have.contains(&side) {
                must_have.push_str(&format!("{side:?}"));
            }
        }
        let mut cant_have = String::new();
        for side in Side::ALL {
            if self.cant_have.contains(&side) {
                cant_have.push_str(&format!("{side:?}"));
            }
        }
        match (must_have.is_empty(), cant_have.is_empty()) {
            (true, true) => "all".to_string(),
            (false, true) => must_have,
            (true, false) => format!("!{cant_have}"),
            (false, false) => format!("{must_have}!{cant_have}"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PieceSet {
    terms: Vec<PieceSetTerm>,
}
impl PieceSet {
    fn contains(&self, piece: &Piece) -> bool {
        self.terms.iter().any(|term| term.contains(piece))
    }

    fn label(&self) -> String {
        if self.terms.is_empty() {
            return "none".to_string();
        }
        self.terms
            .iter()
            .map(PieceSetTerm::label)
            .collect::<Vec<_>>()
            .join(" + ")
    }
}

#[derive(Debug, Clone)]
struct StageTerm {
    set: PieceSet,
    style: FilterStyle,
}
impl StageTerm {
    fn new(style: FilterStyle) -> Self {
        Self {
            set: PieceSet {
                terms: vec![PieceSetTerm::default()],
            },
            style,
        }
    }

    fn get(&self, piece: &Piece) -> Option<PartialStyle> {
        if self.set.contains(piece) {
            Some(self.style.style())
        } else {
            None
        }
    }
}

// TODO: better names
#[derive(Debug, Clone)]
enum IncludePrev {
    /// use the fallbacks.
    Dont,
    /// use the result from the prev stage.
    /// ie if the prev stage matched this piece with some stile, use that style,
    /// and if it didn't match this piece, use the fallbacks.
    Prev,
    /// if the previous stage matched this piece
    /// (and didn't use the fallbacks), use this style
    /// (otherwise use the fallbacks).
    Fixed(FilterStyle),
}

/// each piece scans through `Stage` and takes the first style it matches.
///
/// first the piece checks `sets`.
///
/// then if the piece matches the previous stage and
/// `inherits_prev` is `StyleInherits::Inherits`,
/// the piece takes on the style it was given by the previous stage.
///
/// if the piece matches the previous stage and
/// `inherits_prev` is `StyleInherits::Style(style)`,
/// the piece takes on `style`.
///
/// if the piece didn't get matched, it takes the fallback style.
///
/// note that we consider there to be a stage of "everything gets the fallback style" before the first stage,
/// so if this is the first stage, the piece takes on the fallback style.
#[derive(Debug, Clone)]
struct Stage {
    name: String,
    terms: Vec<StageTerm>,
    include_prev: IncludePrev,
    /// fields the terms leave unset fall through to this,
    /// then to the sequence fallback.
    fallback: FilterStyle,
}
impl Stage {
    fn new(name: String, default_style: FilterStyle) -> Self {
        Self {
            name,
            terms: vec![StageTerm::new(default_style.clone())],
            include_prev: IncludePrev::Dont,
            fallback: default_style,
        }
    }
}

#[derive(Debug, Clone)]
struct Sequence {
    name: String,
    /// fields the stage fallback leaves unset fall through to this.
    fallback: FilterStyle,
    stages: Vec<Stage>,
}
impl Sequence {
    fn new(name: String, default_style: FilterStyle) -> Self {
        Self {
            name,
            fallback: default_style.clone(),
            stages: vec![Stage::new("stage 0".to_string(), default_style.clone())],
        }
    }

    /// `None` if it never got matched.
    /// note that `Some(PartialStyle::NONE)` can occur
    /// if there's a `StageTerm` that assigns that style.
    fn get_no_fallback(&self, piece: &Piece, stage_idx: usize) -> Option<PartialStyle> {
        let stage = &self.stages[stage_idx];
        let mut ret = None;

        for term in &stage.terms {
            if let Some(style) = term.get(piece) {
                // earlier terms win per-field: terms read top-down like
                // match arms, and later terms only fill what earlier
                // matching terms left unset.
                ret = Some(ret.unwrap_or(PartialStyle::NONE).or(&style));
            }
        }

        if stage_idx > 0 {
            match &stage.include_prev {
                IncludePrev::Dont => (),
                IncludePrev::Prev => {
                    if let Some(style) = self.get_no_fallback(piece, stage_idx - 1) {
                        ret = Some(ret.unwrap_or(PartialStyle::NONE).or(&style));
                    }
                }
                IncludePrev::Fixed(style) => {
                    if self.get_no_fallback(piece, stage_idx - 1).is_some() {
                        ret = Some(ret.unwrap_or(PartialStyle::NONE).or(&style.style()));
                    }
                }
            }
        }

        ret
    }
}

/// which stage the puzzle is displayed with
/// (and which stage the stage editor section edits).
/// with `None`, the entire puzzle is shown with the builtin basic style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SelectedSeqStage {
    None,
    Some { seq: usize, stage: usize },
}

#[derive(Debug, Clone)]
pub struct Filters {
    styles: Styles,
    /// the style shown in the style editor section.
    selected_style_idx: StyleIdx,
    sequences: Vec<Sequence>,
    selected_seq_stage: SelectedSeqStage,
}
impl Filters {
    // TODO: bake the prev stage search into the stage to make this constant time
    // (rather than linear) (in the number of stages) (it'll still be linear in the number of terms).
    pub fn style_of(&self, piece: &Piece) -> CompleteStyle {
        let SelectedSeqStage::Some {
            seq: seq_idx,
            stage: stage_idx,
        } = self.selected_seq_stage
        else {
            // no stage selected: the entire puzzle gets the basic style.
            return self.styles.basic.borrow().0.clone();
        };
        let seq = &self.sequences[seq_idx];
        let stage = &seq.stages[stage_idx];
        // unmatched pieces (and fields a matched style leaves unset) fall
        // through the stage fallback, then the sequence fallback, then the
        // builtin "basic" style.
        let fallback = stage.fallback.style().or(&seq.fallback.style());
        match seq.get_no_fallback(piece, stage_idx) {
            None => fallback,
            Some(style) => style.or(&fallback),
        }
        .unwrap_or(&self.styles.basic.borrow().0)
    }

    fn selected_stage(&self) -> Option<&Stage> {
        match self.selected_seq_stage {
            SelectedSeqStage::None => None,
            SelectedSeqStage::Some { seq, stage } => Some(&self.sequences[seq].stages[stage]),
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        ui.heading("filters");
        ui.separator();
        self.ui_sequences(ui);
        ui.separator();
        self.ui_stage(ui);
        ui.separator();
        self.ui_builtin_styles(ui);
        ui.separator();
        self.ui_user_styles(ui);
        ui.separator();
        self.ui_edit_styles(ui);
    }

    /// section 1: the sequences (collapsible, containing their stages), by name.
    fn ui_sequences(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.strong("sequences");
        });

        if no_rename_button(
            ui,
            "none",
            "no filtering: show the entire puzzle with the basic style",
            self.selected_seq_stage == SelectedSeqStage::None,
        ) {
            self.selected_seq_stage = SelectedSeqStage::None;
        }

        let n_seqs = self.sequences.len();
        // let mut select_seq_stage: Option<(usize, usize)> = None;
        let mut remove_seq = None;
        // let mut remove_stage: Option<(usize, usize)> = None;
        let mut append_stage: Option<usize> = None;
        for (seq_idx, seq) in self.sequences.iter_mut().enumerate() {
            ui.push_id(("sequence", seq_idx), |ui| {
                let state = CollapsingState::load_with_default_open(
                    ui.ctx(),
                    ui.make_persistent_id("sequence"),
                    true,
                );
                let header = state.show_header(ui, |ui| {
                    let seq_selected = matches!(
                        self.selected_seq_stage,
                        SelectedSeqStage::Some { seq, .. } if seq == seq_idx
                    );
                    if name_button(ui, &mut seq.name, seq_selected) {
                        self.selected_seq_stage = SelectedSeqStage::Some {
                            seq: seq_idx,
                            stage: 0,
                        };
                    }
                    if n_seqs > 1
                        && ui
                            .button("🗑️")
                            .on_hover_text(format!("delete {}", seq.name))
                            .clicked()
                    {
                        remove_seq = Some(seq_idx);
                    }
                });
                header.body(|ui| {
                    let mut remove_stage: Option<usize> = None;
                    let n_stages = seq.stages.len();
                    for (stage_idx, stage) in seq.stages.iter_mut().enumerate() {
                        ui.push_id(("stage", stage_idx), |ui| {
                            ui.horizontal(|ui| {
                                let stage_selected = self.selected_seq_stage
                                    == SelectedSeqStage::Some {
                                        seq: seq_idx,
                                        stage: stage_idx,
                                    };
                                if name_button(ui, &mut stage.name, stage_selected) {
                                    self.selected_seq_stage = SelectedSeqStage::Some {
                                        seq: seq_idx,
                                        stage: stage_idx,
                                    };
                                }
                                if n_stages > 1
                                    && ui
                                        .button("🗑️")
                                        .on_hover_text(format!("delete {}", stage.name))
                                        .clicked()
                                {
                                    remove_stage = Some(stage_idx);
                                }
                            });
                        });
                    }
                    if ui
                        .button("+ stage")
                        .on_hover_text("append a copy of the selected stage to this sequence")
                        .clicked()
                    {
                        // let copy = self.selected_stage().clone();
                        // self.sequences[seq_idx].stages.push(copy);
                        append_stage = Some(seq_idx);
                    }
                    // if let Some((seq_idx, stage_idx)) = select_stage {
                    //     self.stage_idx = stage_idx;
                    //     self.sequence_idx = seq_idx;
                    // }
                    if let Some(stage_idx) = remove_stage {
                        seq.stages.remove(stage_idx);
                        if let SelectedSeqStage::Some {
                            seq: sel_seq,
                            stage: sel_stage,
                        } = &mut self.selected_seq_stage
                            && *sel_seq == seq_idx
                        {
                            *sel_stage = (*sel_stage).min(
                                seq.stages
                                    .len()
                                    .checked_sub(1)
                                    .expect("we can't handle empty sequences"),
                            );
                        }
                    }
                });
            });
        }
        if ui
            .button("+ sequence")
            .on_hover_text("append a new sequence")
            .clicked()
        {
            self.sequences.push(Sequence::new(
                format!("sequence {}", self.sequences.len()),
                FilterStyle::Basic(Rc::clone(&self.styles.basic)),
            ));
        }
        if let Some(seq_idx) = append_stage {
            let stage = match self.selected_stage() {
                Some(stage) => {
                    let mut copy = stage.clone();
                    copy.name = format!("{} copy", copy.name);
                    copy
                }
                // nothing selected to copy: append a fresh stage.
                None => Stage::new(
                    format!("stage {}", self.sequences[seq_idx].stages.len()),
                    FilterStyle::Basic(Rc::clone(&self.styles.basic)),
                ),
            };
            self.sequences[seq_idx].stages.push(stage);
        }
        if let Some(seq_idx) = remove_seq {
            self.sequences.remove(seq_idx);
            match &mut self.selected_seq_stage {
                SelectedSeqStage::None => (),
                SelectedSeqStage::Some { seq, .. } => {
                    if *seq == seq_idx {
                        // the selected sequence is gone.
                        self.selected_seq_stage = SelectedSeqStage::None;
                    } else if *seq > seq_idx {
                        *seq -= 1;
                    }
                }
            }
        }
    }

    /// section 2: the contents of the selected stage,
    /// in processing order: terms, prev stage, stage fallback, sequence fallback.
    fn ui_stage(&mut self, ui: &mut egui::Ui) {
        let styles = self.styles.clone();

        let SelectedSeqStage::Some {
            seq: seq_idx,
            stage: stage_idx,
        } = self.selected_seq_stage
        else {
            ui.strong("no stage selected");
            ui.weak("the entire puzzle is shown with the basic style");
            return;
        };

        ui.strong(format!(
            "{} / {}",
            self.sequences[seq_idx].name, self.sequences[seq_idx].stages[stage_idx].name
        ));

        let stage = &mut self.sequences[seq_idx].stages[stage_idx];

        let mut remove_term = None;
        for (term_idx, term) in stage.terms.iter_mut().enumerate() {
            ui.push_id(("term", term_idx), |ui| {
                let state = CollapsingState::load_with_default_open(
                    ui.ctx(),
                    ui.make_persistent_id("term"),
                    true,
                );
                let header = state.show_header(ui, |ui| {
                    ui.label(term.set.label());
                    if ui.button("🗑️").clicked() {
                        remove_term = Some(term_idx);
                    }
                });
                header.body(|ui| {
                    let n_rows = term.set.terms.len();
                    let mut remove_row = None;
                    for (row_idx, set_term) in term.set.terms.iter_mut().enumerate() {
                        ui.push_id(("set_row", row_idx), |ui| {
                            ui.horizontal(|ui| {
                                for side in Side::ALL {
                                    ui_piece_set_term_side(ui, side, set_term);
                                }
                                if n_rows > 1 && ui.button("🗑️").clicked() {
                                    remove_row = Some(row_idx);
                                }
                            });
                        });
                    }
                    if let Some(row_idx) = remove_row {
                        term.set.terms.remove(row_idx);
                    }
                    if ui.button("+ piece set term").clicked() {
                        term.set.terms.push(PieceSetTerm::default());
                    }
                    // ui.label("show 68 pieces with");
                    ui_filter_style(ui, "term_style", "style", true, &mut term.style, &styles);
                });
            });
        }
        if let Some(term_idx) = remove_term {
            stage.terms.remove(term_idx);
        }
        if ui.button("+ stage term").clicked() {
            stage
                .terms
                .push(StageTerm::new(FilterStyle::Basic(Rc::clone(
                    &self.styles.basic,
                ))));
        }

        // meaningful from the second stage on; kept visible (disabled) on the
        // first stage so it's discoverable.
        ui.add_enabled_ui(stage_idx > 0, |ui| {
            ui.horizontal(|ui| {
                ui.label("prev stage's pieces get");
                let label = match &stage.include_prev {
                    IncludePrev::Dont => "the stage fallback",
                    IncludePrev::Prev => "their prev style",
                    IncludePrev::Fixed(_) => "a fixed style",
                };
                egui::ComboBox::from_id_salt("include_prev")
                    .selected_text(label)
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(
                                matches!(stage.include_prev, IncludePrev::Dont),
                                "the stage fallback",
                            )
                            .clicked()
                        {
                            stage.include_prev = IncludePrev::Dont;
                        }
                        if ui
                            .selectable_label(
                                matches!(stage.include_prev, IncludePrev::Prev),
                                "their prev style",
                            )
                            .clicked()
                        {
                            stage.include_prev = IncludePrev::Prev;
                        }
                        if ui
                            .selectable_label(
                                matches!(stage.include_prev, IncludePrev::Fixed(_)),
                                "a fixed style",
                            )
                            .clicked()
                            && !matches!(stage.include_prev, IncludePrev::Fixed(_))
                        {
                            stage.include_prev =
                                IncludePrev::Fixed(FilterStyle::Literal(PartialStyle::NONE));
                        }
                    });
            });
            if let IncludePrev::Fixed(style) = &mut stage.include_prev {
                // indented: the style belongs to the dropdown's choice.
                ui.indent("include_prev_style", |ui| {
                    ui_filter_style(ui, "include_prev_style", "style", true, style, &self.styles);
                });
            }
        });

        ui_filter_style(
            ui,
            "stage_fallback",
            "stage fallback",
            false,
            &mut stage.fallback,
            &styles,
        );
        // sequence-level, not stage-level.
        ui.separator();
        ui_filter_style(
            ui,
            "sequence_fallback",
            "sequence fallback",
            false,
            &mut self.sequences[seq_idx].fallback,
            &styles,
        );
    }

    fn ui_builtin_styles(&mut self, ui: &mut egui::Ui) {
        ui.strong("builtin styles");
        for style_idx in StyleIdx::BUILTIN {
            let style = self.styles.get(&style_idx);
            if no_rename_button(
                ui,
                &style.name(),
                "builtin, so unrenamable",
                self.selected_style_idx == style_idx,
            ) {
                self.selected_style_idx = style_idx;
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
                    if name_button(
                        ui,
                        &mut style.name,
                        self.selected_style_idx == StyleIdx::User(user_style_idx),
                    ) {
                        self.selected_style_idx = StyleIdx::User(user_style_idx);
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
            if let StyleIdx::User(selected_user_style_idx) = self.selected_style_idx
                && selected_user_style_idx > self.styles.user.len()
            {
                self.selected_style_idx = if self.styles.user.is_empty() {
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
            let copy = match self.selected_style_idx {
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
        match self.selected_style_idx {
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
impl Default for Filters {
    fn default() -> Self {
        let styles = Styles::default();
        let sequences = vec![Sequence::new(
            "sequence 0".to_string(),
            FilterStyle::Basic(Rc::clone(&styles.basic)),
        )];
        Self {
            styles,
            selected_style_idx: StyleIdx::Basic,
            sequences,
            selected_seq_stage: SelectedSeqStage::None,
        }
    }
}

/// a `name_button` for a fixed name: same look, but no renaming.
/// returns whether it was clicked.
// TODO: this should be centered like `name_button`.
fn no_rename_button(ui: &mut egui::Ui, name: &str, hover: &str, selected: bool) -> bool {
    let mut button = egui::Button::new(name).min_size(egui::vec2(110.0, 0.0));
    button = button.selected(selected);
    ui.add(button).on_hover_text(hover).clicked()
}

/// a name as a wide clickable button; right click renames it inline.
/// returns whether it was left-clicked.
fn name_button(ui: &mut egui::Ui, name: &mut String, selected: bool) -> bool {
    let renaming_id = ui.id().with("renaming");
    let fresh_id = renaming_id.with("fresh");
    let renaming = ui
        .data(|d| d.get_temp::<bool>(renaming_id))
        .unwrap_or(false);
    if renaming {
        let response = ui.add(egui::TextEdit::singleline(name).desired_width(110.0));
        if ui.data(|d| d.get_temp::<bool>(fresh_id)).unwrap_or(false) {
            response.request_focus();
            ui.data_mut(|d| d.remove::<bool>(fresh_id));
        } else if response.lost_focus() {
            ui.data_mut(|d| d.remove::<bool>(renaming_id));
        }
        false
    } else {
        let mut button = egui::Button::new(name.as_str()).min_size(egui::vec2(110.0, 0.0));
        button = button.selected(selected);
        let response = ui.add(button).on_hover_text("right click to rename");
        if response.secondary_clicked() {
            ui.data_mut(|d| d.insert_temp(renaming_id, true));
            ui.data_mut(|d| d.insert_temp(fresh_id, true));
        }
        response.clicked()
    }
}

/// tri-state (may have / must have / must not have) button for one side.
/// left click cycles forward, right click backward.
fn ui_piece_set_term_side(ui: &mut egui::Ui, side: Side, term: &mut PieceSetTerm) {
    #[derive(Clone, Copy, PartialEq)]
    enum SideState {
        May,
        Must,
        Cant,
    }

    let state = if term.must_have.contains(&side) {
        SideState::Must
    } else if term.cant_have.contains(&side) {
        SideState::Cant
    } else {
        SideState::May
    };

    let name = format!("{side:?}");
    let (text, fill, hover) = match state {
        SideState::May => (
            egui::RichText::new(name).color(egui::Color32::GRAY),
            ui.visuals().widgets.inactive.bg_fill,
            "may have",
        ),
        SideState::Must => (
            egui::RichText::new(name)
                .color(egui::Color32::BLACK)
                .strong(),
            side.color(),
            "must have",
        ),
        SideState::Cant => (
            egui::RichText::new(name)
                .color(side.color())
                .strikethrough(),
            egui::Color32::from_gray(48),
            "must not have",
        ),
    };
    let response = ui
        .add(
            egui::Button::new(text)
                .fill(fill)
                .min_size(egui::vec2(22.0, 18.0)),
        )
        .on_hover_text(hover);

    let new_state = if response.clicked() {
        match state {
            SideState::May => SideState::Must,
            SideState::Must => SideState::Cant,
            SideState::Cant => SideState::May,
        }
    } else if response.secondary_clicked() {
        match state {
            SideState::May => SideState::Cant,
            SideState::Must => SideState::May,
            SideState::Cant => SideState::Must,
        }
    } else {
        state
    };
    if new_state != state {
        term.must_have.remove(&side);
        term.cant_have.remove(&side);
        match new_state {
            SideState::May => (),
            SideState::Must => {
                term.must_have.insert(side);
            }
            SideState::Cant => {
                term.cant_have.insert(side);
            }
        }
    }
}

fn ui_filter_style(
    ui: &mut egui::Ui,
    id_salt: &str,
    label: &str,
    default_open: bool,
    style: &mut FilterStyle,
    styles: &Styles,
) {
    let state = CollapsingState::load_with_default_open(
        ui.ctx(),
        ui.make_persistent_id(id_salt),
        default_open,
    );
    let header = state.show_header(ui, |ui| {
        ui.label(label);
        let selected_text = style.name();
        egui::ComboBox::from_id_salt(id_salt)
            .selected_text(selected_text)
            .show_ui(ui, |ui| {
                let is_literal = matches!(style, FilterStyle::Literal(_));
                if ui.selectable_label(is_literal, "literal").clicked() && !is_literal {
                    *style = FilterStyle::Literal(PartialStyle::NONE);
                }
                let is_basic: bool = matches!(style, FilterStyle::Basic(_));
                if ui.selectable_label(is_basic, "basic").clicked() {
                    *style = FilterStyle::Basic(Rc::clone(&styles.basic));
                }
                let is_hovered = matches!(style, FilterStyle::Hovered(_));
                if ui.selectable_label(is_hovered, "hovered").clicked() {
                    *style = FilterStyle::Hovered(Rc::clone(&styles.hovered));
                }
                let is_selected = matches!(style, FilterStyle::Selected(_));
                if ui.selectable_label(is_selected, "selected").clicked() {
                    *style = FilterStyle::Selected(Rc::clone(&styles.selected));
                }
                for s in &styles.user {
                    let is_this = matches!(style, FilterStyle::User(cur) if Rc::ptr_eq(cur, s));
                    if ui
                        .selectable_label(is_this, s.borrow().name.clone())
                        .clicked()
                    {
                        *style = FilterStyle::User(Rc::clone(s));
                    }
                }
            });
    });
    header.body(|ui| match style {
        FilterStyle::Literal(style) => ui_partial_style(ui, style),
        FilterStyle::Basic(_) => {
            ui.weak("edit in the builtin styles section");
        }
        FilterStyle::Hovered(_) => {
            ui.weak("edit in the builtin styles section");
        }
        FilterStyle::Selected(_) => {
            ui.weak("edit in the builtin styles section");
        }
        FilterStyle::User(_) => {
            ui.weak("edit in the user styles section");
        }
    });
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

fn ui_partial_style(ui: &mut egui::Ui, style: &mut PartialStyle) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::puzzle_state::Sticker;

    /// a piece with one (vertexless) sticker per given side.
    fn piece(sides: &[Side]) -> Piece {
        Piece {
            stickers: sides
                .iter()
                .map(|&side| Sticker {
                    verts: vec![],
                    side: Some(side),
                })
                .collect(),
            rot: cgmath::Quaternion::new(1.0, 0.0, 0.0, 0.0),
        }
    }

    fn term(must_have: &[Side], cant_have: &[Side]) -> PieceSetTerm {
        PieceSetTerm {
            must_have: must_have.iter().copied().collect(),
            cant_have: cant_have.iter().copied().collect(),
        }
    }

    #[test]
    fn set_term_semantics() {
        let ruf = piece(&[Side::R, Side::U, Side::F]);
        // empty term matches everything.
        assert!(term(&[], &[]).contains(&ruf));
        // must_have is a subset check, not an exact match.
        assert!(term(&[Side::R], &[]).contains(&ruf));
        assert!(term(&[Side::R, Side::U], &[]).contains(&ruf));
        assert!(!term(&[Side::R, Side::B], &[]).contains(&ruf));
        // cant_have excludes.
        assert!(!term(&[Side::R], &[Side::F]).contains(&ruf));
        assert!(term(&[Side::R], &[Side::B]).contains(&ruf));
    }

    #[test]
    fn style_of_applies_term_and_fallback() {
        let mut filters = Filters::default();
        // drop the seeded "all pieces get basic" term: it's complete and
        // earlier terms win per-field, so it would mask the term under test.
        filters.sequences[0].stages[0].terms.clear();
        filters.sequences[0].stages[0].terms.push(StageTerm {
            set: PieceSet {
                terms: vec![term(&[Side::R], &[])],
            },
            style: FilterStyle::Literal(PartialStyle {
                face_opacity: Some(0.25),
                ..PartialStyle::NONE
            }),
        });

        // matching piece: the term's opacity, other fields from the default.
        let styled = filters.style_of(&piece(&[Side::R, Side::U]));
        assert_eq!(styled.face_opacity, 0.25);
        assert_eq!(styled.outline_opacity, 1.0);
        // non-matching piece: the fallback (here, the default style).
        let fallback = filters.style_of(&piece(&[Side::L]));
        assert_eq!(fallback.face_opacity, 1.0);
    }

    #[test]
    fn no_selection_shows_basic() {
        let mut filters = Filters::default();
        // a stage that styles everything at 0.25...
        let stage = &mut filters.sequences[0].stages[0];
        stage.terms.clear();
        stage.terms.push(StageTerm {
            set: PieceSet {
                terms: vec![term(&[], &[])],
            },
            style: FilterStyle::Literal(PartialStyle {
                face_opacity: Some(0.25),
                ..PartialStyle::NONE
            }),
        });
        assert_eq!(filters.style_of(&piece(&[Side::R])).face_opacity, 0.25);

        // ...is ignored once no stage is selected: everything gets basic.
        filters.selected_seq_stage = SelectedSeqStage::None;
        assert_eq!(filters.style_of(&piece(&[Side::R])).face_opacity, 1.0);
    }

    #[test]
    fn earlier_terms_win() {
        let mut filters = Filters::default();
        let basic = FilterStyle::Basic(Rc::clone(&filters.styles.basic));
        let stage = &mut filters.sequences[0].stages[0];
        // terms read top-down like match arms: a narrowed term first, then a
        // complete catch-all below it.
        stage.terms.clear();
        stage.terms.push(StageTerm {
            set: PieceSet {
                terms: vec![term(&[Side::R], &[])],
            },
            style: FilterStyle::Literal(PartialStyle {
                face_opacity: Some(0.25),
                ..PartialStyle::NONE
            }),
        });
        stage.terms.push(StageTerm::new(basic));

        // the earlier term wins where it matches, even though the later
        // catch-all is complete; its unset fields fall to the catch-all.
        let styled = filters.style_of(&piece(&[Side::R]));
        assert_eq!(styled.face_opacity, 0.25);
        assert_eq!(styled.outline_opacity, 1.0);
        // pieces the earlier term doesn't match get the catch-all.
        let unmatched = filters.style_of(&piece(&[Side::L]));
        assert_eq!(unmatched.face_opacity, 1.0);
    }

    #[test]
    fn fallback_chain() {
        let mut filters = Filters::default();
        let seq = &mut filters.sequences[0];
        // drop the seeded "all pieces get basic" term so that pieces are
        // unmatched and actually reach the fallbacks.
        seq.stages[0].terms.clear();
        seq.fallback = FilterStyle::Literal(PartialStyle {
            face_opacity: Some(0.5),
            outline_size: Some(2.0),
            ..PartialStyle::NONE
        });
        seq.stages[0].fallback = FilterStyle::Literal(PartialStyle {
            face_opacity: Some(0.25),
            ..PartialStyle::NONE
        });

        let styled = filters.style_of(&piece(&[Side::R]));
        // the stage fallback shadows the sequence fallback,
        assert_eq!(styled.face_opacity, 0.25);
        // which fills what the stage fallback leaves unset,
        assert_eq!(styled.outline_size, 2.0);
        // and the default completes the rest.
        assert_eq!(styled.outline_opacity, 1.0);
    }

    #[test]
    fn include_prev_carries_the_prev_stage_style() {
        let mut filters = Filters::default();
        let seq = &mut filters.sequences[0];
        // drop the seeded "all pieces get basic" terms (here and in stage 1
        // below): they're complete and would mask the styles under test.
        seq.stages[0].terms.clear();
        seq.stages[0].terms.push(StageTerm {
            set: PieceSet {
                terms: vec![term(&[Side::R], &[])],
            },
            style: FilterStyle::Literal(PartialStyle {
                face_opacity: Some(0.25),
                ..PartialStyle::NONE
            }),
        });
        seq.stages.push(Stage {
            include_prev: IncludePrev::Prev,
            terms: vec![],
            ..Stage::new(
                "stage 1".to_string(),
                FilterStyle::Basic(Rc::clone(&filters.styles.basic)),
            )
        });
        filters.selected_seq_stage = SelectedSeqStage::Some { seq: 0, stage: 1 };

        let styled = filters.style_of(&piece(&[Side::R]));
        assert_eq!(styled.face_opacity, 0.25);
        let fallback = filters.style_of(&piece(&[Side::L]));
        assert_eq!(fallback.face_opacity, 1.0);
    }
}
