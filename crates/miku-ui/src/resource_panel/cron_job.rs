use eframe::egui::{self, TextWrapMode};
use egui_extras::{Column, TableBuilder};
use miku_api::ResourceSummary;
use miku_core::ClusterId;

#[cfg(test)]
use super::ResourceLoadRequest;
use super::components::{ResourceMapEntry, ResourceMapView, ResourceYamlViewDialog};
use super::{
    LoadStatus, ResourceLoadKind, ResourcePanelRequests, ResourceUiEvent, ResourceWatchRequest,
    namespaces_from_list,
};
use crate::time::human_age_from_rfc3339;

#[derive(Clone, Debug, Default)]
pub(crate) struct CronJobResourcePanel {
    namespace_filter: Option<String>,
    search_text: String,
    namespaces: Vec<String>,
    namespace_status: LoadStatus,
    row_status: LoadStatus,
    rows: Vec<CronJobRow>,
    next_request_id: u64,
    namespace_request_id: Option<u64>,
    row_request_id: Option<u64>,
    namespace_watch_request_id: Option<u64>,
    row_watch_request_id: Option<u64>,
    last_cluster_id: Option<ClusterId>,
    describe_dialog: Option<CronJobDescribeDialog>,
    view_dialog: Option<CronJobViewDialog>,
}

impl CronJobResourcePanel {
    pub(crate) fn show(
        &mut self,
        ui: &mut egui::Ui,
        cluster_id: Option<&ClusterId>,
    ) -> ResourcePanelRequests {
        let mut requests = ResourcePanelRequests::default();
        let Some(cluster_id) = cluster_id else {
            ui.centered_and_justified(|ui| {
                ui.label("Select a cluster to load cronjobs.");
            });
            return requests;
        };

        self.reset_for_cluster_change(cluster_id);
        if matches!(self.namespace_status, LoadStatus::Idle) {
            requests
                .watches
                .push(self.request_namespace_watch(cluster_id.clone()));
        }
        if matches!(self.row_status, LoadStatus::Idle) {
            requests
                .watches
                .push(self.request_cron_job_watch(cluster_id.clone()));
        }

        self.show_toolbar(ui, cluster_id, &mut requests);
        ui.separator();
        self.show_body(ui);
        self.show_describe_dialog(ui.ctx());
        self.show_view_dialog(ui.ctx());

        requests
    }

