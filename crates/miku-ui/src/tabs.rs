use std::collections::HashMap;

use eframe::egui;
use egui_dock::TabViewer;
use miku_api::{
    ClusterStatusReport, ClusterStatusSeverity, ClusterStatusWorkloadSummary, ClusterSummary,
};

use crate::resource_panel::{
    CustomResourcesPanel, PodAttachInputRequest, PodAttachRequest, PodLogRequest, PodResourcePanel,
    ResourceActionRequest, ResourceLoadRequest, ResourceWatchRequest,
};
use crate::resources::{RESOURCE_CATEGORIES, ResourceNavItem};
use crate::state::{AppState, ClusterConnectionState};

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
    pub(crate) cluster_connection_states: &'a HashMap<miku_core::ClusterId, ClusterConnectionState>,
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
    pub(crate) cluster_status_panel: Option<&'a mut ClusterStatusPanel>,
    pub(crate) pod_resource_panel: Option<&'a mut PodResourcePanel>,
    pub(crate) custom_resources_panel: Option<&'a mut CustomResourcesPanel>,
    pub(crate) status_load_requests: Vec<ClusterStatusLoadRequest>,
    pub(crate) resource_load_requests: Vec<ResourceLoadRequest>,
    pub(crate) resource_watch_requests: Vec<ResourceWatchRequest>,
    pub(crate) resource_action_requests: Vec<ResourceActionRequest>,
    pub(crate) pod_log_requests: Vec<PodLogRequest>,
    pub(crate) pod_attach_requests: Vec<PodAttachRequest>,
    pub(crate) pod_attach_input_requests: Vec<PodAttachInputRequest>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ClusterStatusLoadRequest {
    pub(crate) request_id: u64,
    pub(crate) cluster_id: miku_core::ClusterId,
}

#[derive(Debug, Default)]
pub(crate) struct ClusterStatusPanel {
    state: ClusterStatusPanelState,
    next_request_id: u64,
}

