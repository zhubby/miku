#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeMode {
    Native,
    Web,
}

#[derive(Clone, Debug)]
pub struct AppState {
    runtime_mode: RuntimeMode,
    selected_cluster_name: Option<String>,
}

impl AppState {
    pub fn new(runtime_mode: RuntimeMode) -> Self {
        Self {
            runtime_mode,
            selected_cluster_name: None,
        }
    }

    pub fn runtime_mode(&self) -> RuntimeMode {
        self.runtime_mode
    }

    pub fn selected_cluster_name(&self) -> Option<&str> {
        self.selected_cluster_name.as_deref()
    }

    pub fn status_message(&self) -> &str {
        match self.selected_cluster_name {
            Some(_) => "Connected",
            None => "No cluster selected",
        }
    }
}

pub fn app_title(runtime_mode: RuntimeMode) -> &'static str {
    match runtime_mode {
        RuntimeMode::Native => "Miku - Native",
        RuntimeMode::Web => "Miku - Web",
    }
}

#[derive(Debug)]
pub struct MikuApp {
    state: AppState,
}

impl MikuApp {
    pub fn new(runtime_mode: RuntimeMode) -> Self {
        Self {
            state: AppState::new(runtime_mode),
        }
    }

    pub fn native() -> Self {
        Self::new(RuntimeMode::Native)
    }

    pub fn web() -> Self {
        Self::new(RuntimeMode::Web)
    }

    pub fn state(&self) -> &AppState {
        &self.state
    }
}

impl eframe::App for MikuApp {
    fn ui(&mut self, ui: &mut eframe::egui::Ui, _frame: &mut eframe::Frame) {
        eframe::egui::Panel::top("top_bar").show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Miku");
                ui.label(match self.state.runtime_mode {
                    RuntimeMode::Native => "Native",
                    RuntimeMode::Web => "Web",
                });
            });
        });

        eframe::egui::Panel::left("cluster_sidebar")
            .resizable(true)
            .show_inside(ui, |ui| {
                ui.heading("Clusters");
                ui.label("No clusters loaded yet.");
            });

        eframe::egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.heading("Kubernetes workspace");
            ui.label("Select a cluster to inspect namespaces, workloads, services, and logs.");
            ui.separator();
            ui.label(self.state.status_message());
        });

        eframe::egui::Panel::bottom("status_bar").show_inside(ui, |ui| {
            ui.label(self.state.status_message());
        });
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn run_native_app() -> eframe::Result {
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        app_title(RuntimeMode::Native),
        options,
        Box::new(|_cc| Ok(Box::new(MikuApp::native()))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_state_starts_in_empty_cluster_view() {
        let state = AppState::new(RuntimeMode::Native);

        assert_eq!(state.runtime_mode(), RuntimeMode::Native);
        assert_eq!(state.selected_cluster_name(), None);
        assert_eq!(state.status_message(), "No cluster selected");
    }

    #[test]
    fn app_title_names_runtime_mode() {
        assert_eq!(app_title(RuntimeMode::Native), "Miku - Native");
        assert_eq!(app_title(RuntimeMode::Web), "Miku - Web");
    }
}