    pub(crate) fn apply_event(&mut self, event: ResourceUiEvent) {
        match event {
            ResourceUiEvent::ResourcesLoaded { request, result } => match request.kind {
                ResourceLoadKind::Namespaces => {
                    if self.namespace_request_id == Some(request.request_id) {
                        self.namespace_request_id = None;
                    }
                    match result {
                        Ok(list) => {
                            self.namespaces = namespaces_from_list(&list);
                            self.namespace_status = LoadStatus::Loaded;
                        }
                        Err(error) => self.namespace_status = LoadStatus::Error(error),
                    }
                }
                ResourceLoadKind::CronJobs { .. } => {
                    if self.row_request_id != Some(request.request_id) {
                        return;
                    }
                    self.row_request_id = None;
                    match result {
                        Ok(list) => {
                            self.rows = cron_job_rows_from_list(&list.items);
                            self.row_status = LoadStatus::Loaded;
                        }
                        Err(error) => self.row_status = LoadStatus::Error(error),
                    }
                }
                ResourceLoadKind::Nodes
                | ResourceLoadKind::ConfigMaps { .. }
                | ResourceLoadKind::EndpointSlices { .. }
                | ResourceLoadKind::Endpoints { .. }
                | ResourceLoadKind::IngressClasses
                | ResourceLoadKind::Ingresses { .. }
                | ResourceLoadKind::NetworkPolicies { .. }
                | ResourceLoadKind::Jobs { .. }
                | ResourceLoadKind::Deployments { .. }
                | ResourceLoadKind::Events { .. }
                | ResourceLoadKind::LimitRanges { .. }
                | ResourceLoadKind::DaemonSets { .. }
                | ResourceLoadKind::StatefulSets { .. }
                | ResourceLoadKind::ReplicaSets { .. }
                | ResourceLoadKind::ResourceQuotas { .. }
                | ResourceLoadKind::Secrets { .. }
                | ResourceLoadKind::Services { .. }
                | ResourceLoadKind::Pods { .. }
                | ResourceLoadKind::PersistentVolumeClaims { .. }
                | ResourceLoadKind::PersistentVolumes
                | ResourceLoadKind::StorageClasses
                | ResourceLoadKind::ClusterRoleBindings
                | ResourceLoadKind::ClusterRoles
                | ResourceLoadKind::RoleBindings { .. }
                | ResourceLoadKind::Roles { .. }
                | ResourceLoadKind::ServiceAccounts { .. }
                | ResourceLoadKind::HorizontalPodAutoscalers { .. }
                | ResourceLoadKind::PodDisruptionBudgets { .. }
                | ResourceLoadKind::PriorityClasses
                | ResourceLoadKind::RuntimeClasses
                | ResourceLoadKind::Leases { .. }
                | ResourceLoadKind::MutatingWebhookConfigurations
                | ResourceLoadKind::ValidatingWebhookConfigurations
                | ResourceLoadKind::CustomResourceDefinitions => {}
            },
            ResourceUiEvent::ResourceWatchUpdated { request, result } => match request.kind {
                ResourceLoadKind::Namespaces => {
                    if self.namespace_watch_request_id == Some(request.request_id) {
                        self.namespace_watch_request_id = None;
                    }
                    match result {
                        Ok(miku_api::ResourceEvent::Snapshot(list)) => {
                            self.namespaces = namespaces_from_list(&list);
                            self.namespace_status = LoadStatus::Loaded;
                        }
                        Ok(_) => {}
                        Err(error) => self.namespace_status = LoadStatus::Error(error),
                    }
                }
                ResourceLoadKind::CronJobs { .. } => {
                    if self.row_watch_request_id != Some(request.request_id) {
                        return;
                    }
                    match result {
                        Ok(miku_api::ResourceEvent::Snapshot(list)) => {
                            self.rows = cron_job_rows_from_list(&list.items);
                            self.row_status = LoadStatus::Loaded;
                        }
                        Ok(_) => {}
                        Err(error) => self.row_status = LoadStatus::Error(error),
                    }
                }
                ResourceLoadKind::Nodes
                | ResourceLoadKind::ConfigMaps { .. }
                | ResourceLoadKind::EndpointSlices { .. }
                | ResourceLoadKind::Endpoints { .. }
                | ResourceLoadKind::IngressClasses
                | ResourceLoadKind::Ingresses { .. }
                | ResourceLoadKind::NetworkPolicies { .. }
                | ResourceLoadKind::Jobs { .. }
                | ResourceLoadKind::Deployments { .. }
                | ResourceLoadKind::Events { .. }
                | ResourceLoadKind::LimitRanges { .. }
                | ResourceLoadKind::DaemonSets { .. }
                | ResourceLoadKind::StatefulSets { .. }
                | ResourceLoadKind::ReplicaSets { .. }
                | ResourceLoadKind::ResourceQuotas { .. }
                | ResourceLoadKind::Secrets { .. }
                | ResourceLoadKind::Services { .. }
                | ResourceLoadKind::Pods { .. }
                | ResourceLoadKind::PersistentVolumeClaims { .. }
                | ResourceLoadKind::PersistentVolumes
                | ResourceLoadKind::StorageClasses
                | ResourceLoadKind::ClusterRoleBindings
                | ResourceLoadKind::ClusterRoles
                | ResourceLoadKind::RoleBindings { .. }
                | ResourceLoadKind::Roles { .. }
                | ResourceLoadKind::ServiceAccounts { .. }
                | ResourceLoadKind::HorizontalPodAutoscalers { .. }
                | ResourceLoadKind::PodDisruptionBudgets { .. }
                | ResourceLoadKind::PriorityClasses
                | ResourceLoadKind::RuntimeClasses
                | ResourceLoadKind::Leases { .. }
                | ResourceLoadKind::MutatingWebhookConfigurations
                | ResourceLoadKind::ValidatingWebhookConfigurations
                | ResourceLoadKind::CustomResourceDefinitions => {}
            },
            ResourceUiEvent::ResourceActionCompleted { .. }
            | ResourceUiEvent::PodLogsLoaded { .. }
            | ResourceUiEvent::PodAttachConnected { .. }
            | ResourceUiEvent::PodAttachOutput { .. } => {}
        }
    }

    fn reset_for_cluster_change(&mut self, cluster_id: &ClusterId) {
        if self.last_cluster_id.as_ref() == Some(cluster_id) {
            return;
        }

        self.last_cluster_id = Some(cluster_id.clone());
        self.namespace_filter = None;
        self.search_text.clear();
        self.namespaces.clear();
        self.rows.clear();
        self.namespace_status = LoadStatus::Idle;
        self.row_status = LoadStatus::Idle;
        self.namespace_request_id = None;
        self.row_request_id = None;
        self.namespace_watch_request_id = None;
        self.row_watch_request_id = None;
        self.describe_dialog = None;
        self.view_dialog = None;
    }

    fn show_toolbar(
        &mut self,
        ui: &mut egui::Ui,
        cluster_id: &ClusterId,
        requests: &mut ResourcePanelRequests,
    ) {
        ui.horizontal(|ui| {
            let selected_label = self
                .namespace_filter
                .as_deref()
                .unwrap_or("All namespaces")
                .to_owned();

            let mut namespace_changed = false;
            egui::ComboBox::from_id_salt("cron_job_resource_namespace_filter")
                .selected_text(selected_label)
                .width(220.0)
                .show_ui(ui, |ui| {
                    namespace_changed |= ui
                        .selectable_value(&mut self.namespace_filter, None, "All namespaces")
                        .changed();
                    for namespace in &self.namespaces {
                        namespace_changed |= ui
                            .selectable_value(
                                &mut self.namespace_filter,
                                Some(namespace.clone()),
                                namespace,
                            )
                            .changed();
                    }
                });

            ui.add(
                egui::TextEdit::singleline(&mut self.search_text)
                    .hint_text("Search CronJobs...")
                    .desired_width(280.0),
            );

            if ui
                .button(egui_phosphor::regular::ARROWS_CLOCKWISE)
                .on_hover_text("Refresh")
                .clicked()
            {
                requests
                    .watches
                    .push(self.request_namespace_watch(cluster_id.clone()));
                requests
                    .watches
                    .push(self.request_cron_job_watch(cluster_id.clone()));
            }

            ui.separator();
            ui.label(format!("{} items", self.filtered_row_count()));

            if matches!(self.row_status, LoadStatus::Loading) {
                ui.label("Loading...");
            }

            if matches!(self.namespace_status, LoadStatus::Error(_)) {
                ui.colored_label(ui.visuals().error_fg_color, "Namespaces unavailable");
            }

            if namespace_changed {
                requests
                    .watches
                    .push(self.request_cron_job_watch(cluster_id.clone()));
            }
        });
    }

