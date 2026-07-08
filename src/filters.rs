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
        /// completes any fields the fallback chain leaves unset.
        pub const DEFAULT: Self = CompleteStyle {
            face_opacity: 1.0,
            face_color: FaceColor::Sticker,
            outline_opacity: 1.0,
            outline_size: 1.0,
            outline_color: Color32::BLACK,
        };
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

    /// a factored-out named style. `RefCell` so edits (and renames) in the
    /// shared-styles UI propagate to everything referencing it.
    #[derive(Debug, Clone)]
    pub struct SharedStyle {
        pub name: String,
        pub style: PartialStyle,
    }

    #[derive(Debug, Clone)]
    pub enum BoxPartialStyle {
        Literal(PartialStyle),
        Shared(Rc<RefCell<SharedStyle>>),
    }
    impl BoxPartialStyle {
        pub fn resolve(&self) -> PartialStyle {
            match self {
                BoxPartialStyle::Literal(s) => s.clone(),
                BoxPartialStyle::Shared(s) => s.borrow().style.clone(),
            }
        }
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

    /// milo-syntax-ish summary, e.g. "RU!F".
    fn label(&self) -> String {
        let mut s = String::new();
        for side in Side::ALL {
            if self.must_have.contains(&side) {
                s.push_str(&format!("{side:?}"));
            }
        }
        for side in Side::ALL {
            if self.cant_have.contains(&side) {
                s.push_str(&format!("!{side:?}"));
            }
        }
        if s.is_empty() { "all".to_string() } else { s }
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
    style: BoxPartialStyle,
}
impl StageTerm {
    fn new() -> Self {
        Self {
            set: PieceSet {
                terms: vec![PieceSetTerm::default()],
            },
            style: BoxPartialStyle::Literal(PartialStyle::NONE),
        }
    }

    fn get(&self, piece: &Piece) -> Option<PartialStyle> {
        if self.set.contains(piece) {
            Some(self.style.resolve())
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
    Fixed(BoxPartialStyle),
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
    fallback: BoxPartialStyle,
}
impl Stage {
    fn new(name: String) -> Self {
        Self {
            name,
            terms: vec![],
            include_prev: IncludePrev::Dont,
            fallback: BoxPartialStyle::Literal(PartialStyle::NONE),
        }
    }
}

#[derive(Debug, Clone)]
struct Sequence {
    name: String,
    /// fields the stage fallback leaves unset fall through to this
    /// (and finally to `CompleteStyle::DEFAULT`).
    fallback: BoxPartialStyle,
    stages: Vec<Stage>,
    stage_idx: usize,
}
impl Sequence {
    fn new(name: String) -> Self {
        Self {
            name,
            fallback: BoxPartialStyle::Literal(PartialStyle::NONE),
            stages: vec![Stage::new("stage 0".to_string())],
            stage_idx: 0,
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
                        ret = Some(ret.unwrap_or(PartialStyle::NONE).or(&style.resolve()));
                    }
                }
            }
        }

        ret
    }
}

#[derive(Debug, Clone)]
pub struct Filters {
    /// the pool of factored-out styles offered by the style pickers.
    /// deleting one here doesn't break existing references to it;
    /// it just stops being offered for new uses.
    shared_styles: Vec<Rc<RefCell<SharedStyle>>>,
    sequences: Vec<Sequence>,
    sequence_idx: usize,
}
impl Filters {
    // TODO: bake the prev stage search into the stage to make this constant time
    // (rather than linear) (in the number of stages) (it'll still be linear in the number of terms).
    pub fn style_of(&self, piece: &Piece) -> CompleteStyle {
        let seq = &self.sequences[self.sequence_idx];
        let stage = &seq.stages[seq.stage_idx];
        // unmatched pieces (and fields a matched style leaves unset) fall
        // through the stage fallback, then the sequence fallback, then the
        // hardcoded default.
        let fallback = stage.fallback.resolve().or(&seq.fallback.resolve());
        match seq.get_no_fallback(piece, seq.stage_idx) {
            None => fallback,
            Some(style) => style.or(&fallback),
        }
        .unwrap_or(&CompleteStyle::DEFAULT)
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        ui.heading("filters");
        ui.separator();
        self.ui_sequences(ui);
        ui.separator();
        self.ui_stage(ui);
        ui.separator();
        self.ui_shared_styles(ui);
    }