#[derive(Clone, Debug, Default, PartialEq)]
enum ClusterStatusPanelState {
    #[default]
    Idle,
    Loading {
        request_id: u64,
    },
    Ready {
        request_id: u64,
        report: ClusterStatusReport,
    },
    Failed {
        request_id: u64,
        error: String,
    },
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
                        self.show_cluster_card(ui, cluster);
                    }
                }
            }
            AppTab::Resources => self.show_resources(ui),
            AppTab::Workspace(_) => {
                match (
                    self.selected_cluster_id.as_ref(),
                    self.state.selected_cluster_name(),
                    self.cluster_status_panel.as_deref_mut(),
                ) {
                    (Some(cluster_id), Some(cluster_name), Some(panel)) => {
                        self.status_load_requests
                            .extend(panel.show(ui, cluster_id, cluster_name));
                    }
                    _ => {
                        ui.centered_and_justified(|ui| {
                            ui.label("Select a cluster to open its workspace.");
                        });
                    }
                }
            }
            AppTab::Resource(resource) => {
                if resource.name == "Pods" {
                    if let Some(panel) = self.pod_resource_panel.as_deref_mut() {
                        let requests = panel.show(ui, self.selected_cluster_id.as_ref());
                        self.resource_load_requests.extend(requests.loads);
                        self.resource_watch_requests.extend(requests.watches);
                        self.resource_action_requests.extend(requests.actions);
                        self.pod_log_requests.extend(requests.logs);
                        self.pod_attach_requests.extend(requests.attaches);
                        self.pod_attach_input_requests
                            .extend(requests.attach_inputs);
                    } else {
                        ui.centered_and_justified(|ui| {
                            ui.label("Pod resource panel is unavailable.");
                        });
                    }
                } else if resource.name == "Custom Resources" {
                    if let Some(panel) = self.custom_resources_panel.as_deref_mut() {
                        let requests = panel.show(ui, self.selected_cluster_id.as_ref());
                        self.resource_load_requests.extend(requests.loads);
                    } else {
                        ui.centered_and_justified(|ui| {
                            ui.label("Custom resources panel is unavailable.");
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

impl ClusterStatusPanel {
    pub(crate) fn show(
        &mut self,
        ui: &mut egui::Ui,
        cluster_id: &miku_core::ClusterId,
        cluster_name: &str,
    ) -> Vec<ClusterStatusLoadRequest> {
        let mut requests = Vec::new();
        if let Some(request) = self.request_status_if_idle(cluster_id.clone()) {
            requests.push(request);
        }

        let page_rect = ui.available_rect_before_wrap();

        ui.horizontal(|ui| {
            ui.heading(cluster_name);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button(format!(
                        "{} Refresh",
                        egui_phosphor::regular::ARROW_CLOCKWISE
                    ))
                    .clicked()
                {
                    requests.push(self.request_status(cluster_id.clone()));
                }
                match &self.state {
                    ClusterStatusPanelState::Loading { .. } => {
                        ui.add(egui::Spinner::new().size(16.0));
                        ui.label("Loading status");
                    }
                    ClusterStatusPanelState::Ready { .. } => {
                        ui.label(
                            egui::RichText::new(format!(
                                "{} Current",
                                egui_phosphor::regular::CHECK_CIRCLE
                            ))
                            .color(ui.visuals().hyperlink_color),
                        );
                    }
                    ClusterStatusPanelState::Failed { .. } => {
                        ui.label(
                            egui::RichText::new(format!(
                                "{} Status unavailable",
                                egui_phosphor::regular::WARNING_CIRCLE
                            ))
                            .color(ui.visuals().error_fg_color),
                        );
                    }
                    ClusterStatusPanelState::Idle => {}
                }
            });
        });
        ui.separator();

        match &self.state {
            ClusterStatusPanelState::Idle => {
                show_centered_in_rect(ui, page_rect, |ui| {
                    ui.label("Preparing cluster status.");
                });
            }
            ClusterStatusPanelState::Loading { .. } => {
                show_centered_in_rect(ui, page_rect, |ui| {
                    ui.horizontal(|ui| {
                        ui.add(egui::Spinner::new().size(18.0));
                        ui.label("Loading cluster status...");
                    });
                });
            }
            ClusterStatusPanelState::Ready { report, .. } => show_status_report(ui, report),
            ClusterStatusPanelState::Failed { error, .. } => {
                let error = error.clone();
                ui.vertical_centered(|ui| {
                    ui.colored_label(ui.visuals().error_fg_color, &error);
                    if ui.button("Retry").clicked() {
                        requests.push(self.request_status(cluster_id.clone()));
                    }
                });
            }
        }

        requests
    }

    pub(crate) fn apply_result(
        &mut self,
        request: &ClusterStatusLoadRequest,
        result: Result<ClusterStatusReport, String>,
    ) {
        if !matches!(
            self.state,
            ClusterStatusPanelState::Loading { request_id } if request_id == request.request_id
        ) {
            return;
        }

        self.state = match result {
            Ok(report) => ClusterStatusPanelState::Ready {
                request_id: request.request_id,
                report,
            },
            Err(error) => ClusterStatusPanelState::Failed {
                request_id: request.request_id,
                error,
            },
        };
    }

    fn request_status(&mut self, cluster_id: miku_core::ClusterId) -> ClusterStatusLoadRequest {
        self.next_request_id += 1;
        let request = ClusterStatusLoadRequest {
            request_id: self.next_request_id,
            cluster_id,
        };
        self.state = ClusterStatusPanelState::Loading {
            request_id: request.request_id,
        };
        request
    }

    fn request_status_if_idle(
        &mut self,
        cluster_id: miku_core::ClusterId,
    ) -> Option<ClusterStatusLoadRequest> {
        matches!(self.state, ClusterStatusPanelState::Idle).then(|| self.request_status(cluster_id))
    }
}

fn show_centered_in_rect<R>(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::centered_and_justified(
                egui::Direction::TopDown,
            )),
        add_contents,
    )
}

impl AppTabViewer<'_> {
    fn show_cluster_card(&mut self, ui: &mut egui::Ui, cluster: &ClusterSummary) {
        let selected = self.state.selected_cluster_id() == Some(&cluster.id);
        let connection_state = self
            .cluster_connection_states
            .get(&cluster.id)
            .unwrap_or(&ClusterConnectionState::Idle);
        let visuals = ui.visuals();
        let fill = if selected {
            visuals.selection.bg_fill
        } else {
            visuals.widgets.inactive.bg_fill
        };
        let stroke = if selected {
            visuals.selection.stroke
        } else {
            visuals.widgets.inactive.bg_stroke
        };

        let frame = egui::Frame::new()
            .fill(fill)
            .stroke(stroke)
            .corner_radius(egui::CornerRadius::same(6))
            .inner_margin(egui::Margin::symmetric(10, 8));
        let response = frame
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(&cluster.name).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        show_connection_state(ui, connection_state);
                    });
                });
            })
            .response
            .interact(egui::Sense::click());

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

        ui.add_space(6.0);
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