    fn show_body(&mut self, ui: &mut egui::Ui) {
        match &self.row_status {
            LoadStatus::Idle | LoadStatus::Loading if self.rows.is_empty() => {
                ui.centered_and_justified(|ui| {
                    ui.label("Loading cronjobs...");
                });
            }
            LoadStatus::Error(error) => {
                ui.centered_and_justified(|ui| {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                });
            }
            _ => {
                let row_indices = self.filtered_row_indices();
                if row_indices.is_empty() {
                    ui.centered_and_justified(|ui| {
                        ui.label("No cronjobs match the current filters.");
                    });
                    return;
                }

                let action = show_cron_job_table(ui, &self.rows, row_indices);
                self.apply_table_action(action);
            }
        }
    }

    fn apply_table_action(&mut self, action: Option<CronJobTableAction>) {
        match action {
            Some(CronJobTableAction::Describe { key }) => {
                let Some((name, describe)) = self
                    .row_by_key(&key)
                    .map(|row| (row.name.clone(), cron_job_describe_from_row(row)))
                else {
                    return;
                };
                self.describe_dialog = Some(CronJobDescribeDialog {
                    key,
                    name,
                    describe,
                });
            }
            Some(CronJobTableAction::View { key }) => {
                let Some((name, yaml)) = self
                    .row_by_key(&key)
                    .map(|row| (row.name.clone(), full_manifest_yaml(&row.raw)))
                else {
                    return;
                };
                self.view_dialog = Some(CronJobViewDialog { key, name, yaml });
            }
            None => {}
        }
    }

    fn show_describe_dialog(&mut self, ctx: &egui::Context) {
        let Some(dialog) = self.describe_dialog.as_ref() else {
            return;
        };

        let mut open = true;
        egui::Window::new(format!("Describe {}", dialog.name))
            .id(egui::Id::new(("cron_job-describe-dialog", &dialog.key)))
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .open(&mut open)
            .collapsible(false)
            .fixed_size([
                CRON_JOB_DESCRIBE_DIALOG_WIDTH,
                CRON_JOB_DESCRIBE_DIALOG_HEIGHT,
            ])
            .show(ctx, |ui| {
                ui.set_width(CRON_JOB_DESCRIBE_DIALOG_WIDTH);
                ui.set_height(CRON_JOB_DESCRIBE_CONTENT_HEIGHT);
                egui::ScrollArea::both()
                    .id_salt(("cron_job-describe-content", &dialog.key))
                    .max_width(CRON_JOB_DESCRIBE_DIALOG_WIDTH)
                    .max_height(CRON_JOB_DESCRIBE_CONTENT_HEIGHT)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_min_width(CRON_JOB_DESCRIBE_CONTENT_WIDTH);
                        ui.style_mut().wrap_mode = Some(TextWrapMode::Extend);
                        show_cron_job_describe(ui, &dialog.describe);
                    });
            });

        if !open {
            self.describe_dialog = None;
        }
    }

    fn show_view_dialog(&mut self, ctx: &egui::Context) {
        let Some(dialog) = self.view_dialog.as_ref() else {
            return;
        };

        let mut open = true;
        let response = ResourceYamlViewDialog {
            id: egui::Id::new(("cron_job-view-dialog", &dialog.key)),
            title: format!("View {}", dialog.name),
            yaml: &dialog.yaml,
            open: &mut open,
        }
        .show(ctx);

        if !response.open {
            self.view_dialog = None;
        }
    }

    #[cfg(test)]
    fn request_cronjobs(&mut self, cluster_id: ClusterId) -> ResourceLoadRequest {
        let request = ResourceLoadRequest {
            request_id: self.allocate_request_id(),
            cluster_id,
            kind: ResourceLoadKind::CronJobs {
                namespace: self.namespace_filter.clone(),
            },
        };
        self.row_request_id = Some(request.request_id);
        self.row_status = LoadStatus::Loading;
        request
    }

    fn request_namespace_watch(&mut self, cluster_id: ClusterId) -> ResourceWatchRequest {
        let request = ResourceWatchRequest {
            request_id: self.allocate_request_id(),
            cluster_id,
            kind: ResourceLoadKind::Namespaces,
        };
        self.namespace_watch_request_id = Some(request.request_id);
        self.namespace_status = LoadStatus::Loading;
        request
    }

    fn request_cron_job_watch(&mut self, cluster_id: ClusterId) -> ResourceWatchRequest {
        let request = ResourceWatchRequest {
            request_id: self.allocate_request_id(),
            cluster_id,
            kind: ResourceLoadKind::CronJobs {
                namespace: self.namespace_filter.clone(),
            },
        };
        self.row_watch_request_id = Some(request.request_id);
        self.row_status = LoadStatus::Loading;
        request
    }

    fn allocate_request_id(&mut self) -> u64 {
        self.next_request_id += 1;
        self.next_request_id
    }

    fn filtered_row_count(&self) -> usize {
        self.rows
            .iter()
            .filter(|row| row_matches_search(row, &self.search_text))
            .count()
    }

    fn filtered_row_indices(&self) -> Vec<usize> {
        self.rows
            .iter()
            .enumerate()
            .filter_map(|(index, row)| row_matches_search(row, &self.search_text).then_some(index))
            .collect()
    }

    fn row_by_key(&self, key: &str) -> Option<&CronJobRow> {
        self.rows.iter().find(|row| row.key == key)
    }
}

