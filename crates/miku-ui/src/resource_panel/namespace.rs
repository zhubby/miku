use std::collections::BTreeSet;

use eframe::egui;
use egui_extras::{Column, TableBuilder};
use miku_api::ResourceSummary;
use miku_core::{ClusterId, ResourceRef};

#[cfg(test)]
use super::ResourceLoadRequest;
use super::components::{
    DescribeCondition, DescribeField, GenericBatchDeleteDialog, GenericCreateDialog,
    ResourceBatchDeleteDialogInput, ResourceCreateDialogInput, ResourceCreateDialogResponse,
    ResourceDeleteDialogResponse, ResourceMapEntry, ResourceMetadata, ResourceRowTarget,
    ResourceToolbar, ResourceYamlViewDialog, SELECT_COLUMN_WIDTH, apply_resource_request,
    batch_delete_resource_request, condition_describes, default_resource_yaml, describe_conditions,
    describe_fields, describe_group, describe_lines, describe_metadata_maps, describe_raw_manifest,
    resource_map_entries, selected_delete_targets, show_resource_batch_delete_dialog,
    show_resource_create_dialog, show_resource_describe_window, show_row_selection_checkbox,
    visible_keys,
};
use super::{
    LoadStatus, ResourceActionKind, ResourceActionOutcome, ResourceLoadKind, ResourcePanelRequests,
    ResourceUiEvent, ResourceWatchRequest,
};
use crate::time::human_age_from_rfc3339;

#[derive(Clone, Debug, Default)]
pub(crate) struct NamespaceResourcePanel {
    search_text: String,
    row_status: LoadStatus,
    rows: Vec<NamespaceRow>,
    selected_rows: BTreeSet<String>,
    next_request_id: u64,
    row_request_id: Option<u64>,
    row_watch_request_id: Option<u64>,
    last_cluster_id: Option<ClusterId>,
    describe_dialog: Option<NamespaceDescribeDialog>,
    view_dialog: Option<NamespaceViewDialog>,
    create_dialog: Option<GenericCreateDialog>,
    batch_delete_dialog: Option<GenericBatchDeleteDialog>,
    action_request_id: Option<u64>,
    action_error: Option<String>,
}

impl NamespaceResourcePanel {
    pub(crate) fn show(
        &mut self,
        ui: &mut egui::Ui,
        cluster_id: Option<&ClusterId>,
    ) -> ResourcePanelRequests {
        let mut requests = ResourcePanelRequests::default();
        let Some(cluster_id) = cluster_id else {
            ui.centered_and_justified(|ui| {
                ui.label("Select a cluster to load namespaces.");
            });
            return requests;
        };

        self.reset_for_cluster_change(cluster_id);
        if matches!(self.row_status, LoadStatus::Idle) {
            requests
                .watches
                .push(self.request_namespace_watch(cluster_id.clone()));
        }

        self.show_toolbar(ui, cluster_id, &mut requests);
        ui.separator();
        self.show_body(ui);
        self.show_describe_dialog(ui.ctx());
        self.show_view_dialog(ui.ctx());
        self.show_create_dialog(ui.ctx(), cluster_id, &mut requests.actions);
        self.show_batch_delete_dialog(ui.ctx(), cluster_id, &mut requests.actions);

        requests
    }

