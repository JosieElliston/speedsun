use std::{sync::Arc, time::Instant};

use eframe::egui::{self, mutex::Mutex};

use crate::{puzzle_state::*, puzzle_view::*};

pub struct App {
    puzzle: Arc<Mutex<PuzzleState>>,
    puzzle_view: PuzzleView,
    layer: u8,
}
impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let puzzle = Arc::new(Mutex::new(PuzzleState::new()));
        Self {
            puzzle: Arc::clone(&puzzle),
            puzzle_view: PuzzleView::new(Arc::clone(&puzzle)),
            layer: 0,
        }
    }
}
impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        ui.ctx().request_repaint();

        egui::Panel::left("left").show(ui, |ui| {
            ui.heading("speedsun");
            ui.separator();
            ui.label("twist input");
            ui.add(egui::Slider::new(&mut self.layer, 0..=2).text("layer"));
            for side in Side::ALL {
                ui.horizontal(|ui| {
                    if ui.button(format!("{side:?}'")).clicked() {
                        let r = self.puzzle.lock().twist(Twist {
                            side,
                            layer: self.layer,
                            multiplicity: -1,
                        });
                    }
                    if ui.button(format!("{side:?}")).clicked() {
                        let r = self.puzzle.lock().twist(Twist {
                            side,
                            layer: self.layer,
                            multiplicity: 1,
                        });
                    }
                });
            }

            ui.separator();
            ui.label("view");
            self.puzzle_view.ui(ui);
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

            self.puzzle_view.draw(&ui.painter_at(rect), Instant::now());
        });
    }
}