fn show_cron_job_table(
    ui: &mut egui::Ui,
    rows: &[CronJobRow],
    row_indices: Vec<usize>,
) -> Option<CronJobTableAction> {
    let row_height = ui.spacing().interact_size.y;
    let table_width: f32 = CRON_JOB_COLUMN_WIDTHS.iter().sum::<f32>()
        + ui.spacing().item_spacing.x * CRON_JOB_COLUMN_WIDTHS.len().saturating_sub(1) as f32;
    let mut action = None;

    egui::ScrollArea::horizontal()
        .id_salt("cron_job_resource_table_horizontal")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.set_min_width(table_width);

            let mut table = TableBuilder::new(ui)
                .id_salt("cron_job_resource_table")
                .striped(true)
                .resizable(false)
                .sense(egui::Sense::click())
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .min_scrolled_height(0.0);

            for width in CRON_JOB_COLUMN_WIDTHS {
                table = table.column(Column::exact(width));
            }

            table
                .header(row_height, |mut header| {
                    for label in CRON_JOB_COLUMNS {
                        header.col(|ui| {
                            ui.strong(label);
                        });
                    }
                })
                .body(|body| {
                    body.rows(row_height, row_indices.len(), |mut table_row| {
                        let row_index = table_row.index();
                        let Some(row) = row_indices
                            .get(row_index)
                            .and_then(|index| rows.get(*index))
                        else {
                            return;
                        };

                        table_row.col(|ui| {
                            ui.label(&row.name);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.namespace);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.schedule);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.suspend);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.active);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.last_schedule);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.last_success);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.concurrency);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.age);
                        });

                        table_row.response().context_menu(|ui| {
                            if ui
                                .button(format!("{} Describe", egui_phosphor::regular::INFO))
                                .clicked()
                            {
                                action = Some(CronJobTableAction::Describe {
                                    key: row.key.clone(),
                                });
                                ui.close();
                            }
                            if ui
                                .button(format!("{} View", egui_phosphor::regular::EYE))
                                .clicked()
                            {
                                action = Some(CronJobTableAction::View {
                                    key: row.key.clone(),
                                });
                                ui.close();
                            }
                        });
                    });
                });
        });

    action
}

const CRON_JOB_COLUMNS: [&str; 9] = [
    "Name",
    "Namespace",
    "Schedule",
    "Suspend",
    "Active",
    "Last Schedule",
    "Last Success",
    "Concurrency",
    "Age",
];
const CRON_JOB_COLUMN_WIDTHS: [f32; 9] =
    [240.0, 160.0, 180.0, 90.0, 90.0, 180.0, 180.0, 120.0, 90.0];
const CRON_JOB_DESCRIBE_DIALOG_WIDTH: f32 = 860.0;
const CRON_JOB_DESCRIBE_DIALOG_HEIGHT: f32 = 580.0;
const CRON_JOB_DESCRIBE_CONTENT_HEIGHT: f32 = 520.0;
const CRON_JOB_DESCRIBE_CONTENT_WIDTH: f32 = 1160.0;
const CRON_JOB_DESCRIBE_SECTION_WIDTH: f32 = 1128.0;
const CRON_JOB_DESCRIBE_FIELD_LABEL_WIDTH: f32 = 140.0;
const CRON_JOB_DESCRIBE_FIELD_VALUE_WIDTH: f32 = 370.0;
const CRON_JOB_DESCRIBE_LINE_WIDTH: f32 = 1080.0;

#[cfg(test)]
fn filter_cron_job_rows<'a>(rows: &'a [CronJobRow], search_text: &str) -> Vec<&'a CronJobRow> {
    rows.iter()
        .filter(|row| row_matches_search(row, search_text))
        .collect()
}

fn row_matches_search(row: &CronJobRow, search_text: &str) -> bool {
    let needle = search_text.trim().to_lowercase();
    needle.is_empty()
        || row.name.to_lowercase().contains(&needle)
        || row.namespace.to_lowercase().contains(&needle)
        || row.schedule.to_lowercase().contains(&needle)
        || row.suspend.to_lowercase().contains(&needle)
        || row.concurrency.to_lowercase().contains(&needle)
        || row.status_summary.to_lowercase().contains(&needle)
}

fn cron_job_rows_from_list(items: &[ResourceSummary]) -> Vec<CronJobRow> {
    let mut rows = items
        .iter()
        .map(CronJobRow::from_summary)
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        left.namespace
            .cmp(&right.namespace)
            .then(left.name.cmp(&right.name))
    });
    rows
}

#[derive(Clone, Debug, PartialEq)]
struct CronJobRow {
    key: String,
    name: String,
    namespace: String,
    schedule: String,
    suspend: String,
    active: String,
    last_schedule: String,
    last_success: String,
    concurrency: String,
    status_summary: String,
    age: String,
    raw: serde_json::Value,
}

