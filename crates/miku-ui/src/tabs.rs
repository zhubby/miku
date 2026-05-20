use eframe::egui;
use egui_dock::TabViewer;
use miku_api::ClusterSummary;

use crate::resource_panel::{
    PodLogRequest, PodResourcePanel, ResourceActionRequest, ResourceLoadRequest,
};
use crate::resources::{RESOURCE_CATEGORIES, ResourceNavItem};
use crate::state::AppState;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum AppTab {
    Clusters,
    Resources,
    Workspace(usize),
    Resource(ResourceNavItem),
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
    pub(crate) selected_cluster: Option<ClusterSummary>,
    pub(crate) active_resource: Option<ResourceNavItem>,
    pub(crate) selected_resource: Option<ResourceNavItem>,
    pub(crate) selected_cluster_id: Option<miku_core::ClusterId>,
    pub(crate) pod_resource_panel: Option<&'a mut PodResourcePanel>,
    pub(crate) resource_load_requests: Vec<ResourceLoadRequest>,
    pub(crate) resource_action_requests: Vec<ResourceActionRequest>,
    pub(crate) pod_log_requests: Vec<PodLogRequest>,
}

impl TabViewer for AppTabViewer<'_> {
    type Tab = AppTab;

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        match tab {
            AppTab::Clusters => "Clusters",
            AppTab::Resources => "Resources",
            AppTab::Workspace(1) => "Workspace",
            AppTab::Workspace(id) => return format!("Workspace {id}").into(),
            AppTab::Resource(resource) => return resource.name.into(),
            AppTab::Inspector(1) => "Inspector",
            AppTab::Inspector(id) => return format!("Inspector {id}").into(),
        }
        .into()
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        match tab {
            AppTab::Clusters => {
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
                        self.show_cluster_row(ui, cluster);
                    }
                }
            }
            AppTab::Resources => self.show_resources(ui),
            AppTab::Workspace(_) => {
                if let Some(cluster_name) = self.state.selected_cluster_name() {
                    ui.heading(format!("{cluster_name} workspace"));
                    ui.label(
                        "Choose a resource to inspect namespaces, workloads, services, and logs.",
                    );
                } else {
                    ui.heading("Kubernetes workspace");
                    ui.label(
                        "Select a cluster to inspect namespaces, workloads, services, and logs.",
                    );
                }
                ui.separator();
                ui.label(self.state.status_message());
            }
            AppTab::Resource(resource) => {
                if resource.name == "Pods" {
                    if let Some(panel) = self.pod_resource_panel.as_deref_mut() {
                        let requests = panel.show(ui, self.selected_cluster_id.as_ref());
                        self.resource_load_requests.extend(requests.loads);
                        self.resource_action_requests.extend(requests.actions);
                        self.pod_log_requests.extend(requests.logs);
                    } else {
                        ui.centered_and_justified(|ui| {
                            ui.label("Pod resource panel is unavailable.");
                        });
                    }
                } else {
                    ui.centered_and_justified(|ui| {
                        ui.label(format!("{} panel is not implemented yet.", resource.name));
                    });
                }
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

impl AppTabViewer<'_> {
    fn show_cluster_row(&mut self, ui: &mut egui::Ui, cluster: &ClusterSummary) {
        let selected = self.state.selected_cluster_id() == Some(&cluster.id);
        let response = ui.selectable_label(selected, &cluster.name);

        if response.clicked() || response.double_clicked() {
            self.selected_cluster = Some(cluster.clone());
        }

        if response.secondary_clicked() {
            self.selected_cluster = Some(cluster.clone());
        }

        response.context_menu(|ui| {
            if ui.button("Select").clicked() {
                self.selected_cluster = Some(cluster.clone());
                ui.close();
            }
        });
    }

    fn show_resources(&mut self, ui: &mut egui::Ui) {
        for category in RESOURCE_CATEGORIES {
            ui.horizontal(|ui| {
                let text_color = ui.visuals().weak_text_color();
                ui.label(egui::RichText::new(category.icon).color(text_color));
                ui.label(
                    egui::RichText::new(category.name)
                        .color(text_color)
                        .strong(),
                );
            });

            for resource in category.items {
                let selected = self.active_resource == Some(*resource);
                let response = ui.selectable_label(selected, resource.name);
                if response.clicked() {
                    self.selected_resource = Some(*resource);
                }
            }
        }
    }
}
