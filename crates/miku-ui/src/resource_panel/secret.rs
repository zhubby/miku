use std::collections::BTreeSet;

use eframe::egui;
use egui_extras::{Column, TableBuilder};
use miku_api::ResourceSummary;
use miku_core::ClusterId;

#[cfg(test)]
use super::ResourceLoadRequest;
use super::components::{
    DescribeField, GenericBatchDeleteDialog, GenericCreateDialog, GenericDeleteDialog,
    GenericEditDialog, ResourceBatchDeleteDialogInput, ResourceCreateDialogInput,
    ResourceCreateDialogResponse, ResourceDeleteDialogInput, ResourceDeleteDialogResponse,
    ResourceEditDialogInput, ResourceEditDialogResponse, ResourceMapEntry, ResourceMapView,
    ResourceMetadata, ResourceRowTarget, ResourceToolbar, ResourceYamlViewDialog,
    SELECT_COLUMN_WIDTH, apply_resource_request, batch_delete_resource_request,
    delete_resource_request, describe_fields, describe_group, describe_metadata_maps,
    describe_raw_manifest, edit_resource_request, editable_resource_yaml, resource_map_entries,
    selected_delete_targets, show_resource_batch_delete_dialog, show_resource_create_dialog,
    show_resource_delete_dialog, show_resource_describe_window, show_resource_edit_dialog,
    show_row_selection_checkbox, visible_keys,
};
use super::{
    LoadStatus, ResourceActionKind, ResourceActionOutcome, ResourceLoadKind, ResourcePanelRequests,
    ResourceUiEvent, ResourceWatchRequest, namespaces_from_list,
};
use crate::time::human_age_from_rfc3339;

const REDACTED_SECRET_VALUE: &str = "REDACTED";

#[derive(Clone, Debug, Default)]
pub(crate) struct SecretResourcePanel {
    namespace_filter: Option<String>,
    search_text: String,
    namespaces: Vec<String>,
    namespace_status: LoadStatus,
    row_status: LoadStatus,
    rows: Vec<SecretRow>,
    selected_rows: BTreeSet<String>,
    next_request_id: u64,
    namespace_request_id: Option<u64>,
    row_request_id: Option<u64>,
    namespace_watch_request_id: Option<u64>,
    row_watch_request_id: Option<u64>,
    last_cluster_id: Option<ClusterId>,
    describe_dialog: Option<SecretDescribeDialog>,
    view_dialog: Option<SecretViewDialog>,
    edit_dialog: Option<GenericEditDialog>,
    create_dialog: Option<GenericCreateDialog>,
    batch_delete_dialog: Option<GenericBatchDeleteDialog>,
    delete_dialog: Option<GenericDeleteDialog>,
    action_request_id: Option<u64>,
    action_error: Option<String>,
}

