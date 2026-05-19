use eframe::egui;
use egui_dock::TabViewer;
use miku_api::ClusterSummary;

use crate::state::AppState;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum AppTab {
    Clusters,
    Workspace(usize),
    Inspector(usize),
}

pub(crate) struct AppTabViewer<'a> {
    pub(crate) state: &'a AppState,
    pub(crate) clusters: &'a [ClusterSummary],
    pub(crate) cluster_load_in_flight: bool,
    pub(crate) cluster_load_error: Option<&'a str>,
    pub(crate) closeable: bool,
    pub(crate) allow_windows: bool,
    pub(crate) add_tab: Option<AppTab>,
    pub(crate) add_requested: bool,
    pub(crate) new_cluster_requested: bool,
}

impl TabViewer for AppTabViewer<'_> {
    type Tab = AppTab;

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        match tab {
            AppTab::Clusters => "Clusters",
            AppTab::Workspace(1) => "Workspace",
            AppTab::Workspace(id) => return format!("Workspace {id}").into(),
            AppTab::Inspector(1) => "Inspector",
            AppTab::Inspector(id) => return format!("Inspector {id}").into(),
        }
        .into()
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        match tab {
            AppTab::Clusters => {
                ui.heading("Clusters");
                ui.separator();
                if ui
                    .add_sized(
                        [ui.available_width(), 30.0],
                        egui::Button::new("New Cluster"),
                    )
                    .clicked()
                {
                    self.new_cluster_requested = true;
                }
                ui.separator();

                if self.cluster_load_in_flight {
                    ui.label("Loading clusters...");
                } else if let Some(error) = self.cluster_load_error {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                } else if self.clusters.is_empty() {
                    ui.label("No clusters loaded yet.");
                } else {
                    for cluster in self.clusters {
                        ui.label(&cluster.name)
                            .on_hover_text(format!("Context: {}", cluster.context));
                    }
                }
            }
            AppTab::Workspace(_) => {
                ui.heading("Kubernetes workspace");
                ui.label("Select a cluster to inspect namespaces, workloads, services, and logs.");
                ui.separator();
                ui.label(self.state.status_message());
            }
            AppTab::Inspector(_) => {
                ui.heading("Inspector");
                ui.separator();
                ui.label("Select a resource to inspect details.");
            }
        }
    }

    fn is_closeable(&self, tab: &Self::Tab) -> bool {
        self.closeable && !matches!(tab, AppTab::Clusters)
    }

    fn on_add(&mut self, _path: egui_dock::NodePath) {
        self.add_requested = self.add_tab.is_some();
    }

    fn allowed_in_windows(&self, _tab: &mut Self::Tab) -> bool {
        self.allow_windows
    }
}