impl CronJobRow {
    fn from_summary(summary: &ResourceSummary) -> Self {
        let raw = &summary.raw;
        let name = value_str(raw, &["metadata", "name"]).unwrap_or(&summary.name);
        let namespace = value_str(raw, &["metadata", "namespace"])
            .or(summary.namespace.as_deref())
            .unwrap_or("N/A");
        let schedule = value_str(raw, &["spec", "schedule"]).unwrap_or("N/A");
        let suspend = value_bool(raw, &["spec", "suspend"])
            .map_or_else(|| "N/A".to_owned(), |value| value.to_string());
        let active = raw
            .pointer("/status/active")
            .and_then(serde_json::Value::as_array)
            .map_or(0, Vec::len);
        let last_schedule = value_str(raw, &["status", "lastScheduleTime"]).unwrap_or("N/A");
        let last_success = value_str(raw, &["status", "lastSuccessfulTime"]).unwrap_or("N/A");
        let concurrency = value_str(raw, &["spec", "concurrencyPolicy"]).unwrap_or("N/A");

        Self {
            key: cron_job_key(namespace, name),
            name: name.to_owned(),
            namespace: namespace.to_owned(),
            schedule: schedule.to_owned(),
            suspend,
            active: active.to_string(),
            last_schedule: last_schedule.to_owned(),
            last_success: last_success.to_owned(),
            concurrency: concurrency.to_owned(),
            status_summary: cron_job_status_summary(schedule, active, concurrency),
            age: value_str(raw, &["metadata", "creationTimestamp"])
                .map(|timestamp| {
                    human_age_from_rfc3339(timestamp).unwrap_or_else(|| timestamp.to_owned())
                })
                .unwrap_or_else(|| "N/A".to_owned()),
            raw: summary.raw.clone(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum CronJobTableAction {
    Describe { key: String },
    View { key: String },
}

#[derive(Clone, Debug, PartialEq)]
struct CronJobDescribeDialog {
    key: String,
    name: String,
    describe: CronJobDescribe,
}

#[derive(Clone, Debug, PartialEq)]
struct CronJobViewDialog {
    key: String,
    name: String,
    yaml: String,
}

#[derive(Clone, Debug, PartialEq)]
struct CronJobDescribe {
    summary: Vec<DescribeField>,
    replicas: Vec<DescribeField>,
    rollout: Vec<DescribeField>,
    selector: Vec<ResourceMapEntry>,
    template_labels: Vec<ResourceMapEntry>,
    containers: Vec<ContainerDescribe>,
    conditions: Vec<CronJobConditionDescribe>,
    labels: Vec<ResourceMapEntry>,
    annotations: Vec<ResourceMapEntry>,
    raw_yaml: String,
}

#[derive(Clone, Debug, PartialEq)]
struct ContainerDescribe {
    name: String,
    image: String,
}

#[derive(Clone, Debug, PartialEq)]
struct CronJobConditionDescribe {
    condition_type: String,
    status: String,
    reason: String,
    message: String,
}

#[derive(Clone, Debug, PartialEq)]
struct DescribeField {
    label: String,
    value: String,
}

fn show_cron_job_describe(ui: &mut egui::Ui, describe: &CronJobDescribe) {
    describe_group(ui, egui_phosphor::regular::STACK, "CronJob", |ui| {
        describe_fields(ui, &describe.summary);
    });

    ui.add_space(10.0);
    describe_group(ui, egui_phosphor::regular::GAUGE, "Schedule", |ui| {
        describe_fields(ui, &describe.replicas);
    });

    ui.add_space(10.0);
    describe_group(
        ui,
        egui_phosphor::regular::ARROWS_CLOCKWISE,
        "Status",
        |ui| {
            describe_fields(ui, &describe.rollout);
        },
    );

    ui.add_space(10.0);
    describe_group(ui, egui_phosphor::regular::FUNNEL, "Selector", |ui| {
        ResourceMapView {
            id_salt: "cron_job-describe-selector",
            icon: egui_phosphor::regular::FUNNEL,
            title: "Match labels",
            entries: &describe.selector,
            empty_label: "No selector labels.",
        }
        .show(ui);
    });

    ui.add_space(10.0);
    describe_group(ui, egui_phosphor::regular::CUBE, "Pod template", |ui| {
        ResourceMapView {
            id_salt: "cron_job-describe-template-labels",
            icon: egui_phosphor::regular::TAG,
            title: "Labels",
            entries: &describe.template_labels,
            empty_label: "No template labels.",
        }
        .show(ui);
        ui.add_space(8.0);
        if describe.containers.is_empty() {
            non_wrapping_value(ui, "N/A", CRON_JOB_DESCRIBE_LINE_WIDTH);
        } else {
            for container in &describe.containers {
                non_wrapping_value(
                    ui,
                    &format!("{}: {}", container.name, container.image),
                    CRON_JOB_DESCRIBE_LINE_WIDTH,
                );
            }
        }
    });

    ui.add_space(10.0);
    describe_group(
        ui,
        egui_phosphor::regular::CHECK_CIRCLE,
        "Conditions",
        |ui| {
            if describe.conditions.is_empty() {
                non_wrapping_value(ui, "N/A", CRON_JOB_DESCRIBE_LINE_WIDTH);
            } else {
                egui::Grid::new("cron_job-describe-conditions")
                    .num_columns(4)
                    .spacing([18.0, 4.0])
                    .striped(true)
                    .show(ui, |ui| {
                        ui.strong("Type");
                        ui.strong("Status");
                        ui.strong("Reason");
                        ui.strong("Message");
                        ui.end_row();
                        for condition in &describe.conditions {
                            non_wrapping_value(ui, &condition.condition_type, 180.0);
                            ui.colored_label(
                                condition_color(ui, &condition.status),
                                &condition.status,
                            );
                            non_wrapping_value(ui, &condition.reason, 220.0);
                            non_wrapping_value(ui, &condition.message, 520.0);
                            ui.end_row();
                        }
                    });
            }
        },
    );

    ui.add_space(10.0);
    describe_group(ui, egui_phosphor::regular::TAG, "Metadata", |ui| {
        ResourceMapView {
            id_salt: "cron_job-describe-labels",
            icon: egui_phosphor::regular::TAG,
            title: "Labels",
            entries: &describe.labels,
            empty_label: "No labels.",
        }
        .show(ui);
        ui.add_space(8.0);
        ResourceMapView {
            id_salt: "cron_job-describe-annotations",
            icon: egui_phosphor::regular::NOTE,
            title: "Annotations",
            entries: &describe.annotations,
            empty_label: "No annotations.",
        }
        .show(ui);
    });

    ui.add_space(10.0);
    describe_group(ui, egui_phosphor::regular::CODE, "Raw manifest", |ui| {
        egui::ScrollArea::both()
            .id_salt("cron_job-describe-raw-manifest-content")
            .max_height(180.0)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add(
                    egui::Label::new(egui::RichText::new(&describe.raw_yaml).monospace())
                        .wrap_mode(TextWrapMode::Extend)
                        .selectable(true),
                );
            });
    });
}

fn describe_group(
    ui: &mut egui::Ui,
    icon: &str,
    title: &str,
    contents: impl FnOnce(&mut egui::Ui),
) {
    egui::Frame::new()
        .fill(ui.visuals().extreme_bg_color)
        .stroke(egui::Stroke::new(
            1.0,
            ui.visuals().widgets.noninteractive.bg_stroke.color,
        ))
        .corner_radius(egui::CornerRadius::same(4))
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_min_width(CRON_JOB_DESCRIBE_SECTION_WIDTH);
            ui.horizontal(|ui| {
                ui.label(icon);
                ui.strong(title);
            });
            ui.separator();
            contents(ui);
        });
}

