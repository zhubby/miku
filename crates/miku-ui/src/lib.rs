use eframe::egui;
use egui_dock::{DockArea, DockState, Style, TabViewer};

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
    left_dock_state: DockState<AppTab>,
    center_dock_state: DockState<AppTab>,
    right_dock_state: DockState<AppTab>,
}

impl MikuApp {
    pub fn new(runtime_mode: RuntimeMode) -> Self {
        let left_dock_state = DockState::new(vec![AppTab::Clusters]);
        let right_dock_state = DockState::new(vec![AppTab::Inspector]);

        let center_dock_state = DockState::new(vec![AppTab::Workspace]);

        Self {
            state: AppState::new(runtime_mode),
            left_dock_state,
            center_dock_state,
            right_dock_state,
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

#[derive(Clone, Debug, Eq, PartialEq)]
enum AppTab {
    Clusters,
    Workspace,
    Inspector,
}

struct AppTabViewer<'a> {
    state: &'a AppState,
}

impl TabViewer for AppTabViewer<'_> {
    type Tab = AppTab;

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        match tab {
            AppTab::Clusters => "Clusters",
            AppTab::Workspace => "Workspace",
            AppTab::Inspector => "Inspector",
        }
        .into()
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        match tab {
            AppTab::Clusters => {
                ui.heading("Clusters");
                ui.separator();
                ui.label("No clusters loaded yet.");
            }
            AppTab::Workspace => {
                ui.heading("Kubernetes workspace");
                ui.label("Select a cluster to inspect namespaces, workloads, services, and logs.");
                ui.separator();
                ui.label(self.state.status_message());
            }
            AppTab::Inspector => {
                ui.heading("Inspector");
                ui.separator();
                ui.label("Select a resource to inspect details.");
            }
        }
    }

    fn is_closeable(&self, _tab: &Self::Tab) -> bool {
        false
    }

    fn allowed_in_windows(&self, _tab: &mut Self::Tab) -> bool {
        false
    }
}

impl eframe::App for MikuApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("menu_bar")
            .exact_size(32.0)
            .show_inside(ui, |ui| {
                egui::MenuBar::new().ui(ui, |ui| {
                    self.show_menu_bar(ui);
                });
            });

        egui::Panel::bottom("status_bar")
            .exact_size(24.0)
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(self.state.status_message());
                    ui.separator();
                    ui.label(match self.state.runtime_mode {
                        RuntimeMode::Native => "Native",
                        RuntimeMode::Web => "Web",
                    });
                });
            });

        let dock_style = Style::from_egui(ui.style().as_ref());

        egui::Panel::left("left_sidebar")
            .resizable(true)
            .default_size(220.0)
            .size_range(160.0..=360.0)
            .show_inside(ui, |ui| {
                let mut tab_viewer = AppTabViewer { state: &self.state };

                DockArea::new(&mut self.left_dock_state)
                    .id(egui::Id::new("left_sidebar_dock"))
                    .style(dock_style.clone())
                    .show_close_buttons(false)
                    .show_leaf_close_all_buttons(false)
                    .show_leaf_collapse_buttons(false)
                    .show_inside(ui, &mut tab_viewer);
            });

        egui::Panel::right("right_sidebar")
            .resizable(true)
            .default_size(260.0)
            .size_range(180.0..=420.0)
            .show_inside(ui, |ui| {
                let mut tab_viewer = AppTabViewer { state: &self.state };

                DockArea::new(&mut self.right_dock_state)
                    .id(egui::Id::new("right_sidebar_dock"))
                    .style(dock_style.clone())
                    .show_close_buttons(false)
                    .show_leaf_close_all_buttons(false)
                    .show_leaf_collapse_buttons(false)
                    .show_inside(ui, &mut tab_viewer);
            });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            let mut tab_viewer = AppTabViewer { state: &self.state };

            DockArea::new(&mut self.center_dock_state)
                .id(egui::Id::new("center_workspace_dock"))
                .style(dock_style)
                .show_close_buttons(false)
                .show_leaf_close_all_buttons(false)
                .show_leaf_collapse_buttons(false)
                .show_inside(ui, &mut tab_viewer);
        });
    }
}

impl MikuApp {
    fn show_menu_bar(&self, ui: &mut egui::Ui) {
        ui.heading("Miku");

        ui.menu_button("File", |ui| {
            if ui.button("Quit").clicked() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });

        ui.menu_button("View", |ui| {
            ui.label("Workspace");
            ui.label("Logs");
        });

        ui.add_space(8.0);
        ui.label(match self.state.runtime_mode {
            RuntimeMode::Native => "Native",
            RuntimeMode::Web => "Web",
        });

        let drag_response = ui.interact(
            ui.available_rect_before_wrap(),
            ui.id().with("title_bar_drag_region"),
            egui::Sense::click_and_drag(),
        );
        if drag_response.drag_started() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("x").clicked() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }

            if ui.button("□").clicked() {
                let maximized = ui
                    .ctx()
                    .input(|input| input.viewport().maximized.unwrap_or(false));
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
            }

            if ui.button("-").clicked() {
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::Minimized(true));
            }
        });
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn run_native_app() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_decorations(false)
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([900.0, 560.0]),
        ..Default::default()
    };

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
