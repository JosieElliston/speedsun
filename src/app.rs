use eframe::egui;

use crate::{camera::*, puzzle::*};

pub struct App {
    camera: Camera,
    puzzle: MixupCube,
    layer: u8,
}
impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        Self {
            camera: Camera::new(),
            puzzle: MixupCube::new(),
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
                        let r = self.puzzle.twist(Twist {
                            side,
                            layer: self.layer,
                            multiplicity: -1,
                        });
                    }
                    if ui.button(format!("{side:?}")).clicked() {
                        let r = self.puzzle.twist(Twist {
                            side,
                            layer: self.layer,
                            multiplicity: 1,
                        });
                    }
                });
            }
        });

        egui::CentralPanel::default().show(ui, |ui| {
            let (rect, response) =
                ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());

            // drag to rotate
            {
                const SENSITIVITY: f32 = 0.5;
                let drag = response.drag_delta();
                self.camera.drag(drag, SENSITIVITY);
            }

            crate::camera::show(ui, rect, &self.camera, &self.puzzle);
        });
    }
}
