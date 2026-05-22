use eframe::egui;

use crate::app::MikuApp;

const NEW_CLUSTER_DIALOG_WIDTH: f32 = 420.0;
const CONFIG_TEXT_HEIGHT: f32 = 180.0;
const SETTINGS_PANEL_WIDTH: f32 = 420.0;

impl MikuApp {
    #[cfg(target_arch = "wasm32")]
    pub(crate) fn update_file_dialog(&mut self, _ctx: &egui::Context) {}

    pub(crate) fn show_new_cluster_dialog(&mut self, ctx: &egui::Context) {
        if !self.new_cluster_form.open {
            return;
        }

        let mut open = self.new_cluster_form.open;
        egui::Window::new("New Cluster")
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.set_min_width(NEW_CLUSTER_DIALOG_WIDTH);

                ui.label("Context");
                ui.text_edit_singleline(&mut self.new_cluster_form.context);
                ui.add_space(8.0);

                ui.horizontal(|ui| {
                    ui.label("Config");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Choose File").clicked() {
                            self.pick_config_file();
                        }
                    });
                });
                egui::ScrollArea::vertical()
                    .id_salt("new_cluster_config_scroll")
                    .max_height(CONFIG_TEXT_HEIGHT)
                    .min_scrolled_height(CONFIG_TEXT_HEIGHT)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut self.new_cluster_form.config)
                                .desired_width(NEW_CLUSTER_DIALOG_WIDTH)
                                .desired_rows(10),
                        );
                    });

                if let Some(error) = &self.new_cluster_form.error {
                    ui.add_space(8.0);
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }

                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        self.new_cluster_form.cancel();
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let save_enabled = !self.cluster_load_in_flight;
                        if ui
                            .add_enabled(save_enabled, egui::Button::new("Save"))
                            .clicked()
                        {
                            self.submit_new_cluster();
                        }
                    });
                });
            });

        if !open {
            self.new_cluster_form.cancel();
        }
    }

    pub(crate) fn show_settings_panel(&mut self, ctx: &egui::Context) {
        if !self.settings_open {
            return;
        }

        egui::Window::new("Settings")
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .collapsible(false)
            .resizable(false)
            .open(&mut self.settings_open)
            .show(ctx, |ui| {
                ui.set_min_width(SETTINGS_PANEL_WIDTH);
                ui.heading("Settings");
                ui.separator();
                ui.label("Settings will be available here.");
            });
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn pick_config_file(&mut self) {
        self.file_dialog.pick_file();
    }

    #[cfg(target_arch = "wasm32")]
    fn pick_config_file(&mut self) {
        self.new_cluster_form
            .save_failed("file selection is only available in native mode");
    }
}
