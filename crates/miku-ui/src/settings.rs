use eframe::egui;
use egui_dock::{DockArea, DockState, Style, TabViewer};
use miku_api::LlmProviderSettings;

#[derive(Debug)]
pub(crate) struct SettingsPanel {
    dock_state: DockState<SettingsTab>,
    llm: LlmSettingsForm,
}

impl Default for SettingsPanel {
    fn default() -> Self {
        Self {
            dock_state: DockState::new(vec![SettingsTab::General, SettingsTab::Llm]),
            llm: LlmSettingsForm::default(),
        }
    }
}

impl SettingsPanel {
    pub(crate) fn should_load(&self) -> bool {
        !self.llm.loaded && !self.llm.load_in_flight
    }

    pub(crate) fn start_load(&mut self) {
        self.llm.load_in_flight = true;
        self.llm.error = None;
    }

    pub(crate) fn apply_loaded(&mut self, result: Result<LlmProviderSettings, String>) {
        self.llm.load_in_flight = false;
        self.llm.loaded = true;
        self.llm.saved = false;
        match result {
            Ok(settings) => {
                self.llm.form = settings.with_default_base_url();
                self.llm.error = None;
            }
            Err(error) => self.llm.error = Some(error),
        }
    }

    pub(crate) fn apply_saved(&mut self, result: Result<(), String>) {
        self.llm.save_in_flight = false;
        match result {
            Ok(()) => {
                self.llm.saved = true;
                self.llm.error = None;
            }
            Err(error) => {
                self.llm.saved = false;
                self.llm.error = Some(error);
            }
        }
    }

    pub(crate) fn show(&mut self, ui: &mut egui::Ui) -> Vec<SettingsUiRequest> {
        let mut viewer = SettingsTabViewer {
            llm: &mut self.llm,
            requests: Vec::new(),
        };
        DockArea::new(&mut self.dock_state)
            .show_leaf_collapse_buttons(false)
            .show_leaf_close_all_buttons(false)
            .show_close_buttons(false)
            .style(Style::from_egui(ui.style().as_ref()))
            .show_inside(ui, &mut viewer);
        viewer.requests
    }

    #[cfg(test)]
    pub(crate) fn tab_titles(&self) -> Vec<&'static str> {
        self.dock_state
            .iter_all_tabs()
            .map(|(_, tab)| tab.title())
            .collect()
    }
}

#[derive(Clone, Debug)]
pub(crate) enum SettingsUiRequest {
    SaveLlm(LlmProviderSettings),
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum SettingsTab {
    General,
    Llm,
}

impl SettingsTab {
    fn title(&self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Llm => "LLM",
        }
    }
}

#[derive(Debug)]
struct LlmSettingsForm {
    form: LlmProviderSettings,
    loaded: bool,
    load_in_flight: bool,
    save_in_flight: bool,
    pending_save: bool,
    saved: bool,
    error: Option<String>,
}

impl Default for LlmSettingsForm {
    fn default() -> Self {
        Self {
            form: LlmProviderSettings::default().with_default_base_url(),
            loaded: false,
            load_in_flight: false,
            save_in_flight: false,
            pending_save: false,
            saved: false,
            error: None,
        }
    }
}

struct SettingsTabViewer<'a> {
    llm: &'a mut LlmSettingsForm,
    requests: Vec<SettingsUiRequest>,
}

impl TabViewer for SettingsTabViewer<'_> {
    type Tab = SettingsTab;

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        tab.title().into()
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        match tab {
            SettingsTab::General => show_general_settings(ui),
            SettingsTab::Llm => self.show_llm_settings(ui),
        }
    }

    fn is_closeable(&self, _tab: &Self::Tab) -> bool {
        false
    }

    fn context_menu(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab, _path: egui_dock::NodePath) {
        crate::clipboard::copy_name_menu_item(ui, tab.title());
    }

    fn allowed_in_windows(&self, _tab: &mut Self::Tab) -> bool {
        false
    }
}

impl SettingsTabViewer<'_> {
    fn show_llm_settings(&mut self, ui: &mut egui::Ui) {
        ui.heading("LLM");
        ui.separator();

        let mut changed = false;

        egui::Grid::new("llm_settings_grid")
            .num_columns(2)
            .spacing([12.0, 10.0])
            .show(ui, |ui| {
                ui.label("Base URL");
                changed |= ui
                    .text_edit_singleline(&mut self.llm.form.base_url)
                    .changed();
                ui.end_row();

                ui.label("API Key");
                changed |= ui
                    .add(egui::TextEdit::singleline(&mut self.llm.form.api_key).password(true))
                    .changed();
                ui.end_row();

                ui.label("Model");
                changed |= ui.text_edit_singleline(&mut self.llm.form.model).changed();
                ui.end_row();

                ui.label("Streaming");
                changed |= ui.checkbox(&mut self.llm.form.stream, "Enabled").changed();
                ui.end_row();
            });
        ui.add_space(12.0);

        if changed && self.llm.loaded && !self.llm.load_in_flight {
            self.llm.saved = false;
            self.llm.error = None;
            if self.llm.save_in_flight {
                self.llm.pending_save = true;
            } else {
                self.enqueue_save();
            }
        }

        if self.llm.pending_save && !self.llm.save_in_flight && !self.llm.load_in_flight {
            self.enqueue_save();
        }

        if self.llm.load_in_flight {
            ui.label("Loading settings...");
        } else if self.llm.save_in_flight || self.llm.pending_save {
            ui.label("Saving...");
        }
        if let Some(error) = &self.llm.error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        } else if self.llm.saved {
            ui.label("Saved.");
        }
    }

    fn enqueue_save(&mut self) {
        self.llm.save_in_flight = true;
        self.llm.pending_save = false;
        self.llm.saved = false;
        self.llm.error = None;
        self.requests
            .push(SettingsUiRequest::SaveLlm(self.llm.form.clone()));
    }
}

fn show_general_settings(ui: &mut egui::Ui) {
    ui.heading("General");
    ui.separator();
    ui.label("General settings are not available yet.");
}

trait LlmProviderSettingsExt {
    fn with_default_base_url(self) -> Self;
}

impl LlmProviderSettingsExt for LlmProviderSettings {
    fn with_default_base_url(mut self) -> Self {
        if self.base_url.trim().is_empty() {
            self.base_url = "https://api.openai.com/v1".to_owned();
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_panel_starts_with_general_and_llm_tabs() {
        let panel = SettingsPanel::default();

        assert_eq!(panel.tab_titles(), vec!["General", "LLM"]);
    }

    #[test]
    fn default_llm_form_prefills_openai_base_url() {
        let panel = SettingsPanel::default();

        assert_eq!(panel.llm.form.base_url, "https://api.openai.com/v1");
        assert_eq!(panel.llm.form.model, "");
        assert!(panel.llm.form.stream);
    }
}