impl SecretResourcePanel {
    pub(crate) fn show(
        &mut self,
        ui: &mut egui::Ui,
        cluster_id: Option<&ClusterId>,
    ) -> ResourcePanelRequests {
        let mut requests = ResourcePanelRequests::default();
        let Some(cluster_id) = cluster_id else {
            ui.centered_and_justified(|ui| {
                ui.label("Select a cluster to load secrets.");
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
                .push(self.request_secret_watch(cluster_id.clone()));
        }

        self.show_toolbar(ui, cluster_id, &mut requests);
        ui.separator();
        self.show_body(ui);
        self.show_describe_dialog(ui.ctx());
        self.show_view_dialog(ui.ctx());
        self.show_edit_dialog(ui.ctx(), cluster_id, &mut requests.actions);
        self.show_create_dialog(ui.ctx(), cluster_id, &mut requests.actions);
        self.show_batch_delete_dialog(ui.ctx(), cluster_id, &mut requests.actions);
        self.show_delete_dialog(ui.ctx(), cluster_id, &mut requests.actions);

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
                ResourceLoadKind::Secrets { .. } => {
                    if self.row_request_id != Some(request.request_id) {
                        return;
                    }
                    self.row_request_id = None;
                    match result {
                        Ok(list) => {
                            self.replace_rows(secret_rows_from_list(&list.items));
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
                | ResourceLoadKind::CronJobs { .. }
                | ResourceLoadKind::DaemonSets { .. }
                | ResourceLoadKind::Deployments { .. }
                | ResourceLoadKind::Events { .. }
                | ResourceLoadKind::Jobs { .. }
                | ResourceLoadKind::LimitRanges { .. }
                | ResourceLoadKind::ReplicaSets { .. }
                | ResourceLoadKind::ResourceQuotas { .. }
                | ResourceLoadKind::Services { .. }
                | ResourceLoadKind::StatefulSets { .. }
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
                ResourceLoadKind::Secrets { .. } => {
                    if self.row_watch_request_id != Some(request.request_id) {
                        return;
                    }
                    match result {
                        Ok(miku_api::ResourceEvent::Snapshot(list)) => {
                            self.replace_rows(secret_rows_from_list(&list.items));
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
                | ResourceLoadKind::CronJobs { .. }
                | ResourceLoadKind::DaemonSets { .. }
                | ResourceLoadKind::Deployments { .. }
                | ResourceLoadKind::Events { .. }
                | ResourceLoadKind::Jobs { .. }
                | ResourceLoadKind::LimitRanges { .. }
                | ResourceLoadKind::ReplicaSets { .. }
                | ResourceLoadKind::ResourceQuotas { .. }
                | ResourceLoadKind::Services { .. }
                | ResourceLoadKind::StatefulSets { .. }
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
                    Ok(ResourceActionOutcome::Applied(summary)) => {
                        self.upsert_row(SecretRow::from_summary(&summary));
                        self.create_dialog = None;
                        self.edit_dialog = None;
                        self.action_error = None;
                    }
                    Ok(ResourceActionOutcome::Deleted) => {
                        if let ResourceActionKind::DeleteResource {
                            resource,
                            namespace,
                            name,
                        } = request.kind
                            && resource == secret_metadata().resource
                        {
                            let key = namespaced_key(namespace.as_deref().unwrap_or(""), &name);
                            self.rows.retain(|row| row.key != key);
                            self.selected_rows.remove(&key);
                        }
                        self.delete_dialog = None;
                        self.action_error = None;
                    }
                    Ok(ResourceActionOutcome::BatchDeleted(targets)) => {
                        for target in targets {
                            let key = namespaced_key(
                                target.namespace.as_deref().unwrap_or(""),
                                &target.name,
                            );
                            self.rows.retain(|row| row.key != key);
                            self.selected_rows.remove(&key);
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
            | ResourceUiEvent::PodAttachOutput { .. }
            | ResourceUiEvent::PodExecConnected { .. }
            | ResourceUiEvent::PodExecOutput { .. } => {}
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
        self.selected_rows.clear();
        self.namespace_status = LoadStatus::Idle;
        self.row_status = LoadStatus::Idle;
        self.namespace_request_id = None;
        self.row_request_id = None;
        self.namespace_watch_request_id = None;
        self.row_watch_request_id = None;
        self.describe_dialog = None;
        self.view_dialog = None;
        self.edit_dialog = None;
        self.create_dialog = None;
        self.batch_delete_dialog = None;
        self.delete_dialog = None;
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
            id_salt: "secret_resource_toolbar",
            namespaces: &self.namespaces,
            namespace_filter: &mut self.namespace_filter,
            search_text: &mut self.search_text,
            search_hint: "Search Secrets...",
            item_count,
            selected_count: self.selected_rows.len(),
            loading: matches!(self.row_status, LoadStatus::Loading),
        }
        .show(ui);

        if response.namespace_changed {
            requests
                .watches
                .push(self.request_secret_watch(cluster_id.clone()));
        }
        if response.search_changed {
            self.prune_selection_to_visible();
        }
        if response.refresh_clicked {
            requests
                .watches
                .push(self.request_namespace_watch(cluster_id.clone()));
            requests
                .watches
                .push(self.request_secret_watch(cluster_id.clone()));
        }
        if response.create_clicked {
            self.create_dialog = Some(GenericCreateDialog {
                yaml: default_secret_yaml(self.namespace_filter.as_deref()),
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
        if matches!(self.namespace_status, LoadStatus::Error(_)) {
            ui.colored_label(ui.visuals().error_fg_color, "Namespaces unavailable");
        }
    }

    fn show_body(&mut self, ui: &mut egui::Ui) {
        match &self.row_status {
            LoadStatus::Idle | LoadStatus::Loading if self.rows.is_empty() => {
                ui.centered_and_justified(|ui| {
                    ui.label("Loading secrets...");
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
                        ui.label("No secrets match the current filters.");
                    });
                    return;
                }

                let action =
                    show_secret_table(ui, &self.rows, row_indices, &mut self.selected_rows);
                self.apply_table_action(action);
            }
        }
    }

    fn apply_table_action(&mut self, action: Option<SecretTableAction>) {
        match action {
            Some(SecretTableAction::Describe { key }) => {
                let Some((name, describe)) = self
                    .row_by_key(&key)
                    .map(|row| (row.name.clone(), secret_describe_from_row(row)))
                else {
                    return;
                };
                self.describe_dialog = Some(SecretDescribeDialog {
                    key,
                    name,
                    describe,
                });
            }
            Some(SecretTableAction::View { key }) => {
                let Some((name, yaml)) = self.row_by_key(&key).map(|row| {
                    (
                        row.name.clone(),
                        full_manifest_yaml(&redacted_secret_manifest(&row.raw)),
                    )
                }) else {
                    return;
                };
                self.view_dialog = Some(SecretViewDialog { key, name, yaml });
            }
            Some(SecretTableAction::Edit { key }) => {
                let Some((target, yaml)) = self
                    .row_by_key(&key)
                    .map(|row| (row.target(), editable_resource_yaml(&row.raw)))
                else {
                    return;
                };
                self.edit_dialog = Some(GenericEditDialog {
                    target,
                    yaml,
                    parse_error: None,
                });
                self.action_error = None;
            }
            Some(SecretTableAction::Delete { key }) => {
                let Some(row) = self.row_by_key(&key) else {
                    return;
                };
                self.delete_dialog = Some(GenericDeleteDialog {
                    target: row.delete_target(),
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
            egui::Id::new(("secret-describe-dialog", &dialog.key)),
            format!("Describe {}", dialog.name),
            &mut open,
            |ui| {
                show_secret_describe(ui, &dialog.describe);
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
            id: egui::Id::new(("secret-view-dialog", &dialog.key)),
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
                metadata: secret_metadata(),
                dialog,
                action_error: self.action_error.as_deref(),
                action_in_flight: self.action_request_id.is_some(),
                namespace_default: self.namespace_filter.as_deref(),
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
                    secret_metadata(),
                    parsed,
                );
                self.action_request_id = Some(request.request_id);
                requests.push(request);
            }
        }
    }

    fn show_edit_dialog(
        &mut self,
        ctx: &egui::Context,
        cluster_id: &ClusterId,
        requests: &mut Vec<super::ResourceActionRequest>,
    ) {
        let Some(target) = self
            .edit_dialog
            .as_ref()
            .map(|dialog| dialog.target.clone())
        else {
            return;
        };
        let Some(dialog) = self.edit_dialog.as_mut() else {
            return;
        };
        match show_resource_edit_dialog(
            ctx,
            ResourceEditDialogInput {
                metadata: secret_metadata(),
                dialog,
                action_error: self.action_error.as_deref(),
                action_in_flight: self.action_request_id.is_some(),
            },
        ) {
            ResourceEditDialogResponse::None => {}
            ResourceEditDialogResponse::Cancel => {
                self.edit_dialog = None;
                self.action_error = None;
            }
            ResourceEditDialogResponse::Apply(manifest) => {
                let request = edit_resource_request(
                    self.allocate_request_id(),
                    cluster_id.clone(),
                    secret_metadata(),
                    target,
                    manifest,
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
                metadata: secret_metadata(),
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
                    secret_metadata(),
                    dialog.targets,
                );
                self.action_request_id = Some(request.request_id);
                requests.push(request);
            }
        }
    }

    fn show_delete_dialog(
        &mut self,
        ctx: &egui::Context,
        cluster_id: &ClusterId,
        requests: &mut Vec<super::ResourceActionRequest>,
    ) {
        let Some(dialog) = self.delete_dialog.clone() else {
            return;
        };
        match show_resource_delete_dialog(
            ctx,
            ResourceDeleteDialogInput {
                metadata: secret_metadata(),
                target: &dialog.target,
                action_error: self.action_error.as_deref(),
                action_in_flight: self.action_request_id.is_some(),
            },
        ) {
            ResourceDeleteDialogResponse::None => {}
            ResourceDeleteDialogResponse::Cancel => {
                self.delete_dialog = None;
                self.action_error = None;
            }
            ResourceDeleteDialogResponse::Delete => {
                let request = delete_resource_request(
                    self.allocate_request_id(),
                    cluster_id.clone(),
                    secret_metadata(),
                    dialog.target,
                );
                self.action_request_id = Some(request.request_id);
                requests.push(request);
            }
        }
    }

    #[cfg(test)]
    fn request_secrets(&mut self, cluster_id: ClusterId) -> ResourceLoadRequest {
        let request = ResourceLoadRequest {
            request_id: self.allocate_request_id(),
            cluster_id,
            kind: ResourceLoadKind::Secrets {
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

    fn request_secret_watch(&mut self, cluster_id: ClusterId) -> ResourceWatchRequest {
        let request = ResourceWatchRequest {
            request_id: self.allocate_request_id(),
            cluster_id,
            kind: ResourceLoadKind::Secrets {
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

    fn row_by_key(&self, key: &str) -> Option<&SecretRow> {
        self.rows.iter().find(|row| row.key == key)
    }

    fn replace_rows(&mut self, rows: Vec<SecretRow>) {
        let targets = rows.iter().map(SecretRow::target).collect::<Vec<_>>();
        let visible_keys = visible_keys(&targets);
        self.selected_rows.retain(|key| visible_keys.contains(key));
        self.rows = rows;
    }

    fn upsert_row(&mut self, row: SecretRow) {
        if let Some(existing) = self
            .rows
            .iter_mut()
            .find(|existing| existing.key == row.key)
        {
            *existing = row;
        } else {
            self.rows.push(row);
        }
        self.rows.sort_by(|left, right| {
            left.namespace
                .cmp(&right.namespace)
                .then(left.name.cmp(&right.name))
        });
    }

    fn prune_selection_to_visible(&mut self) {
        let targets = self
            .filtered_row_indices()
            .into_iter()
            .filter_map(|index| self.rows.get(index))
            .map(SecretRow::target)
            .collect::<Vec<_>>();
        let visible_keys = visible_keys(&targets);
        self.selected_rows.retain(|key| visible_keys.contains(key));
    }

    fn selected_delete_targets(&self) -> Vec<super::ResourceDeleteTarget> {
        let targets = self.rows.iter().map(SecretRow::target).collect::<Vec<_>>();
        selected_delete_targets(&targets, &self.selected_rows)
    }
}

fn show_secret_table(
    ui: &mut egui::Ui,
    rows: &[SecretRow],
    row_indices: Vec<usize>,
    selected_rows: &mut BTreeSet<String>,
) -> Option<SecretTableAction> {
    let row_height = ui.spacing().interact_size.y;
    let table_width: f32 = SELECT_COLUMN_WIDTH
        + COLUMN_WIDTHS.iter().sum::<f32>()
        + ui.spacing().item_spacing.x * COLUMNS.len() as f32;
    let mut action = None;

    egui::ScrollArea::horizontal()
        .id_salt("secret_resource_table_horizontal")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.set_min_width(table_width);
            let mut table = TableBuilder::new(ui)
                .id_salt("secret_resource_table")
                .striped(true)
                .resizable(false)
                .sense(egui::Sense::click())
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .min_scrolled_height(0.0);
            table = table.column(Column::exact(SELECT_COLUMN_WIDTH));
            for width in COLUMN_WIDTHS {
                table = table.column(Column::exact(width));
            }
            table
                .header(row_height, |mut header| {
                    header.col(|_| {});
                    for label in COLUMNS {
                        header.col(|ui| {
                            ui.strong(label);
                        });
                    }
                })
                .body(|body| {
                    body.rows(row_height, row_indices.len(), |mut table_row| {
                        let Some(row) = row_indices
                            .get(table_row.index())
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
                            ui.label(&row.namespace);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.secret_type);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.data_count);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.immutable);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.keys);
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
                            crate::clipboard::copy_name_menu_item(ui, &row.name);
                            ui.separator();
                            if ui
                                .button(format!("{} Describe", egui_phosphor::regular::INFO))
                                .clicked()
                            {
                                action = Some(SecretTableAction::Describe {
                                    key: row.key.clone(),
                                });
                                ui.close();
                            }
                            if ui
                                .button(format!("{} View", egui_phosphor::regular::EYE))
                                .clicked()
                            {
                                action = Some(SecretTableAction::View {
                                    key: row.key.clone(),
                                });
                                ui.close();
                            }
                            if ui
                                .button(format!("{} Edit", egui_phosphor::regular::PENCIL_SIMPLE))
                                .clicked()
                            {
                                action = Some(SecretTableAction::Edit {
                                    key: row.key.clone(),
                                });
                                ui.close();
                            }
                            let delete_text = egui::RichText::new(format!(
                                "{} Delete",
                                egui_phosphor::regular::TRASH
                            ))
                            .color(ui.visuals().error_fg_color);
                            if ui.button(delete_text).clicked() {
                                action = Some(SecretTableAction::Delete {
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

const COLUMNS: [&str; 7] = [
    "Name",
    "Namespace",
    "Type",
    "Data",
    "Immutable",
    "Keys",
    "Age",
];
const COLUMN_WIDTHS: [f32; 7] = [240.0, 160.0, 240.0, 80.0, 100.0, 360.0, 90.0];

#[cfg(test)]
fn filter_secret_rows<'a>(rows: &'a [SecretRow], search_text: &str) -> Vec<&'a SecretRow> {
    rows.iter()
        .filter(|row| row_matches_search(row, search_text))
        .collect()
}

fn row_matches_search(row: &SecretRow, search_text: &str) -> bool {
    let needle = search_text.trim().to_lowercase();
    needle.is_empty()
        || row.name.to_lowercase().contains(&needle)
        || row.namespace.to_lowercase().contains(&needle)
        || row.secret_type.to_lowercase().contains(&needle)
        || row.keys.to_lowercase().contains(&needle)
        || row.summary.to_lowercase().contains(&needle)
}

fn secret_metadata() -> ResourceMetadata {
    ResourceMetadata {
        id: "secret".to_owned(),
        title: "Secrets".to_owned(),
        api_version: "v1".to_owned(),
        kind: "Secret".to_owned(),
        resource: miku_core::ResourceRef::core("v1", "secrets"),
        namespaced: true,
    }
}

fn default_secret_yaml(namespace: Option<&str>) -> String {
    let namespace = namespace.unwrap_or("default");
    format!(
        r#"apiVersion: v1
kind: Secret
metadata:
  name: example-secret
  namespace: {namespace}
type: Opaque
stringData: {{}}
"#
    )
}

fn secret_rows_from_list(items: &[ResourceSummary]) -> Vec<SecretRow> {
    let mut rows = items
        .iter()
        .map(SecretRow::from_summary)
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        left.namespace
            .cmp(&right.namespace)
            .then(left.name.cmp(&right.name))
    });
    rows
}

#[derive(Clone, Debug, PartialEq)]
struct SecretRow {
    key: String,
    name: String,
    namespace: String,
    secret_type: String,
    data_count: String,
    immutable: String,
    keys: String,
    summary: String,
    age: String,
    raw: serde_json::Value,
}

impl SecretRow {
    fn from_summary(summary: &ResourceSummary) -> Self {
        let raw = &summary.raw;
        let name = value_str(raw, &["metadata", "name"]).unwrap_or(&summary.name);
        let namespace = value_str(raw, &["metadata", "namespace"])
            .or(summary.namespace.as_deref())
            .unwrap_or("N/A");
        let secret_type = value_str(raw, &["type"]).unwrap_or("Opaque").to_owned();
        let keys = sorted_keys(&[raw.pointer("/data"), raw.pointer("/stringData")]);
        let keys_label = if keys.is_empty() {
            "N/A".to_owned()
        } else {
            keys.join(", ")
        };
        let data_count = keys.len();
        let immutable = value_bool(raw, &["immutable"])
            .map_or_else(|| "N/A".to_owned(), |value| value.to_string());

        Self {
            key: namespaced_key(namespace, name),
            name: name.to_owned(),
            namespace: namespace.to_owned(),
            secret_type: secret_type.clone(),
            data_count: data_count.to_string(),
            immutable,
            keys: keys_label.clone(),
            summary: format!("type={secret_type}, data={data_count}, keys={keys_label}"),
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
            namespace: namespace_value(&self.namespace),
            name: self.name.clone(),
        }
    }

    fn delete_target(&self) -> super::ResourceDeleteTarget {
        super::ResourceDeleteTarget {
            namespace: namespace_value(&self.namespace),
            name: self.name.clone(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum SecretTableAction {
    Describe { key: String },
    View { key: String },
    Edit { key: String },
    Delete { key: String },
}

#[derive(Clone, Debug, PartialEq)]
struct SecretDescribeDialog {
    key: String,
    name: String,
    describe: SecretDescribe,
}

#[derive(Clone, Debug, PartialEq)]
struct SecretViewDialog {
    key: String,
    name: String,
    yaml: String,
}

#[derive(Clone, Debug, PartialEq)]
struct SecretDescribe {
    summary: Vec<DescribeField>,
    data_keys: Vec<ResourceMapEntry>,
    string_data_keys: Vec<ResourceMapEntry>,
    labels: Vec<ResourceMapEntry>,
    annotations: Vec<ResourceMapEntry>,
    raw_yaml: String,
}

fn show_secret_describe(ui: &mut egui::Ui, describe: &SecretDescribe) {
    describe_group(ui, egui_phosphor::regular::LOCK_KEY, "Secret", |ui| {
        describe_fields(ui, &describe.summary);
    });

    ui.add_space(10.0);
    describe_group(ui, egui_phosphor::regular::KEY, "Keys", |ui| {
        ResourceMapView {
            id_salt: "secret-describe-data-keys",
            icon: egui_phosphor::regular::KEY,
            title: "Data",
            entries: &describe.data_keys,
            empty_label: "No data keys.",
        }
        .show(ui);
        ui.add_space(8.0);
        ResourceMapView {
            id_salt: "secret-describe-string-data-keys",
            icon: egui_phosphor::regular::KEY,
            title: "String data",
            entries: &describe.string_data_keys,
            empty_label: "No stringData keys.",
        }
        .show(ui);
    });

    ui.add_space(10.0);
    describe_group(ui, egui_phosphor::regular::TAG, "Metadata", |ui| {
        describe_metadata_maps(
            ui,
            "secret-describe-metadata",
            &describe.labels,
            &describe.annotations,
        );
    });

    ui.add_space(10.0);
    describe_group(ui, egui_phosphor::regular::CODE, "Raw manifest", |ui| {
        describe_raw_manifest(
            ui,
            "secret-describe-raw-manifest-content",
            &describe.raw_yaml,
        );
    });
}

fn secret_describe_from_row(row: &SecretRow) -> SecretDescribe {
    let raw = &row.raw;
    let redacted = redacted_secret_manifest(raw);
    SecretDescribe {
        summary: vec![
            DescribeField::new("Name", row.name.clone()),
            DescribeField::new("Namespace", row.namespace.clone()),
            DescribeField::new("Type", row.secret_type.clone()),
            DescribeField::new("Age", row.age.clone()),
            DescribeField::new("Immutable", row.immutable.clone()),
            DescribeField::new("Data", row.data_count.clone()),
            DescribeField::new("Keys", row.keys.clone()),
        ],
        data_keys: key_entries(raw.pointer("/data")),
        string_data_keys: key_entries(raw.pointer("/stringData")),
        labels: resource_map_entries(raw.pointer("/metadata/labels")),
        annotations: resource_map_entries(raw.pointer("/metadata/annotations")),
        raw_yaml: full_manifest_yaml(&redacted),
    }
}

fn redacted_secret_manifest(raw: &serde_json::Value) -> serde_json::Value {
    let mut redacted = raw.clone();
    for field in ["data", "stringData"] {
        if let Some(object) = redacted
            .get_mut(field)
            .and_then(serde_json::Value::as_object_mut)
        {
            for value in object.values_mut() {
                *value = serde_json::Value::String(REDACTED_SECRET_VALUE.to_owned());
            }
        }
    }
    redacted
}

fn namespaced_key(namespace: &str, name: &str) -> String {
    format!("{namespace}/{name}")
}

fn namespace_value(namespace: &str) -> Option<String> {
    if namespace.is_empty() || namespace == "N/A" {
        None
    } else {
        Some(namespace.to_owned())
    }
}

fn sorted_keys(values: &[Option<&serde_json::Value>]) -> Vec<String> {
    let mut keys = BTreeSet::new();
    for value in values {
        if let Some(object) = value.and_then(serde_json::Value::as_object) {
            keys.extend(object.keys().cloned());
        }
    }
    keys.into_iter().collect()
}

fn key_entries(value: Option<&serde_json::Value>) -> Vec<ResourceMapEntry> {
    sorted_keys(&[value])
        .into_iter()
        .map(|key| ResourceMapEntry::new(key, REDACTED_SECRET_VALUE))
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
    fn secret_request_query_uses_selected_namespace() {
        let mut panel = SecretResourcePanel {
            namespace_filter: Some("production".to_owned()),
            ..SecretResourcePanel::default()
        };

        let request = panel.request_secrets(ClusterId::new("local"));
        let query = request.query();

        assert_eq!(query.resource.plural, "secrets");
        assert_eq!(query.resource.group, None);
        assert_eq!(query.namespace.as_deref(), Some("production"));
    }

    #[test]
    fn secret_row_extracts_table_fields_from_raw_summary() {
        let row = SecretRow::from_summary(&secret_summary());

        assert_eq!(row.name, "app-secret");
        assert_eq!(row.namespace, "default");
        assert_eq!(row.secret_type, "kubernetes.io/tls");
        assert_eq!(row.data_count, "3");
        assert_eq!(row.immutable, "true");
        assert_eq!(row.keys, "password, tls.crt, token");
        assert!(row.age.ends_with(" ago"));
    }

    #[test]
    fn secret_row_handles_missing_optional_fields() {
        let row = SecretRow::from_summary(&ResourceSummary {
            name: "minimal".to_owned(),
            namespace: Some("default".to_owned()),
            kind: "Secret".to_owned(),
            status: None,
            raw: serde_json::json!({"metadata": {"name": "minimal", "namespace": "default"}}),
        });

        assert_eq!(row.secret_type, "Opaque");
        assert_eq!(row.data_count, "0");
        assert_eq!(row.immutable, "N/A");
        assert_eq!(row.keys, "N/A");
        assert_eq!(row.age, "N/A");
    }

    #[test]
    fn secret_rows_filter_by_fields_case_insensitively() {
        let rows = vec![
            SecretRow::from_summary(&secret_summary()),
            SecretRow::from_summary(&secret_summary_with_name("production", "worker-secret")),
        ];

        assert_eq!(filter_secret_rows(&rows, "APP-SECRET").len(), 1);
        assert_eq!(filter_secret_rows(&rows, "PRODUCTION").len(), 1);
        assert_eq!(filter_secret_rows(&rows, "TLS.CRT").len(), 2);
        assert_eq!(filter_secret_rows(&rows, "kubernetes.io/TLS").len(), 2);
    }

    #[test]
    fn secret_rows_are_sorted_by_namespace_and_name() {
        let rows = secret_rows_from_list(&[
            secret_summary_with_name("zeta", "worker"),
            secret_summary_with_name("default", "api-b"),
            secret_summary_with_name("default", "api-a"),
        ]);

        let keys = rows.into_iter().map(|row| row.key).collect::<Vec<_>>();
        assert_eq!(keys, vec!["default/api-a", "default/api-b", "zeta/worker"]);
    }

    #[test]
    fn secret_describe_redacts_data_and_string_data_values() {
        let row = SecretRow::from_summary(&secret_summary());
        let describe = secret_describe_from_row(&row);

        assert_eq!(describe.data_keys.len(), 2);
        assert_eq!(describe.string_data_keys.len(), 1);
        assert!(describe.raw_yaml.contains("tls.crt"));
        assert!(describe.raw_yaml.contains("password"));
        assert!(describe.raw_yaml.contains(REDACTED_SECRET_VALUE));
        assert!(!describe.raw_yaml.contains("dG9rZW4="));
        assert!(!describe.raw_yaml.contains("plain-password"));
    }

    #[test]
    fn redacted_secret_manifest_preserves_keys_and_replaces_values() {
        let redacted = redacted_secret_manifest(&secret_summary().raw);
        let yaml = full_manifest_yaml(&redacted);

        assert!(yaml.contains("token"));
        assert!(yaml.contains("password"));
        assert!(yaml.contains(REDACTED_SECRET_VALUE));
        assert!(!yaml.contains("dG9rZW4="));
        assert!(!yaml.contains("plain-password"));
    }

    #[test]
    fn stale_resource_events_do_not_replace_current_rows() {
        let mut panel = SecretResourcePanel::default();
        let cluster_id = ClusterId::new("local");
        let first = panel.request_secrets(cluster_id.clone());
        let second = panel.request_secrets(cluster_id);

        panel.apply_event(ResourceUiEvent::ResourcesLoaded {
            request: first,
            result: Ok(ResourceList {
                items: vec![secret_summary_with_name("default", "stale")],
                continue_token: None,
            }),
        });
        assert!(panel.rows.is_empty());

        panel.apply_event(ResourceUiEvent::ResourcesLoaded {
            request: second,
            result: Ok(ResourceList {
                items: vec![secret_summary()],
                continue_token: None,
            }),
        });

        assert_eq!(panel.rows[0].name, "app-secret");
    }

    #[test]
    fn stale_watch_events_do_not_replace_current_rows() {
        let mut panel = SecretResourcePanel::default();
        let cluster_id = ClusterId::new("local");
        let first = panel.request_secret_watch(cluster_id.clone());
        let second = panel.request_secret_watch(cluster_id);

        panel.apply_event(ResourceUiEvent::ResourceWatchUpdated {
            request: first,
            result: Ok(miku_api::ResourceEvent::Snapshot(ResourceList {
                items: vec![secret_summary_with_name("default", "stale")],
                continue_token: None,
            })),
        });
        assert!(panel.rows.is_empty());

        panel.apply_event(ResourceUiEvent::ResourceWatchUpdated {
            request: second,
            result: Ok(miku_api::ResourceEvent::Snapshot(ResourceList {
                items: vec![secret_summary()],
                continue_token: None,
            })),
        });

        assert_eq!(panel.rows[0].name, "app-secret");
    }

    #[test]
    fn namespace_watch_events_from_shared_request_update_selector() {
        let mut panel = SecretResourcePanel::default();
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

    #[test]
    fn edit_action_opens_edit_dialog_with_raw_editable_yaml() {
        let mut panel = SecretResourcePanel::default();
        let row = SecretRow::from_summary(&with_server_fields(secret_summary()));
        let key = row.key.clone();
        panel.rows = vec![row];

        panel.apply_table_action(Some(SecretTableAction::Edit { key }));

        let dialog = panel.edit_dialog.as_ref().unwrap();
        assert_eq!(dialog.target.namespace.as_deref(), Some("default"));
        assert_eq!(dialog.target.name, "app-secret");
        assert!(dialog.yaml.contains("dG9rZW4="));
        assert!(dialog.yaml.contains("plain-password"));
        assert!(!dialog.yaml.contains(REDACTED_SECRET_VALUE));
        let manifest = serde_yaml::from_str::<serde_json::Value>(&dialog.yaml).unwrap();
        assert!(manifest.pointer("/metadata/creationTimestamp").is_none());
        assert!(manifest.pointer("/metadata/resourceVersion").is_none());
        assert!(manifest.pointer("/metadata/managedFields").is_none());
        assert!(manifest.pointer("/status").is_none());
    }

    #[test]
    fn delete_action_opens_delete_dialog() {
        let mut panel = SecretResourcePanel::default();
        let row = SecretRow::from_summary(&secret_summary());
        let key = row.key.clone();
        panel.rows = vec![row];

        panel.apply_table_action(Some(SecretTableAction::Delete { key }));

        let dialog = panel.delete_dialog.as_ref().unwrap();
        assert_eq!(dialog.target.namespace.as_deref(), Some("default"));
        assert_eq!(dialog.target.name, "app-secret");
        assert_eq!(panel.action_error, None);
    }

    #[test]
    fn apply_completion_closes_edit_dialog_and_upserts_sorted_row() {
        let mut panel = SecretResourcePanel::default();
        let row = SecretRow::from_summary(&secret_summary());
        panel.rows = secret_rows_from_list(&[
            secret_summary_with_name("zeta", "worker"),
            secret_summary_with_name("default", "api-b"),
        ]);
        panel.edit_dialog = Some(GenericEditDialog {
            target: row.target(),
            yaml: "kind: Secret".to_owned(),
            parse_error: None,
        });
        panel.action_request_id = Some(7);
        panel.action_error = Some("old error".to_owned());

        panel.apply_event(ResourceUiEvent::ResourceActionCompleted {
            request: super::super::ResourceActionRequest {
                request_id: 7,
                cluster_id: ClusterId::new("local"),
                kind: ResourceActionKind::ApplyResource {
                    resource: secret_metadata().resource,
                    namespace: Some("default".to_owned()),
                    name: "api-a".to_owned(),
                    manifest: serde_json::json!({}),
                },
            },
            result: Ok(ResourceActionOutcome::Applied(secret_summary_with_name(
                "default", "api-a",
            ))),
        });

        assert!(panel.edit_dialog.is_none());
        assert_eq!(panel.action_error, None);
        let keys = panel
            .rows
            .iter()
            .map(|row| row.key.as_str())
            .collect::<Vec<_>>();
        assert_eq!(keys, vec!["default/api-a", "default/api-b", "zeta/worker"]);
    }

    #[test]
    fn delete_completion_closes_delete_dialog_and_removes_row_selection() {
        let mut panel = SecretResourcePanel::default();
        let row = SecretRow::from_summary(&secret_summary());
        let key = row.key.clone();
        panel.rows = vec![row.clone()];
        panel.selected_rows.insert(key.clone());
        panel.delete_dialog = Some(GenericDeleteDialog {
            target: row.delete_target(),
        });
        panel.action_request_id = Some(7);
        panel.action_error = Some("old error".to_owned());

        panel.apply_event(ResourceUiEvent::ResourceActionCompleted {
            request: super::super::ResourceActionRequest {
                request_id: 7,
                cluster_id: ClusterId::new("local"),
                kind: ResourceActionKind::DeleteResource {
                    resource: secret_metadata().resource,
                    namespace: Some("default".to_owned()),
                    name: "app-secret".to_owned(),
                },
            },
            result: Ok(ResourceActionOutcome::Deleted),
        });

        assert!(panel.rows.is_empty());
        assert!(!panel.selected_rows.contains(&key));
        assert!(panel.delete_dialog.is_none());
        assert_eq!(panel.action_error, None);
    }

    #[test]
    fn action_errors_keep_current_dialogs() {
        let row = SecretRow::from_summary(&secret_summary());
        let mut edit_panel = SecretResourcePanel {
            edit_dialog: Some(GenericEditDialog {
                target: row.target(),
                yaml: "kind: Secret".to_owned(),
                parse_error: None,
            }),
            action_request_id: Some(7),
            ..SecretResourcePanel::default()
        };

        edit_panel.apply_event(ResourceUiEvent::ResourceActionCompleted {
            request: super::super::ResourceActionRequest {
                request_id: 7,
                cluster_id: ClusterId::new("local"),
                kind: ResourceActionKind::ApplyResource {
                    resource: secret_metadata().resource,
                    namespace: Some("default".to_owned()),
                    name: "app-secret".to_owned(),
                    manifest: serde_json::json!({}),
                },
            },
            result: Err("field is immutable".to_owned()),
        });

        assert!(edit_panel.edit_dialog.is_some());
        assert_eq!(
            edit_panel.action_error.as_deref(),
            Some("field is immutable")
        );

        let mut delete_panel = SecretResourcePanel {
            delete_dialog: Some(GenericDeleteDialog {
                target: row.delete_target(),
            }),
            action_request_id: Some(9),
            ..SecretResourcePanel::default()
        };

        delete_panel.apply_event(ResourceUiEvent::ResourceActionCompleted {
            request: super::super::ResourceActionRequest {
                request_id: 9,
                cluster_id: ClusterId::new("local"),
                kind: ResourceActionKind::DeleteResource {
                    resource: secret_metadata().resource,
                    namespace: Some("default".to_owned()),
                    name: "app-secret".to_owned(),
                },
            },
            result: Err("delete denied".to_owned()),
        });

        assert!(delete_panel.delete_dialog.is_some());
        assert_eq!(delete_panel.action_error.as_deref(), Some("delete denied"));
    }

    #[test]
    fn cluster_change_clears_edit_delete_batch_and_pending_action() {
        let row = SecretRow::from_summary(&secret_summary());
        let mut panel = SecretResourcePanel {
            last_cluster_id: Some(ClusterId::new("old")),
            edit_dialog: Some(GenericEditDialog {
                target: row.target(),
                yaml: "kind: Secret".to_owned(),
                parse_error: None,
            }),
            delete_dialog: Some(GenericDeleteDialog {
                target: row.delete_target(),
            }),
            batch_delete_dialog: Some(GenericBatchDeleteDialog {
                targets: vec![row.delete_target()],
            }),
            action_request_id: Some(7),
            action_error: Some("old error".to_owned()),
            ..SecretResourcePanel::default()
        };

        panel.reset_for_cluster_change(&ClusterId::new("new"));

        assert!(panel.edit_dialog.is_none());
        assert!(panel.delete_dialog.is_none());
        assert!(panel.batch_delete_dialog.is_none());
        assert_eq!(panel.action_request_id, None);
        assert_eq!(panel.action_error, None);
    }

    fn secret_summary() -> ResourceSummary {
        secret_summary_with_name("default", "app-secret")
    }

    fn secret_summary_with_name(namespace: &str, name: &str) -> ResourceSummary {
        ResourceSummary {
            name: name.to_owned(),
            namespace: Some(namespace.to_owned()),
            kind: "Secret".to_owned(),
            status: None,
            raw: serde_json::json!({
                "metadata": {
                    "name": name,
                    "namespace": namespace,
                    "creationTimestamp": "2026-05-18T10:00:00Z",
                    "labels": {"app": "api"},
                    "annotations": {"owner": "platform"}
                },
                "type": "kubernetes.io/tls",
                "immutable": true,
                "data": {
                    "token": "dG9rZW4=",
                    "tls.crt": "Y2VydA=="
                },
                "stringData": {
                    "password": "plain-password"
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

    fn with_server_fields(mut summary: ResourceSummary) -> ResourceSummary {
        if let Some(metadata) = summary
            .raw
            .get_mut("metadata")
            .and_then(serde_json::Value::as_object_mut)
        {
            metadata.insert(
                "managedFields".to_owned(),
                serde_json::json!([{"manager": "kube-controller-manager"}]),
            );
            metadata.insert("resourceVersion".to_owned(), serde_json::json!("42"));
            metadata.insert("uid".to_owned(), serde_json::json!("uid"));
        }
        if let Some(object) = summary.raw.as_object_mut() {
            object.insert("status".to_owned(), serde_json::json!({"phase": "Active"}));
        }
        summary
    }
}