fn show_connection_state(ui: &mut egui::Ui, state: &ClusterConnectionState) {
    match state {
        ClusterConnectionState::Idle => {
            ui.label(
                egui::RichText::new(egui_phosphor::regular::CIRCLE)
                    .color(ui.visuals().weak_text_color()),
            )
            .on_hover_text("Not initialized");
        }
        ClusterConnectionState::Initializing => {
            ui.add(egui::Spinner::new().size(16.0))
                .on_hover_text("Initializing cluster");
        }
        ClusterConnectionState::Ready { info } => {
            let color = ui.visuals().hyperlink_color;
            let label = if info.version.is_empty() {
                egui_phosphor::regular::CHECK_CIRCLE.to_owned()
            } else {
                format!("{} {}", egui_phosphor::regular::CHECK_CIRCLE, info.version)
            };
            let hover = info
                .platform
                .as_ref()
                .map(|platform| format!("Connected\nPlatform: {platform}"))
                .unwrap_or_else(|| "Connected".to_owned());
            ui.label(egui::RichText::new(label).color(color))
                .on_hover_text(hover);
        }
        ClusterConnectionState::Failed { error } => {
            ui.label(
                egui::RichText::new(egui_phosphor::regular::WARNING_CIRCLE)
                    .color(ui.visuals().error_fg_color),
            )
            .on_hover_text(error);
        }
    }
}

fn show_status_report(ui: &mut egui::Ui, report: &ClusterStatusReport) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            metric(ui, "Version", &report.overview.version);
            metric(
                ui,
                "Platform",
                report.overview.platform.as_deref().unwrap_or("unknown"),
            );
            metric(ui, "Namespaces", &report.overview.namespaces.to_string());
            metric(
                ui,
                "Nodes",
                &format!("{}/{}", report.overview.ready_nodes, report.overview.nodes),
            );
            metric(ui, "Pods", &report.overview.pods.to_string());
            metric(
                ui,
                "Unhealthy Pods",
                &report.overview.unhealthy_pods.to_string(),
            );
        });

        ui.add_space(12.0);
        ui.columns(2, |columns| {
            show_health_conditions(&mut columns[0], &report.conditions);
            show_workloads(&mut columns[1], &report.workloads);
        });

        ui.add_space(12.0);
        show_recent_events(ui, &report.recent_events);
    });
}

fn metric(ui: &mut egui::Ui, label: &str, value: &str) {
    egui::Frame::new()
        .fill(ui.visuals().widgets.inactive.bg_fill)
        .stroke(ui.visuals().widgets.inactive.bg_stroke)
        .corner_radius(egui::CornerRadius::same(6))
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_min_width(110.0);
            ui.vertical(|ui| {
                ui.label(egui::RichText::new(label).color(ui.visuals().weak_text_color()));
                ui.label(egui::RichText::new(value).strong());
            });
        });
}

fn show_health_conditions(ui: &mut egui::Ui, conditions: &[miku_api::ClusterStatusCondition]) {
    ui.heading("Cluster Health");
    ui.separator();
    for condition in conditions {
        ui.horizontal(|ui| {
            let (icon, color) = severity_icon(ui, &condition.severity);
            ui.label(egui::RichText::new(icon).color(color));
            ui.vertical(|ui| {
                ui.label(egui::RichText::new(&condition.name).strong());
                ui.label(&condition.status);
                ui.label(
                    egui::RichText::new(&condition.message).color(ui.visuals().weak_text_color()),
                );
            });
        });
        ui.add_space(8.0);
    }
}

fn show_workloads(ui: &mut egui::Ui, workloads: &ClusterStatusWorkloadSummary) {
    ui.heading("Workloads");
    ui.separator();
    workload_row(ui, "Pods", workloads.pods);
    workload_row(ui, "Deployments", workloads.deployments);
    workload_row(ui, "Services", workloads.services);
    workload_row(ui, "Config Maps", workloads.config_maps);
    workload_row(ui, "Secrets", workloads.secrets);
}