fn describe_fields(ui: &mut egui::Ui, fields: &[DescribeField]) {
    egui::Grid::new(ui.next_auto_id())
        .num_columns(4)
        .spacing([16.0, 4.0])
        .show(ui, |ui| {
            for chunk in fields.chunks(2) {
                for field in chunk {
                    ui.add_sized(
                        [CRON_JOB_DESCRIBE_FIELD_LABEL_WIDTH, 0.0],
                        egui::Label::new(egui::RichText::new(&field.label).weak())
                            .wrap_mode(TextWrapMode::Extend),
                    );
                    non_wrapping_value(ui, &field.value, CRON_JOB_DESCRIBE_FIELD_VALUE_WIDTH);
                }
                if chunk.len() == 1 {
                    ui.label("");
                    ui.label("");
                }
                ui.end_row();
            }
        });
}

fn non_wrapping_value(ui: &mut egui::Ui, value: &str, width: f32) {
    ui.add_sized(
        [width, 0.0],
        egui::Label::new(value)
            .wrap_mode(TextWrapMode::Extend)
            .selectable(true),
    );
}

fn condition_color(ui: &egui::Ui, status: &str) -> egui::Color32 {
    match status {
        "True" | "Available" => egui::Color32::from_rgb(46, 160, 67),
        "False" | "Progressing" => egui::Color32::from_rgb(191, 135, 0),
        "Unknown" => ui.visuals().error_fg_color,
        _ => ui.visuals().text_color(),
    }
}

impl DescribeField {
    fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
        }
    }
}

fn cron_job_describe_from_row(row: &CronJobRow) -> CronJobDescribe {
    let raw = &row.raw;
    CronJobDescribe {
        summary: vec![
            DescribeField::new("Name", row.name.clone()),
            DescribeField::new("Namespace", row.namespace.clone()),
            DescribeField::new("Age", row.age.clone()),
            DescribeField::new("Concurrency", row.concurrency.clone()),
        ],
        replicas: vec![
            DescribeField::new("Schedule", row.schedule.clone()),
            DescribeField::new("Suspend", row.suspend.clone()),
            DescribeField::new("Last schedule", row.last_schedule.clone()),
            DescribeField::new("Last success", row.last_success.clone()),
        ],
        rollout: vec![
            DescribeField::new("Active", row.active.clone()),
            DescribeField::new(
                "Successful history",
                value_u64(raw, &["spec", "successfulJobsHistoryLimit"])
                    .map_or_else(|| "N/A".to_owned(), |value| value.to_string()),
            ),
            DescribeField::new(
                "Failed history",
                value_u64(raw, &["spec", "failedJobsHistoryLimit"])
                    .map_or_else(|| "N/A".to_owned(), |value| value.to_string()),
            ),
            DescribeField::new(
                "Starting deadline",
                value_u64(raw, &["spec", "startingDeadlineSeconds"])
                    .map_or_else(|| "N/A".to_owned(), |value| value.to_string()),
            ),
        ],
        selector: string_map_entries(raw.pointer("/spec/jobTemplate/spec/selector/matchLabels")),
        template_labels: string_map_entries(
            raw.pointer("/spec/jobTemplate/spec/template/metadata/labels"),
        ),
        containers: cron_job_containers(raw),
        conditions: cron_job_condition_describes(raw),
        labels: string_map_entries(raw.pointer("/metadata/labels")),
        annotations: string_map_entries(raw.pointer("/metadata/annotations")),
        raw_yaml: full_manifest_yaml(raw),
    }
}

