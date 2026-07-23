use std::{collections::VecDeque, time::Instant};

use eframe::egui;

use crate::{
    commands::{Command, Origin},
    filters::Filters,
    keybinds::{self, InputContext, Keybinds},
    puzzle_state::*,
    puzzle_view::*,
    simulation::PuzzleSimulation,
    styles::StyleEditor,
};

/// Which subsystem's UI the sidebar shows (HSC2-style, minus the icons).
#[derive(Clone, Copy, PartialEq)]
enum SidebarTab {
    Twists,
    View,
    Filters,
    Styles,
    Keybinds,
}
impl SidebarTab {
    const ALL: [Self; 5] = [
        Self::Twists,
        Self::View,
        Self::Filters,
        Self::Styles,
        Self::Keybinds,
    ];

    fn name(self) -> &'static str {
        match self {
            Self::Twists => "Twists",
            Self::View => "View",
            Self::Filters => "Filters",
            Self::Styles => "Styles",
            Self::Keybinds => "Keybinds",
        }
    }
}

/// The hub: owns every component and routes `Command`s between them each
/// frame. Components never reference each other; anything cross-component
/// goes through `queue`, which is also where a future log file will observe
/// the command stream.
pub struct App {
    sim: PuzzleSimulation,
    puzzle_view: PuzzleView,
    keybinds: Keybinds,
    filters: Filters,
    style_editor: StyleEditor,
    sidebar_tab: SidebarTab,
    queue: VecDeque<Command>,
}
impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let render_state = cc
            .wgpu_render_state
            .clone()
            .expect("main requests the wgpu renderer");
        // filters hold `Rc` handles into the style set, so the styles
        // component exists first.
        let style_editor = StyleEditor::default();
        let filters = Filters::new(&style_editor.styles);
        Self {
            sim: PuzzleSimulation::new(PuzzleState::new()),
            puzzle_view: PuzzleView::new(render_state),
            keybinds: Keybinds::default(),
            filters,
            style_editor,
            sidebar_tab: SidebarTab::Twists,
            queue: VecDeque::new(),
        }
    }

    /// twist-input sidebar tab: the layer mask + a button per face twist. the
    /// mask is the `default_mask` keybind variable, so these buttons twist
    /// exactly what the keys and the mouse twist.
    fn twists_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("default_mask");
            match self.keybinds.var_mut("default_mask") {
                Some(value) => keybinds::ui_value(ui, value),
                None => {
                    ui.weak("(no such variable)");
                }
            }
        });
        let layers = self.keybinds.default_mask();
        for side in Side::ALL {
            ui.horizontal(|ui| {
                for multiplicity in [-1, 1] {
                    let label = if multiplicity < 0 {
                        format!("{side:?}'")
                    } else {
                        format!("{side:?}")
                    };
                    if ui.button(label).clicked() {
                        self.queue.push_back(Command::Twist {
                            twist: Twist {
                                side,
                                layers,
                                multiplicity,
                            },
                            origin: Origin::User,
                        });
                    }
                }
            });
        }
    }

    /// drain the command queue, routing each command to the component that
    /// owns it.
    fn route_commands(&mut self, now: Instant) {
        while let Some(command) = self.queue.pop_front() {
            match command {
                Command::Twist { .. }
                | Command::Reorient { .. }
                | Command::Undo
                | Command::Redo => {
                    self.sim.handle(command, now);
                }
                // align spans two components: the view keeps its sub-90°
                // residual and the simulation bakes the axis-aligned part
                // into the puzzle state.
                Command::Align => {
                    let orientation = self.puzzle_view.align(now);
                    self.sim.align(orientation, now);
                }
                Command::TogglePieceSelection(piece_idx) => {
                    self.puzzle_view.toggle_selection(piece_idx);
                }
                Command::ClearSelection => self.puzzle_view.clear_selection(),
            }
        }
    }
}
impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.ctx().request_repaint();
        let now = Instant::now();

        egui::Panel::left("sidebar").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("speedsun");
                egui::ComboBox::from_id_salt("sidebar_tab")
                    .selected_text(self.sidebar_tab.name())
                    .show_ui(ui, |ui| {
                        for tab in SidebarTab::ALL {
                            ui.selectable_value(&mut self.sidebar_tab, tab, tab.name());
                        }
                    });
            });
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| match self.sidebar_tab {
                SidebarTab::Twists => self.twists_ui(ui),
                SidebarTab::View => self.puzzle_view.ui(ui),
                SidebarTab::Filters => self.filters.ui(ui, &self.style_editor.styles),
                SidebarTab::Styles => self.style_editor.ui(ui),
                SidebarTab::Keybinds => self.keybinds.ui(ui),
            });
        });

        // the pinned keybind variables. only there once something is pinned,
        // so an unused bar doesn't eat a strip of the window.
        if self.keybinds.has_pinned() {
            egui::Panel::bottom("pinned_variables").show(ui, |ui| {
                self.keybinds.pinned_ui(ui);
            });
        }

        egui::CentralPanel::default().show(ui, |ui| {
            let (rect, response) =
                ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());

            // drag to rotate
            {
                const SENSITIVITY: f32 = 0.5;
                let drag = response.drag_delta();
                self.puzzle_view.drag(drag, SENSITIVITY);
            }

            // gather commands: pointer input first (its hover feeds the
            // keybind context), then the keybind pass, which is where both the
            // keyboard and the mouse turn into twists.
            let (commands, hover) = self.puzzle_view.interact(&self.sim, &response, now);
            self.queue.extend(commands);
            let (hovered_grip, hovered_grip_inverted) = self.puzzle_view.hovered_grip(&hover);
            let input_context = InputContext {
                hovered_grip,
                hovered_grip_inverted,
            };
            self.queue
                .extend(self.keybinds.collect(ui.ctx(), &input_context));

            // route, tick, draw: twists submitted above start animating on
            // this same frame.
            self.route_commands(now);
            let stable_dt = ui.ctx().input(|i| i.stable_dt);
            self.sim
                .tick(now, stable_dt, self.puzzle_view.twist_duration);
            self.puzzle_view.draw(
                &self.sim,
                &self.filters,
                &self.style_editor.styles,
                &hover,
                self.keybinds.default_mask(),
                &ui.painter_at(rect),
                now,
            );
        });
    }
}