fn workload_row(ui: &mut egui::Ui, label: &str, value: usize) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(egui::RichText::new(value.to_string()).strong());
        });
    });
}

fn show_recent_events(ui: &mut egui::Ui, events: &[miku_api::ClusterStatusEventSummary]) {
    ui.heading("Recent Events");
    ui.separator();
    if events.is_empty() {
        ui.label(egui::RichText::new("No recent events.").color(ui.visuals().weak_text_color()));
        return;
    }

    egui::ScrollArea::horizontal()
        .id_salt("cluster_status_recent_events_scroll")
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.set_min_width(860.0);
            egui::Grid::new("cluster_status_recent_events")
                .num_columns(5)
                .striped(true)
                .show(ui, |ui| {
                    ui.strong("Namespace");
                    ui.strong("Object");
                    ui.strong("Reason");
                    ui.strong("Type");
                    ui.strong("Message");
                    ui.end_row();

                    for event in events {
                        ui.label(event.namespace.as_deref().unwrap_or("-"));
                        ui.label(&event.involved_object);
                        ui.label(&event.reason);
                        ui.label(&event.event_type);
                        ui.label(&event.message);
                        ui.end_row();
                    }
                });
        });
}

fn severity_icon(ui: &egui::Ui, severity: &ClusterStatusSeverity) -> (&'static str, egui::Color32) {
    match severity {
        ClusterStatusSeverity::Ok => (
            egui_phosphor::regular::CHECK_CIRCLE,
            ui.visuals().hyperlink_color,
        ),
        ClusterStatusSeverity::Warning => (
            egui_phosphor::regular::WARNING_CIRCLE,
            ui.visuals().warn_fg_color,
        ),
        ClusterStatusSeverity::Critical => (
            egui_phosphor::regular::X_CIRCLE,
            ui.visuals().error_fg_color,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_panel_requests_status_once_when_idle() {
        let mut panel = ClusterStatusPanel::default();
        let cluster_id = miku_core::ClusterId::new("local");

        let first = panel.request_status_if_idle(cluster_id.clone());
        let second = panel.request_status_if_idle(cluster_id);

        assert!(first.is_some());
        assert!(second.is_none());
        assert!(matches!(
            panel.state,
            ClusterStatusPanelState::Loading { request_id: 1 }
        ));
    }

    #[test]
    fn status_panel_applies_ready_result_for_matching_request() {
        let mut panel = ClusterStatusPanel::default();
        let request = panel.request_status(miku_core::ClusterId::new("local"));

        panel.apply_result(&request, Ok(status_report()));

        assert!(matches!(
            &panel.state,
            ClusterStatusPanelState::Ready { report, .. }
                if report.overview.version == "v1.35.0"
        ));
    }

    #[test]
    fn status_panel_applies_failed_result_for_matching_request() {
        let mut panel = ClusterStatusPanel::default();
        let request = panel.request_status(miku_core::ClusterId::new("local"));

        panel.apply_result(&request, Err("forbidden".to_owned()));

        assert!(matches!(
            &panel.state,
            ClusterStatusPanelState::Failed { error, .. } if error == "forbidden"
        ));
    }

    #[test]
    fn status_panel_ignores_stale_results() {
        let mut panel = ClusterStatusPanel::default();
        let stale = panel.request_status(miku_core::ClusterId::new("local"));
        let current = panel.request_status(miku_core::ClusterId::new("local"));

        panel.apply_result(&stale, Ok(status_report()));

        assert!(matches!(
            panel.state,
            ClusterStatusPanelState::Loading { request_id } if request_id == current.request_id
        ));
    }

    fn status_report() -> ClusterStatusReport {
        ClusterStatusReport {
            overview: miku_api::ClusterStatusOverview {
                version: "v1.35.0".to_owned(),
                platform: Some("darwin/arm64".to_owned()),
                namespaces: 1,
                nodes: 1,
                pods: 1,
                ready_nodes: 1,
                unhealthy_pods: 0,
            },
            conditions: Vec::new(),
            workloads: ClusterStatusWorkloadSummary {
                pods: 1,
                deployments: 1,
                services: 1,
                config_maps: 1,
                secrets: 1,
            },
            recent_events: Vec::new(),
        }
    }
}
