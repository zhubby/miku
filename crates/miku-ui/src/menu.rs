use eframe::egui;

use crate::app::MikuApp;

impl MikuApp {
    pub(crate) fn show_menu_bar(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("File", |ui| {
            if ui.button("Settings").clicked() {
                self.settings_open = true;
                ui.close();
            }

            if ui.button("Quit").clicked() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });

        ui.menu_button("View", |ui| {
            ui.label("Workspace");
            ui.label("Logs");
        });

        ui.add_space(8.0);

        let drag_response = ui.interact(
            ui.available_rect_before_wrap(),
            ui.id().with("title_bar_drag_region"),
            egui::Sense::click_and_drag(),
        );
        if drag_response.drag_started() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .button(egui_phosphor::regular::X)
                .on_hover_text("Close")
                .clicked()
            {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }

            if ui
                .button(egui_phosphor::regular::SQUARE)
                .on_hover_text("Maximize")
                .clicked()
            {
                let maximized = ui
                    .ctx()
                    .input(|input| input.viewport().maximized.unwrap_or(false));
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
            }

            if ui
                .button(egui_phosphor::regular::MINUS)
                .on_hover_text("Minimize")
                .clicked()
            {
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::Minimized(true));
            }
        });
    }
}