fn cron_job_key(namespace: &str, name: &str) -> String {
    format!("{namespace}/{name}")
}

fn cron_job_status_summary(schedule: &str, active: usize, concurrency: &str) -> String {
    format!("schedule={schedule}, active={active}, concurrency={concurrency}")
}

fn cron_job_containers(raw: &serde_json::Value) -> Vec<ContainerDescribe> {
    raw.pointer("/spec/jobTemplate/spec/template/spec/containers")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .map(|container| ContainerDescribe {
            name: value_str(container, &["name"]).unwrap_or("N/A").to_owned(),
            image: value_str(container, &["image"]).unwrap_or("N/A").to_owned(),
        })
        .collect()
}

fn cron_job_condition_describes(raw: &serde_json::Value) -> Vec<CronJobConditionDescribe> {
    raw.pointer("/status/conditions")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .map(|condition| CronJobConditionDescribe {
            condition_type: value_str(condition, &["type"]).unwrap_or("N/A").to_owned(),
            status: value_str(condition, &["status"])
                .unwrap_or("N/A")
                .to_owned(),
            reason: value_str(condition, &["reason"])
                .unwrap_or("N/A")
                .to_owned(),
            message: value_str(condition, &["message"])
                .unwrap_or("N/A")
                .to_owned(),
        })
        .collect()
}

fn string_map_entries(value: Option<&serde_json::Value>) -> Vec<ResourceMapEntry> {
    let mut entries = value
        .and_then(serde_json::Value::as_object)
        .into_iter()
        .flat_map(|object| {
            object.iter().map(|(key, value)| {
                let value = value
                    .as_str()
                    .map_or_else(|| value.to_string(), ToOwned::to_owned);
                ResourceMapEntry::new(key, value)
            })
        })
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| left.key.cmp(&right.key));
    entries
}

fn full_manifest_yaml(raw: &serde_json::Value) -> String {
    serde_yaml::to_string(raw)
        .or_else(|_| serde_json::to_string_pretty(raw))
        .unwrap_or_default()
}

fn value_str<'a>(value: &'a serde_json::Value, path: &[&str]) -> Option<&'a str> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    current.as_str()
}

fn value_u64(value: &serde_json::Value, path: &[&str]) -> Option<u64> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    current.as_u64()
}

fn value_bool(value: &serde_json::Value, path: &[&str]) -> Option<bool> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    current.as_bool()
}

#[cfg(test)]
mod tests {
    use super::*;
    use miku_api::ResourceList;

    #[test]
    fn cron_job_request_query_uses_selected_namespace() {
        let mut panel = CronJobResourcePanel {
            namespace_filter: Some("production".to_owned()),
            ..CronJobResourcePanel::default()
        };

        let request = panel.request_cronjobs(ClusterId::new("local"));
        let query = request.query();

        assert_eq!(query.resource.plural, "cronjobs");
        assert_eq!(query.resource.group.as_deref(), Some("batch"));
        assert_eq!(query.namespace.as_deref(), Some("production"));
    }

    #[test]
    fn cron_job_row_extracts_table_fields_from_raw_summary() {
        let row = CronJobRow::from_summary(&cron_job_summary());

        assert_eq!(row.name, "backup");
        assert_eq!(row.namespace, "default");
        assert_eq!(row.schedule, "*/5 * * * *");
        assert_eq!(row.suspend, "false");
        assert_eq!(row.active, "2");
        assert_eq!(row.last_schedule, "2026-05-18T10:00:00Z");
        assert_eq!(row.last_success, "2026-05-18T09:55:00Z");
        assert_eq!(row.concurrency, "Forbid");
        assert_eq!(
            row.status_summary,
            "schedule=*/5 * * * *, active=2, concurrency=Forbid"
        );
        assert!(row.age.ends_with(" ago"));
    }

    #[test]
    fn cron_job_row_handles_missing_optional_fields() {
        let row = CronJobRow::from_summary(&ResourceSummary {
            name: "minimal".to_owned(),
            namespace: Some("default".to_owned()),
            kind: "CronJob".to_owned(),
            status: None,
            raw: serde_json::json!({"metadata": {"name": "minimal", "namespace": "default"}}),
        });

        assert_eq!(row.schedule, "N/A");
        assert_eq!(row.suspend, "N/A");
        assert_eq!(row.active, "0");
        assert_eq!(row.last_schedule, "N/A");
        assert_eq!(row.last_success, "N/A");
        assert_eq!(row.concurrency, "N/A");
    }

    #[test]
    fn cron_job_rows_filter_by_multiple_fields_case_insensitively() {
        let rows = vec![
            CronJobRow::from_summary(&cron_job_summary()),
            CronJobRow::from_summary(&cron_job_summary_with_name("production", "worker")),
        ];

        assert_eq!(filter_cron_job_rows(&rows, "BACKUP").len(), 1);
        assert_eq!(filter_cron_job_rows(&rows, "PRODUCTION").len(), 1);
        assert_eq!(filter_cron_job_rows(&rows, "FORBID").len(), 2);
        assert_eq!(filter_cron_job_rows(&rows, "*/5").len(), 2);
    }