    /// section 1: the sequences (collapsible, containing their stages), by name.
    fn ui_sequences(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.strong("sequences");
            if ui
                .small_button("+")
                .on_hover_text("insert a new sequence after the selected one")
                .clicked()
            {
                let i = self.sequence_idx + 1;
                self.sequences.insert(
                    i,
                    Sequence::new(format!("sequence {}", self.sequences.len())),
                );
                self.sequence_idx = i;
            }
        });

        let sequence_idx = self.sequence_idx;
        let n_seqs = self.sequences.len();
        let mut select_seq = None;
        let mut remove_seq = None;
        // (sequence, position): insert a copy of the selected stage there.
        let mut insert_stage = None;
        for (i, seq) in self.sequences.iter_mut().enumerate() {
            ui.push_id(("sequence", i), |ui| {
                let state = CollapsingState::load_with_default_open(
                    ui.ctx(),
                    ui.make_persistent_id("sequence"),
                    true,
                );
                let header = state.show_header(ui, |ui| {
                    if name_button(ui, &mut seq.name, i == sequence_idx) {
                        select_seq = Some(i);
                    }
                    if n_seqs > 1
                        && ui
                            .small_button("−")
                            .on_hover_text("delete this sequence")
                            .clicked()
                    {
                        remove_seq = Some(i);
                    }
                    if ui
                        .small_button("+ stage")
                        .on_hover_text("insert a copy of the selected stage below this row")
                        .clicked()
                    {
                        insert_stage = Some((i, 0));
                    }
                });
                header.body(|ui| {
                    let stage_idx = seq.stage_idx;
                    let n_stages = seq.stages.len();
                    let mut select_stage = None;
                    let mut remove_stage = None;
                    for (j, stage) in seq.stages.iter_mut().enumerate() {
                        ui.push_id(("stage", j), |ui| {
                            ui.horizontal(|ui| {
                                let selected = i == sequence_idx && j == stage_idx;
                                if name_button(ui, &mut stage.name, selected) {
                                    select_stage = Some(j);
                                }
                                if n_stages > 1
                                    && ui
                                        .small_button("−")
                                        .on_hover_text("delete this stage")
                                        .clicked()
                                {
                                    remove_stage = Some(j);
                                }
                                if ui
                                    .small_button("+ stage")
                                    .on_hover_text(
                                        "insert a copy of the selected stage below this row",
                                    )
                                    .clicked()
                                {
                                    insert_stage = Some((i, j + 1));
                                }
                            });
                        });
                    }
                    if let Some(j) = select_stage {
                        seq.stage_idx = j;
                        select_seq = Some(i);
                    }
                    if let Some(j) = remove_stage {
                        seq.stages.remove(j);
                        if seq.stage_idx > j {
                            seq.stage_idx -= 1;
                        }
                        seq.stage_idx = seq.stage_idx.min(seq.stages.len() - 1);
                    }
                });
            });
        }
        if let Some(i) = select_seq {
            self.sequence_idx = i;
        }
        if let Some((i, pos)) = insert_stage {
            let src = &self.sequences[self.sequence_idx];
            let copy = src.stages[src.stage_idx].clone();
            let seq = &mut self.sequences[i];
            seq.stages.insert(pos, copy);
            seq.stage_idx = pos;
            self.sequence_idx = i;
        }
        if let Some(i) = remove_seq {
            self.sequences.remove(i);
            if self.sequence_idx > i {
                self.sequence_idx -= 1;
            }
            self.sequence_idx = self.sequence_idx.min(self.sequences.len() - 1);
        }
    }

    /// section 2: the contents of the selected stage,
    /// in processing order: terms, prev stage, stage fallback, sequence fallback.
    fn ui_stage(&mut self, ui: &mut egui::Ui) {
        let shared = self.shared_styles.clone();
        let seq = &mut self.sequences[self.sequence_idx];

        ui.strong(format!("{} / {}", seq.name, seq.stages[seq.stage_idx].name));

        let stage_idx = seq.stage_idx;
        let stage = &mut seq.stages[stage_idx];

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
                    if ui.small_button("−").clicked() {
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
                                    side_state_ui(ui, side, set_term);
                                }
                                if n_rows > 1 && ui.small_button("−").clicked() {
                                    remove_row = Some(row_idx);
                                }
                            });
                        });
                    }
                    if let Some(row_idx) = remove_row {
                        term.set.terms.remove(row_idx);
                    }
                    // another union row.
                    if ui.small_button("+").clicked() {
                        term.set.terms.push(PieceSetTerm::default());
                    }
                    box_style_ui(ui, "term_style", "style", true, &mut term.style, &shared);
                });
            });
        }
        if let Some(term_idx) = remove_term {
            stage.terms.remove(term_idx);
        }
        if ui.button("+ term").clicked() {
            stage.terms.push(StageTerm::new());
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
                                IncludePrev::Fixed(BoxPartialStyle::Literal(PartialStyle::NONE));
                        }
                    });
            });
            if let IncludePrev::Fixed(style) = &mut stage.include_prev {
                // indented: the style belongs to the dropdown's choice.
                ui.indent("include_prev_style", |ui| {
                    box_style_ui(ui, "include_prev_style", "style", true, style, &shared);
                });
            }
        });

        box_style_ui(
            ui,
            "stage_fallback",
            "stage fallback",
            false,
            &mut stage.fallback,
            &shared,
        );
        // sequence-level, not stage-level.
        ui.separator();
        box_style_ui(
            ui,
            "sequence_fallback",
            "sequence fallback",
            false,
            &mut seq.fallback,
            &shared,
        );
    }

    /// section 3: the pool of shared styles.
    fn ui_shared_styles(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.strong("shared styles");
            if ui.small_button("+").clicked() {
                self.shared_styles.push(Rc::new(RefCell::new(SharedStyle {
                    name: format!("style {}", self.shared_styles.len()),
                    style: PartialStyle::NONE,
                })));
            }
        });
        let mut remove = None;
        for (i, style) in self.shared_styles.iter().enumerate() {
            ui.push_id(("shared_style", i), |ui| {
                let mut style = style.borrow_mut();
                let state = CollapsingState::load_with_default_open(
                    ui.ctx(),
                    ui.make_persistent_id("shared_style"),
                    false,
                );
                let header = state.show_header(ui, |ui| {
                    name_button(ui, &mut style.name, false);
                    if ui.small_button("−").clicked() {
                        remove = Some(i);
                    }
                });
                header.body(|ui| partial_style_ui(ui, &mut style.style));
            });
        }
        if let Some(i) = remove {
            self.shared_styles.remove(i);
        }
    }
}
impl Default for Filters {
    fn default() -> Self {
        Self {
            shared_styles: vec![Rc::new(RefCell::new(SharedStyle {
                name: "dimmed".to_string(),
                style: PartialStyle {
                    face_opacity: Some(0.15),
                    face_color: None,
                    outline_opacity: Some(0.15),
                    outline_size: None,
                    outline_color: None,
                },
            }))],
            sequences: vec![Sequence::new("sequence 0".to_string())],
            sequence_idx: 0,
        }
    }
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
        if selected {
            button = button.fill(ui.visuals().selection.bg_fill);
        }
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
fn side_state_ui(ui: &mut egui::Ui, side: Side, term: &mut PieceSetTerm) {
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

/// picker between a literal style (edited inline, collapsible) and a shared
/// style (picked by name, edited in the shared-styles section).
fn box_style_ui(
    ui: &mut egui::Ui,
    id_salt: &str,
    label: &str,
    default_open: bool,
    style: &mut BoxPartialStyle,
    shared: &[Rc<RefCell<SharedStyle>>],
) {
    let state = CollapsingState::load_with_default_open(
        ui.ctx(),
        ui.make_persistent_id(id_salt),
        default_open,
    );
    let header = state.show_header(ui, |ui| {
        ui.label(label);
        let selected_text = match style {
            BoxPartialStyle::Literal(_) => "literal".to_string(),
            BoxPartialStyle::Shared(s) => s.borrow().name.clone(),
        };
        egui::ComboBox::from_id_salt(id_salt)
            .selected_text(selected_text)
            .show_ui(ui, |ui| {
                let is_literal = matches!(style, BoxPartialStyle::Literal(_));
                if ui.selectable_label(is_literal, "literal").clicked() && !is_literal {
                    // seed the literal with the shared style's current contents.
                    *style = BoxPartialStyle::Literal(style.resolve());
                }
                for s in shared {
                    let is_this =
                        matches!(style, BoxPartialStyle::Shared(cur) if Rc::ptr_eq(cur, s));
                    if ui
                        .selectable_label(is_this, s.borrow().name.clone())
                        .clicked()
                    {
                        *style = BoxPartialStyle::Shared(Rc::clone(s));
                    }
                }
            });
    });
    header.body(|ui| match style {
        BoxPartialStyle::Literal(style) => partial_style_ui(ui, style),
        BoxPartialStyle::Shared(_) => {
            ui.weak("edit in the shared styles section");
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

fn partial_style_ui(ui: &mut egui::Ui, style: &mut PartialStyle) {
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
        filters.sequences[0].stages[0].terms.push(StageTerm {
            set: PieceSet {
                terms: vec![term(&[Side::R], &[])],
            },
            style: BoxPartialStyle::Literal(PartialStyle {
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
    fn fallback_chain() {
        let mut filters = Filters::default();
        let seq = &mut filters.sequences[0];
        seq.fallback = BoxPartialStyle::Literal(PartialStyle {
            face_opacity: Some(0.5),
            outline_size: Some(2.0),
            ..PartialStyle::NONE
        });
        seq.stages[0].fallback = BoxPartialStyle::Literal(PartialStyle {
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
        seq.stages[0].terms.push(StageTerm {
            set: PieceSet {
                terms: vec![term(&[Side::R], &[])],
            },
            style: BoxPartialStyle::Literal(PartialStyle {
                face_opacity: Some(0.25),
                ..PartialStyle::NONE
            }),
        });
        seq.stages.push(Stage {
            include_prev: IncludePrev::Prev,
            ..Stage::new("stage 1".to_string())
        });
        seq.stage_idx = 1;

        let styled = filters.style_of(&piece(&[Side::R]));
        assert_eq!(styled.face_opacity, 0.25);
        let fallback = filters.style_of(&piece(&[Side::L]));
        assert_eq!(fallback.face_opacity, 1.0);
    }
}
