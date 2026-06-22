use eframe::egui;

use crate::app::{MikuApp, about_git_commit_sha};

const NEW_CLUSTER_DIALOG_WIDTH: f32 = 420.0;
const CONFIG_TEXT_HEIGHT: f32 = 180.0;
const SETTINGS_PANEL_WIDTH: f32 = 520.0;
const SETTINGS_PANEL_HEIGHT: f32 = 360.0;
const ABOUT_DIALOG_WIDTH: f32 = 360.0;
const ABOUT_ICON_MAX_SIDE: f32 = 160.0;
const ABOUT_APP_ICON_PNG: &[u8] = include_bytes!("../assets/icons/macOS-Default-1024x1024@1x.png");

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

        if self.settings_panel.should_load() {
            self.request_llm_settings_load();
        }

        let mut open = self.settings_open;
        let mut requests = Vec::new();
        egui::Window::new("Settings")
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .collapsible(false)
            .resizable(true)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.set_min_width(SETTINGS_PANEL_WIDTH);
                ui.set_min_height(SETTINGS_PANEL_HEIGHT);
                requests = self.settings_panel.show(ui);
            });
        self.settings_open = open;

        for request in requests {
            match request {
                crate::settings::SettingsUiRequest::SaveLlm(settings) => {
                    self.request_llm_settings_save(settings);
                }
            }
        }
    }

    pub(crate) fn show_about_dialog(&mut self, ctx: &egui::Context) {
        if !self.about_open {
            return;
        }

        let mut open = self.about_open;
        let mut close_clicked = false;
        egui::Window::new("About Miku")
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.set_min_width(ABOUT_DIALOG_WIDTH);
                ui.vertical_centered(|ui| {
                    ui.add_space(10.0);
                    ui.label(egui::RichText::new("Miku").strong().size(22.0));
                    ui.add_space(18.0);

                    if let Some(texture) = self.about_icon_texture(ctx) {
                        let source_size = texture.size_vec2();
                        let scale =
                            (ABOUT_ICON_MAX_SIDE / source_size.x.max(source_size.y)).min(1.0);
                        let display_size = source_size * scale;
                        ui.add(egui::Image::from_texture(texture).fit_to_exact_size(display_size));
                        ui.add_space(12.0);
                    }

                    ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
                    ui.monospace(format!("Git commit {}", about_git_commit_sha()));
                    ui.add_space(4.0);
                    ui.hyperlink_to(env!("CARGO_PKG_REPOSITORY"), env!("CARGO_PKG_REPOSITORY"));
                    ui.add_space(12.0);

                    if ui.button("Close").clicked() {
                        close_clicked = true;
                    }
                });
            });

        self.about_open = open && !close_clicked;
    }

    fn about_icon_texture(&mut self, ctx: &egui::Context) -> Option<&egui::TextureHandle> {
        if self.about_icon.is_none() && !self.about_icon_load_failed {
            match load_about_icon_image() {
                Ok(image) => {
                    self.about_icon =
                        Some(ctx.load_texture("about-dialog-app-icon", image, Default::default()));
                }
                Err(error) => {
                    self.about_icon_load_failed = true;
                    tracing::warn!(%error, "failed to decode About dialog icon");
                }
            }
        }

        self.about_icon.as_ref()
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

fn load_about_icon_image() -> Result<egui::ColorImage, image::ImageError> {
    let image = image::load_from_memory(ABOUT_APP_ICON_PNG)?.into_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    let rgba = image.into_raw();

    Ok(egui::ColorImage::from_rgba_unmultiplied(size, &rgba))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn about_icon_decodes_embedded_png() {
        let image = load_about_icon_image().unwrap();

        assert_eq!(image.size, [1024, 1024]);
        assert!(!image.pixels.is_empty());
    }
}
