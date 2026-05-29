use eframe::egui;

use crate::app::MikuApp;

impl MikuApp {
    pub(crate) fn show_menu_bar(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("File", |ui| {
            if ui
                .button(format!("{} Settings", egui_phosphor::regular::GEAR))
                .clicked()
            {
                self.settings_open = true;
                ui.close();
            }

            ui.separator();

            if ui
                .button(format!("{} Quit", egui_phosphor::regular::SIGN_OUT))
                .clicked()
            {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });

        ui.menu_button("View", |ui| {
            ui.checkbox(
                &mut self.left_sidebar_visible,
                format!("{} Left Sidebar", egui_phosphor::regular::SIDEBAR),
            )
            .on_hover_text("Show or hide the cluster and resource sidebar");
            ui.separator();
            ui.checkbox(
                &mut self.right_sidebar_visible,
                format!("{} Right Sidebar", egui_phosphor::regular::SIDEBAR_SIMPLE),
            )
            .on_hover_text("Show or hide the agent sidebar");
        });

        ui.menu_button("Help", |ui| {
            if ui
                .button(format!("{} About Miku", egui_phosphor::regular::INFO))
                .clicked()
            {
                self.about_open = true;
                ui.close();
            }
        });

        ui.add_space(8.0);

        let control_button_width = 3.0 * ui.spacing().interact_size.x
            + 2.0 * ui.spacing().item_spacing.x
            + ui.spacing().item_spacing.x;
        let mut drag_rect = ui.available_rect_before_wrap();
        drag_rect.max.x = (drag_rect.max.x - control_button_width).max(drag_rect.min.x);

        if drag_rect.is_positive() {
            let drag_response = ui.interact(
                drag_rect,
                ui.id().with("title_bar_drag_region"),
                egui::Sense::click_and_drag(),
            );
            if drag_response.drag_started() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
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