    #[test]
    fn cron_job_rows_are_sorted_by_namespace_and_name() {
        let rows = cron_job_rows_from_list(&[
            cron_job_summary_with_name("zeta", "worker"),
            cron_job_summary_with_name("default", "backup"),
            cron_job_summary_with_name("default", "archive"),
        ]);

        let keys = rows.into_iter().map(|row| row.key).collect::<Vec<_>>();
        assert_eq!(
            keys,
            vec!["default/archive", "default/backup", "zeta/worker"]
        );
    }

    #[test]
    fn cron_job_describe_extracts_details() {
        let row = CronJobRow::from_summary(&cron_job_summary());
        let describe = cron_job_describe_from_row(&row);

        assert_eq!(describe.template_labels.len(), 1);
        assert_eq!(describe.containers.len(), 1);
        assert!(describe.labels.iter().any(|entry| entry.key == "app"));
        assert!(
            describe
                .summary
                .iter()
                .any(|field| field.label == "Concurrency" && field.value == "Forbid")
        );
    }

    #[test]
    fn stale_resource_events_do_not_replace_current_rows() {
        let mut panel = CronJobResourcePanel::default();
        let cluster_id = ClusterId::new("local");
        let first = panel.request_cronjobs(cluster_id.clone());
        let second = panel.request_cronjobs(cluster_id);

        panel.apply_event(ResourceUiEvent::ResourcesLoaded {
            request: first,
            result: Ok(ResourceList {
                items: vec![cron_job_summary_with_name("default", "stale")],
                continue_token: None,
            }),
        });
        assert!(panel.rows.is_empty());

        panel.apply_event(ResourceUiEvent::ResourcesLoaded {
            request: second,
            result: Ok(ResourceList {
                items: vec![cron_job_summary()],
                continue_token: None,
            }),
        });

        assert_eq!(panel.rows[0].name, "backup");
    }

    #[test]
    fn stale_watch_events_do_not_replace_current_rows() {
        let mut panel = CronJobResourcePanel::default();
        let cluster_id = ClusterId::new("local");
        let first = panel.request_cron_job_watch(cluster_id.clone());
        let second = panel.request_cron_job_watch(cluster_id);

        panel.apply_event(ResourceUiEvent::ResourceWatchUpdated {
            request: first,
            result: Ok(miku_api::ResourceEvent::Snapshot(ResourceList {
                items: vec![cron_job_summary_with_name("default", "stale")],
                continue_token: None,
            })),
        });
        assert!(panel.rows.is_empty());

        panel.apply_event(ResourceUiEvent::ResourceWatchUpdated {
            request: second,
            result: Ok(miku_api::ResourceEvent::Snapshot(ResourceList {
                items: vec![cron_job_summary()],
                continue_token: None,
            })),
        });

        assert_eq!(panel.rows[0].name, "backup");
    }

    #[test]
    fn namespace_watch_events_from_shared_request_update_selector() {
        let mut panel = CronJobResourcePanel::default();
        panel.apply_event(ResourceUiEvent::ResourceWatchUpdated {
            request: ResourceWatchRequest {
                request_id: 42,
                cluster_id: ClusterId::new("local"),
                kind: ResourceLoadKind::Namespaces,
            },
            result: Ok(miku_api::ResourceEvent::Snapshot(ResourceList {
                items: vec![namespace_summary("production")],
                continue_token: None,
            })),
        });

        assert_eq!(panel.namespaces, vec!["production".to_owned()]);
    }

    fn cron_job_summary() -> ResourceSummary {
        cron_job_summary_with_name("default", "backup")
    }

    fn cron_job_summary_with_name(namespace: &str, name: &str) -> ResourceSummary {
        ResourceSummary {
            name: name.to_owned(),
            namespace: Some(namespace.to_owned()),
            kind: "CronJob".to_owned(),
            status: None,
            raw: serde_json::json!({
                "metadata": {
                    "name": name,
                    "namespace": namespace,
                    "creationTimestamp": "2026-05-18T10:00:00Z",
                    "labels": {"app": name}
                },
                "spec": {
                    "schedule": "*/5 * * * *",
                    "suspend": false,
                    "concurrencyPolicy": "Forbid",
                    "successfulJobsHistoryLimit": 3,
                    "failedJobsHistoryLimit": 1,
                    "jobTemplate": {
                        "spec": {
                            "template": {
                                "metadata": {"labels": {"job-name": name}},
                                "spec": {
                                    "containers": [
                                        {"name": name, "image": "ghcr.io/example/backup:1.0.0"}
                                    ]
                                }
                            }
                        }
                    }
                },
                "status": {
                    "active": [
                        {"name": "backup-1"},
                        {"name": "backup-2"}
                    ],
                    "lastScheduleTime": "2026-05-18T10:00:00Z",
                    "lastSuccessfulTime": "2026-05-18T09:55:00Z"
                }
            }),
        }
    }

    fn namespace_summary(name: &str) -> ResourceSummary {
        ResourceSummary {
            name: name.to_owned(),
            namespace: None,
            kind: "Namespace".to_owned(),
            status: Some("Active".to_owned()),
            raw: serde_json::json!({"metadata": {"name": name}}),
        }
    }
}
