use std::{collections::VecDeque, time::Instant};

use eframe::egui;

use crate::{
    commands::{Command, Origin},
    filters::Filters,
    keybinds::{InputContext, Keybinds},
    puzzle_state::*,
    puzzle_view::*,
    simulation::PuzzleSimulation,
};

/// The hub: owns every component and routes `Command`s between them each
/// frame. Components never reference each other; anything cross-component
/// goes through `queue`, which is also where a future log file will observe
/// the command stream.
pub struct App {
    sim: PuzzleSimulation,
    puzzle_view: PuzzleView,
    keybinds: Keybinds,
    layer: u8,
    filters: Filters,
    queue: VecDeque<Command>,
}
impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let render_state = cc
            .wgpu_render_state
            .clone()
            .expect("main requests the wgpu renderer");
        Self {
            sim: PuzzleSimulation::new(PuzzleState::new()),
            puzzle_view: PuzzleView::new(render_state),
            keybinds: Keybinds,
            layer: 0,
            filters: Filters::default(),
            queue: VecDeque::new(),
        }
    }

    /// drain the command queue, routing each command to the component that
    /// owns it. components may push follow-up commands (e.g. undoing a
    /// rotation: the simulation records it, the camera applies it).
    fn route_commands(&mut self, now: Instant) {
        let mut budget = 100;
        while let Some(command) = self.queue.pop_front() {
            budget -= 1;
            if budget < 0 {
                debug_assert!(false, "command ping-pong: {command:?}");
                break;
            }
            match command {
                Command::Twist { .. } | Command::Undo | Command::Redo => {
                    self.queue.extend(self.sim.handle(command, now));
                }
                Command::Rotate { rotation, origin } => {
                    if origin == Origin::User {
                        self.sim.record_rotation(rotation);
                    }
                    self.puzzle_view.apply_rotation(rotation, now);
                }
                Command::Align => self.puzzle_view.align(now),
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

        egui::Panel::left("left").show(ui, |ui| {
            ui.heading("speedsun");
            ui.separator();
            ui.label("twist input");
            ui.add(egui::Slider::new(&mut self.layer, 0..=2).text("layer"));
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
                                    layer: self.layer,
                                    multiplicity,
                                },
                                origin: Origin::User,
                            });
                        }
                    }
                });
            }

            ui.separator();
            ui.label("view");
            self.puzzle_view.ui(ui);
        });

        egui::Panel::right("filters").show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                self.filters.ui(ui);
            });
        });

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
            // keybind context), then keys.
            let (commands, hover) = self
                .puzzle_view
                .interact(&self.sim, &response, self.layer, now);
            self.queue.extend(commands);
            let input_context = InputContext {
                alignment: self.puzzle_view.alignment(),
                layer: self.layer,
                hovered_gizmo: hover.gizmo,
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
                &hover,
                self.layer,
                &ui.painter_at(rect),
                now,
            );
        });
    }
}