    pub(crate) fn apply_event(&mut self, event: ResourceUiEvent) {
        match event {
            ResourceUiEvent::ResourcesLoaded { request, result } => match request.kind {
                ResourceLoadKind::Namespaces => {
                    if self.row_request_id != Some(request.request_id) {
                        return;
                    }
                    self.row_request_id = None;
                    match result {
                        Ok(list) => {
                            self.replace_rows(namespace_rows_from_list(&list.items));
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
                | ResourceLoadKind::Events { .. }
                | ResourceLoadKind::Deployments { .. }
                | ResourceLoadKind::DaemonSets { .. }
                | ResourceLoadKind::StatefulSets { .. }
                | ResourceLoadKind::CronJobs { .. }
                | ResourceLoadKind::Jobs { .. }
                | ResourceLoadKind::LimitRanges { .. }
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
                | ResourceLoadKind::CustomResourceDefinitions
                | ResourceLoadKind::CustomResources { .. } => {}
            },
            ResourceUiEvent::ResourceWatchUpdated { request, result } => match request.kind {
                ResourceLoadKind::Namespaces => {
                    if self.row_watch_request_id == Some(request.request_id) {
                        self.row_watch_request_id = None;
                    }
                    match result {
                        Ok(miku_api::ResourceEvent::Snapshot(list)) => {
                            self.replace_rows(namespace_rows_from_list(&list.items));
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
                | ResourceLoadKind::Events { .. }
                | ResourceLoadKind::Deployments { .. }
                | ResourceLoadKind::DaemonSets { .. }
                | ResourceLoadKind::StatefulSets { .. }
                | ResourceLoadKind::CronJobs { .. }
                | ResourceLoadKind::Jobs { .. }
                | ResourceLoadKind::LimitRanges { .. }
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
                | ResourceLoadKind::CustomResourceDefinitions
                | ResourceLoadKind::CustomResources { .. } => {}
            },
            ResourceUiEvent::ResourceActionCompleted { request, result } => {
                if self.action_request_id != Some(request.request_id) {
                    return;
                }
                self.action_request_id = None;
                match result {
                    Ok(ResourceActionOutcome::Applied(_)) => {
                        self.create_dialog = None;
                        self.action_error = None;
                    }
                    Ok(ResourceActionOutcome::Deleted) => {
                        if let ResourceActionKind::DeleteResource { resource, name, .. } =
                            request.kind
                            && resource == namespace_metadata().resource
                        {
                            self.rows.retain(|row| row.key != name);
                            self.selected_rows.remove(&name);
                        }
                        self.action_error = None;
                    }
                    Ok(ResourceActionOutcome::BatchDeleted(targets)) => {
                        for target in targets {
                            self.rows.retain(|row| row.key != target.name);
                            self.selected_rows.remove(&target.name);
                        }
                        self.batch_delete_dialog = None;
                        self.action_error = None;
                    }
                    Ok(ResourceActionOutcome::Patched(_)) => {}
                    Ok(ResourceActionOutcome::Evicted) => {}
                    Err(error) => self.action_error = Some(error),
                }
            }
            ResourceUiEvent::PodLogsLoaded { .. }
            | ResourceUiEvent::PodAttachConnected { .. }
            | ResourceUiEvent::PodAttachOutput { .. } => {}
        }
    }

    fn reset_for_cluster_change(&mut self, cluster_id: &ClusterId) {
        if self.last_cluster_id.as_ref() == Some(cluster_id) {
            return;
        }

        self.last_cluster_id = Some(cluster_id.clone());
        self.search_text.clear();
        self.row_status = LoadStatus::Idle;
        self.rows.clear();
        self.selected_rows.clear();
        self.row_request_id = None;
        self.row_watch_request_id = None;
        self.describe_dialog = None;
        self.view_dialog = None;
        self.create_dialog = None;
        self.batch_delete_dialog = None;
        self.action_request_id = None;
        self.action_error = None;
    }

    fn show_toolbar(
        &mut self,
        ui: &mut egui::Ui,
        cluster_id: &ClusterId,
        requests: &mut ResourcePanelRequests,
    ) {
        let item_count = self.filtered_row_count();
        let response = ResourceToolbar {
            id_salt: "namespace_resource_toolbar",
            namespaces: &[],
            namespace_filter: &mut None,
            search_text: &mut self.search_text,
            search_hint: "Search Namespaces...",
            item_count,
            selected_count: self.selected_rows.len(),
            loading: matches!(self.row_status, LoadStatus::Loading),
        }
        .show(ui);

        if response.search_changed {
            self.prune_selection_to_visible();
        }
        if response.refresh_clicked {
            requests
                .watches
                .push(self.request_namespace_watch(cluster_id.clone()));
        }
        if response.create_clicked {
            self.create_dialog = Some(GenericCreateDialog {
                yaml: default_resource_yaml(namespace_metadata(), None),
                parse_error: None,
            });
            self.action_error = None;
        }
        if response.batch_delete_clicked {
            let targets = self.selected_delete_targets();
            if !targets.is_empty() {
                self.batch_delete_dialog = Some(GenericBatchDeleteDialog { targets });
                self.action_error = None;
            }
        }
    }

    fn show_body(&mut self, ui: &mut egui::Ui) {
        match &self.row_status {
            LoadStatus::Idle | LoadStatus::Loading if self.rows.is_empty() => {
                ui.centered_and_justified(|ui| {
                    ui.label("Loading namespaces...");
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
                        ui.label("No namespaces match the current filters.");
                    });
                    return;
                }

                let action =
                    show_namespace_table(ui, &self.rows, row_indices, &mut self.selected_rows);
                self.apply_table_action(action);
            }
        }
    }

    fn apply_table_action(&mut self, action: Option<NamespaceTableAction>) {
        match action {
            Some(NamespaceTableAction::Describe { key }) => {
                let Some((name, describe)) = self
                    .row_by_key(&key)
                    .map(|row| (row.name.clone(), namespace_describe_from_row(row)))
                else {
                    return;
                };
                self.describe_dialog = Some(NamespaceDescribeDialog {
                    key,
                    name,
                    describe,
                });
            }
            Some(NamespaceTableAction::View { key }) => {
                let Some((name, yaml)) = self
                    .row_by_key(&key)
                    .map(|row| (row.name.clone(), full_manifest_yaml(&row.raw)))
                else {
                    return;
                };
                self.view_dialog = Some(NamespaceViewDialog { key, name, yaml });
            }
            Some(NamespaceTableAction::Delete { key }) => {
                let Some(target) = self.row_by_key(&key).map(NamespaceRow::target) else {
                    return;
                };
                self.batch_delete_dialog = Some(GenericBatchDeleteDialog {
                    targets: vec![super::ResourceDeleteTarget {
                        namespace: target.namespace,
                        name: target.name,
                    }],
                });
                self.action_error = None;
            }
            None => {}
        }
    }

    fn show_describe_dialog(&mut self, ctx: &egui::Context) {
        let Some(dialog) = self.describe_dialog.as_ref() else {
            return;
        };

        let mut open = true;
        show_resource_describe_window(
            ctx,
            egui::Id::new(("namespace-describe-dialog", &dialog.key)),
            format!("Describe {}", dialog.name),
            &mut open,
            |ui| {
                show_namespace_describe(ui, &dialog.describe);
            },
        );

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
            id: egui::Id::new(("namespace-view-dialog", &dialog.key)),
            title: format!("View {}", dialog.name),
            yaml: &dialog.yaml,
            open: &mut open,
        }
        .show(ctx);

        if !response.open {
            self.view_dialog = None;
        }
    }

    fn show_create_dialog(
        &mut self,
        ctx: &egui::Context,
        cluster_id: &ClusterId,
        requests: &mut Vec<super::ResourceActionRequest>,
    ) {
        let Some(dialog) = self.create_dialog.as_mut() else {
            return;
        };
        match show_resource_create_dialog(
            ctx,
            ResourceCreateDialogInput {
                metadata: namespace_metadata(),
                dialog,
                action_error: self.action_error.as_deref(),
                action_in_flight: self.action_request_id.is_some(),
                namespace_default: None,
            },
        ) {
            ResourceCreateDialogResponse::None => {}
            ResourceCreateDialogResponse::Cancel => {
                self.create_dialog = None;
                self.action_error = None;
            }
            ResourceCreateDialogResponse::Apply(parsed) => {
                let request = apply_resource_request(
                    self.allocate_request_id(),
                    cluster_id.clone(),
                    namespace_metadata(),
                    parsed,
                );
                self.action_request_id = Some(request.request_id);
                requests.push(request);
            }
        }
    }

    fn show_batch_delete_dialog(
        &mut self,
        ctx: &egui::Context,
        cluster_id: &ClusterId,
        requests: &mut Vec<super::ResourceActionRequest>,
    ) {
        let Some(dialog) = self.batch_delete_dialog.clone() else {
            return;
        };
        match show_resource_batch_delete_dialog(
            ctx,
            ResourceBatchDeleteDialogInput {
                metadata: namespace_metadata(),
                targets: &dialog.targets,
                action_error: self.action_error.as_deref(),
                action_in_flight: self.action_request_id.is_some(),
            },
        ) {
            ResourceDeleteDialogResponse::None => {}
            ResourceDeleteDialogResponse::Cancel => {
                self.batch_delete_dialog = None;
                self.action_error = None;
            }
            ResourceDeleteDialogResponse::Delete => {
                let request = batch_delete_resource_request(
                    self.allocate_request_id(),
                    cluster_id.clone(),
                    namespace_metadata(),
                    dialog.targets,
                );
                self.action_request_id = Some(request.request_id);
                requests.push(request);
            }
        }
    }

    #[cfg(test)]
    fn request_namespaces(&mut self, cluster_id: ClusterId) -> ResourceLoadRequest {
        let request = ResourceLoadRequest {
            request_id: self.allocate_request_id(),
            cluster_id,
            kind: ResourceLoadKind::Namespaces,
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

    fn row_by_key(&self, key: &str) -> Option<&NamespaceRow> {
        self.rows.iter().find(|row| row.key == key)
    }

    fn replace_rows(&mut self, rows: Vec<NamespaceRow>) {
        let targets = rows.iter().map(NamespaceRow::target).collect::<Vec<_>>();
        let visible_keys = visible_keys(&targets);
        self.selected_rows.retain(|key| visible_keys.contains(key));
        self.rows = rows;
    }

    fn prune_selection_to_visible(&mut self) {
        let targets = self
            .filtered_row_indices()
            .into_iter()
            .filter_map(|index| self.rows.get(index))
            .map(NamespaceRow::target)
            .collect::<Vec<_>>();
        let visible_keys = visible_keys(&targets);
        self.selected_rows.retain(|key| visible_keys.contains(key));
    }

    fn selected_delete_targets(&self) -> Vec<super::ResourceDeleteTarget> {
        let targets = self
            .rows
            .iter()
            .map(NamespaceRow::target)
            .collect::<Vec<_>>();
        selected_delete_targets(&targets, &self.selected_rows)
    }
}

fn show_namespace_table(
    ui: &mut egui::Ui,
    rows: &[NamespaceRow],
    row_indices: Vec<usize>,
    selected_rows: &mut BTreeSet<String>,
) -> Option<NamespaceTableAction> {
    let row_height = ui.spacing().interact_size.y;
    let table_width: f32 = SELECT_COLUMN_WIDTH
        + NAMESPACE_COLUMN_WIDTHS.iter().sum::<f32>()
        + ui.spacing().item_spacing.x * NAMESPACE_COLUMNS.len() as f32;
    let mut action = None;

    egui::ScrollArea::horizontal()
        .id_salt("namespace_resource_table_horizontal")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.set_min_width(table_width);

            let mut table = TableBuilder::new(ui)
                .id_salt("namespace_resource_table")
                .striped(true)
                .resizable(false)
                .sense(egui::Sense::click())
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .min_scrolled_height(0.0);

            table = table.column(Column::exact(SELECT_COLUMN_WIDTH));
            for width in NAMESPACE_COLUMN_WIDTHS {
                table = table.column(Column::exact(width));
            }

            table
                .header(row_height, |mut header| {
                    header.col(|_| {});
                    for label in NAMESPACE_COLUMNS {
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

                        let row_selected = selected_rows.contains(&row.key);
                        table_row.set_selected(row_selected);
                        let mut checkbox_changed = false;
                        table_row.col(|ui| {
                            checkbox_changed =
                                show_row_selection_checkbox(ui, selected_rows, &row.key);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.name);
                        });
                        table_row.col(|ui| {
                            ui.colored_label(status_color(ui, &row.status), &row.status);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.labels);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.finalizers);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.age);
                        });

                        let response = table_row.response();
                        if response.clicked() && !checkbox_changed {
                            selected_rows.clear();
                            selected_rows.insert(row.key.clone());
                        }
                        response.context_menu(|ui| {
                            if ui
                                .button(format!("{} Describe", egui_phosphor::regular::INFO))
                                .clicked()
                            {
                                action = Some(NamespaceTableAction::Describe {
                                    key: row.key.clone(),
                                });
                                ui.close();
                            }
                            if ui
                                .button(format!("{} View", egui_phosphor::regular::EYE))
                                .clicked()
                            {
                                action = Some(NamespaceTableAction::View {
                                    key: row.key.clone(),
                                });
                                ui.close();
                            }
                            ui.separator();
                            let delete_text = egui::RichText::new(format!(
                                "{} Delete",
                                egui_phosphor::regular::TRASH
                            ))
                            .color(ui.visuals().error_fg_color);
                            if ui.button(delete_text).clicked() {
                                action = Some(NamespaceTableAction::Delete {
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

const NAMESPACE_COLUMNS: [&str; 5] = ["Name", "Status", "Labels", "Finalizers", "Age"];
const NAMESPACE_COLUMN_WIDTHS: [f32; 5] = [260.0, 120.0, 120.0, 320.0, 90.0];

fn status_color(ui: &egui::Ui, status: &str) -> egui::Color32 {
    match status {
        "Active" => egui::Color32::from_rgb(46, 160, 67),
        "Terminating" => egui::Color32::from_rgb(191, 135, 0),
        "Unknown" => ui.visuals().error_fg_color,
        _ => ui.visuals().text_color(),
    }
}

#[cfg(test)]
fn filter_namespace_rows<'a>(rows: &'a [NamespaceRow], search_text: &str) -> Vec<&'a NamespaceRow> {
    rows.iter()
        .filter(|row| row_matches_search(row, search_text))
        .collect()
}

fn row_matches_search(row: &NamespaceRow, search_text: &str) -> bool {
    let needle = search_text.trim().to_lowercase();
    needle.is_empty() || row.name.to_lowercase().contains(&needle)
}

fn namespace_metadata() -> ResourceMetadata {
    ResourceMetadata {
        id: "namespace".to_owned(),
        title: "Namespaces".to_owned(),
        api_version: "v1".to_owned(),
        kind: "Namespace".to_owned(),
        resource: ResourceRef::core("v1", "namespaces").cluster_scoped(),
        namespaced: false,
    }
}

fn namespace_rows_from_list(items: &[ResourceSummary]) -> Vec<NamespaceRow> {
    let mut rows = items
        .iter()
        .map(NamespaceRow::from_summary)
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| left.name.cmp(&right.name));
    rows
}

#[derive(Clone, Debug, PartialEq)]
struct NamespaceRow {
    key: String,
    name: String,
    status: String,
    labels: String,
    finalizers: String,
    age: String,
    raw: serde_json::Value,
}

impl NamespaceRow {
    fn from_summary(summary: &ResourceSummary) -> Self {
        let raw = &summary.raw;
        let name = value_str(raw, &["metadata", "name"]).unwrap_or(&summary.name);

        Self {
            key: name.to_owned(),
            name: name.to_owned(),
            status: namespace_status(raw, summary.status.as_deref()),
            labels: count_label(raw.pointer("/metadata/labels"), "label"),
            finalizers: namespace_finalizers(raw),
            age: value_str(raw, &["metadata", "creationTimestamp"])
                .map(|timestamp| {
                    human_age_from_rfc3339(timestamp).unwrap_or_else(|| timestamp.to_owned())
                })
                .unwrap_or_else(|| "N/A".to_owned()),
            raw: summary.raw.clone(),
        }
    }

    fn target(&self) -> ResourceRowTarget {
        ResourceRowTarget {
            key: self.key.clone(),
            namespace: None,
            name: self.name.clone(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum NamespaceTableAction {
    Describe { key: String },
    View { key: String },
    Delete { key: String },
}

#[derive(Clone, Debug, PartialEq)]
struct NamespaceDescribeDialog {
    key: String,
    name: String,
    describe: NamespaceDescribe,
}

#[derive(Clone, Debug, PartialEq)]
struct NamespaceViewDialog {
    key: String,
    name: String,
    yaml: String,
}

#[derive(Clone, Debug, PartialEq)]
struct NamespaceDescribe {
    summary: Vec<DescribeField>,
    labels: Vec<ResourceMapEntry>,
    annotations: Vec<ResourceMapEntry>,
    finalizers: Vec<String>,
    conditions: Vec<DescribeCondition>,
    raw_yaml: String,
}

fn show_namespace_describe(ui: &mut egui::Ui, describe: &NamespaceDescribe) {
    describe_group(ui, egui_phosphor::regular::FOLDER, "Namespace", |ui| {
        describe_fields(ui, &describe.summary);
    });

    ui.add_space(10.0);
    describe_group(ui, egui_phosphor::regular::TAG, "Metadata", |ui| {
        describe_metadata_maps(
            ui,
            "namespace-describe-metadata",
            &describe.labels,
            &describe.annotations,
        );
    });

    ui.add_space(10.0);
    describe_group(
        ui,
        egui_phosphor::regular::LIST_CHECKS,
        "Finalizers",
        |ui| {
            describe_lines(ui, &describe.finalizers);
        },
    );

    ui.add_space(10.0);
    describe_group(
        ui,
        egui_phosphor::regular::CHECK_CIRCLE,
        "Conditions",
        |ui| {
            describe_conditions(ui, "namespace-describe-conditions", &describe.conditions);
        },
    );

    ui.add_space(10.0);
    describe_group(ui, egui_phosphor::regular::CODE, "Raw manifest", |ui| {
        describe_raw_manifest(
            ui,
            "namespace-describe-raw-manifest-content",
            &describe.raw_yaml,
        );
    });
}

fn namespace_describe_from_row(row: &NamespaceRow) -> NamespaceDescribe {
    let raw = &row.raw;
    NamespaceDescribe {
        summary: vec![
            DescribeField::new("Name", row.name.clone()),
            DescribeField::new("Status", row.status.clone()),
            DescribeField::new("Labels", row.labels.clone()),
            DescribeField::new("Finalizers", row.finalizers.clone()),
            DescribeField::new("Age", row.age.clone()),
        ],
        labels: resource_map_entries(raw.pointer("/metadata/labels")),
        annotations: resource_map_entries(raw.pointer("/metadata/annotations")),
        finalizers: namespace_finalizer_lines(raw),
        conditions: condition_describes(raw.pointer("/status/conditions")),
        raw_yaml: full_manifest_yaml(raw),
    }
}

fn namespace_status(raw: &serde_json::Value, summary_status: Option<&str>) -> String {
    value_str(raw, &["status", "phase"])
        .or(summary_status)
        .unwrap_or("Unknown")
        .to_owned()
}

fn count_label(value: Option<&serde_json::Value>, noun: &str) -> String {
    let count = value
        .and_then(serde_json::Value::as_object)
        .map_or(0, serde_json::Map::len);
    if count == 1 {
        format!("1 {noun}")
    } else {
        format!("{count} {noun}s")
    }
}

fn namespace_finalizers(raw: &serde_json::Value) -> String {
    let finalizers = namespace_finalizer_lines(raw);
    if finalizers.is_empty() {
        "N/A".to_owned()
    } else {
        finalizers.join(", ")
    }
}

fn namespace_finalizer_lines(raw: &serde_json::Value) -> Vec<String> {
    raw.pointer("/spec/finalizers")
        .or_else(|| raw.pointer("/metadata/finalizers"))
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .map(ToOwned::to_owned)
        .collect()
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

#[cfg(test)]
mod tests {
    use super::*;
    use miku_api::ResourceList;

    #[test]
    fn namespace_row_extracts_table_fields_from_raw_summary() {
        let row = NamespaceRow::from_summary(&namespace_summary());

        assert_eq!(row.name, "production");
        assert_eq!(row.status, "Active");
        assert_eq!(row.labels, "2 labels");
        assert_eq!(row.finalizers, "kubernetes");
        assert!(row.age.ends_with(" ago"));
    }

    #[test]
    fn namespace_rows_filter_by_name_case_insensitively() {
        let rows = vec![
            NamespaceRow::from_summary(&namespace_summary()),
            NamespaceRow::from_summary(&ResourceSummary {
                name: "default".to_owned(),
                namespace: None,
                kind: "Namespace".to_owned(),
                status: Some("Active".to_owned()),
                raw: serde_json::json!({"metadata": {"name": "default"}}),
            }),
        ];

        let filtered = filter_namespace_rows(&rows, "PROD");

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "production");
    }

    #[test]
    fn namespace_rows_are_sorted_by_name() {
        let rows = namespace_rows_from_list(&[
            ResourceSummary {
                name: "zeta".to_owned(),
                namespace: None,
                kind: "Namespace".to_owned(),
                status: None,
                raw: serde_json::json!({"metadata": {"name": "zeta"}}),
            },
            ResourceSummary {
                name: "default".to_owned(),
                namespace: None,
                kind: "Namespace".to_owned(),
                status: None,
                raw: serde_json::json!({"metadata": {"name": "default"}}),
            },
            ResourceSummary {
                name: "production".to_owned(),
                namespace: None,
                kind: "Namespace".to_owned(),
                status: None,
                raw: serde_json::json!({"metadata": {"name": "production"}}),
            },
        ]);

        let names = rows.into_iter().map(|row| row.name).collect::<Vec<_>>();
        assert_eq!(names, vec!["default", "production", "zeta"]);
    }

    #[test]
    fn namespace_describe_extracts_details() {
        let row = NamespaceRow::from_summary(&namespace_summary());
        let describe = namespace_describe_from_row(&row);

        assert_eq!(describe.conditions.len(), 1);
        assert_eq!(
            describe.conditions[0].condition_type,
            "NamespaceContentRemaining"
        );
        assert_eq!(describe.finalizers, vec!["kubernetes"]);
        assert!(describe.labels.iter().any(|entry| entry.key == "team"));
        assert!(
            describe
                .annotations
                .iter()
                .any(|entry| entry.key == "owner")
        );
    }

    #[test]
    fn namespace_describe_handles_missing_optional_fields() {
        let row = NamespaceRow::from_summary(&ResourceSummary {
            name: "minimal".to_owned(),
            namespace: None,
            kind: "Namespace".to_owned(),
            status: None,
            raw: serde_json::json!({"metadata": {"name": "minimal"}, "status": {}}),
        });

        let describe = namespace_describe_from_row(&row);

        assert_eq!(row.status, "Unknown");
        assert_eq!(row.labels, "0 labels");
        assert_eq!(row.finalizers, "N/A");
        assert!(describe.conditions.is_empty());
        assert!(describe.finalizers.is_empty());
    }

    #[test]
    fn stale_resource_events_do_not_replace_current_rows() {
        let mut panel = NamespaceResourcePanel::default();
        let cluster_id = ClusterId::new("local");
        let first = panel.request_namespaces(cluster_id.clone());
        let second = panel.request_namespaces(cluster_id);

        panel.apply_event(ResourceUiEvent::ResourcesLoaded {
            request: first,
            result: Ok(ResourceList {
                items: vec![ResourceSummary {
                    name: "stale".to_owned(),
                    namespace: None,
                    kind: "Namespace".to_owned(),
                    status: None,
                    raw: serde_json::json!({"metadata": {"name": "stale"}}),
                }],
                continue_token: None,
            }),
        });
        assert!(panel.rows.is_empty());

        panel.apply_event(ResourceUiEvent::ResourcesLoaded {
            request: second,
            result: Ok(ResourceList {
                items: vec![namespace_summary()],
                continue_token: None,
            }),
        });

        assert_eq!(panel.rows.len(), 1);
        assert_eq!(panel.rows[0].name, "production");
    }

    #[test]
    fn namespace_watch_events_are_shared_between_panels() {
        let mut panel = NamespaceResourcePanel::default();
        let request = ResourceWatchRequest {
            request_id: 42,
            cluster_id: ClusterId::new("local"),
            kind: ResourceLoadKind::Namespaces,
        };

        panel.apply_event(ResourceUiEvent::ResourceWatchUpdated {
            request,
            result: Ok(miku_api::ResourceEvent::Snapshot(ResourceList {
                items: vec![namespace_summary()],
                continue_token: None,
            })),
        });

        assert_eq!(panel.rows.len(), 1);
        assert_eq!(panel.rows[0].name, "production");
    }

    #[test]
    fn row_delete_action_opens_single_namespace_delete_dialog() {
        let mut panel = NamespaceResourcePanel {
            rows: vec![NamespaceRow::from_summary(&namespace_summary())],
            ..NamespaceResourcePanel::default()
        };

        panel.apply_table_action(Some(NamespaceTableAction::Delete {
            key: "production".to_owned(),
        }));

        let dialog = panel.batch_delete_dialog.unwrap();
        assert_eq!(
            dialog.targets,
            vec![super::super::ResourceDeleteTarget {
                namespace: None,
                name: "production".to_owned(),
            }]
        );
    }

    fn namespace_summary() -> ResourceSummary {
        ResourceSummary {
            name: "production".to_owned(),
            namespace: None,
            kind: "Namespace".to_owned(),
            status: Some("Active".to_owned()),
            raw: serde_json::json!({
                "metadata": {
                    "name": "production",
                    "creationTimestamp": "2026-05-18T10:00:00Z",
                    "labels": {
                        "team": "platform",
                        "environment": "prod"
                    },
                    "annotations": {
                        "owner": "platform"
                    }
                },
                "spec": {
                    "finalizers": ["kubernetes"]
                },
                "status": {
                    "phase": "Active",
                    "conditions": [
                        {
                            "type": "NamespaceContentRemaining",
                            "status": "False",
                            "reason": "ContentRemoved",
                            "message": "All content removed"
                        }
                    ]
                }
            }),
        }
    }
}
