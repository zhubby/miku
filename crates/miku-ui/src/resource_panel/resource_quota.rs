use std::collections::BTreeSet;

use eframe::egui;
use egui_extras::{Column, TableBuilder};
use miku_api::ResourceSummary;
use miku_core::{ClusterId, ResourceRef};

#[cfg(test)]
use super::ResourceLoadRequest;
use super::components::{
    DescribeField, GenericBatchDeleteDialog, GenericCreateDialog, GenericDeleteDialog,
    GenericEditDialog, ResourceBatchDeleteDialogInput, ResourceCreateDialogInput,
    ResourceCreateDialogResponse, ResourceDeleteDialogInput, ResourceDeleteDialogResponse,
    ResourceEditDialogInput, ResourceEditDialogResponse, ResourceMapEntry, ResourceMapView,
    ResourceMetadata, ResourceRowTarget, ResourceToolbar, ResourceYamlViewDialog,
    SELECT_COLUMN_WIDTH, apply_resource_request, batch_delete_resource_request,
    default_resource_yaml, delete_resource_request, describe_fields, describe_group,
    describe_metadata_maps, describe_raw_manifest, edit_resource_request, editable_resource_yaml,
    resource_map_entries, selected_delete_targets, show_resource_batch_delete_dialog,
    show_resource_create_dialog, show_resource_delete_dialog, show_resource_describe_window,
    show_resource_edit_dialog, show_row_selection_checkbox, visible_keys,
};
use super::{
    LoadStatus, ResourceActionKind, ResourceActionOutcome, ResourceLoadKind, ResourcePanelRequests,
    ResourceUiEvent, ResourceWatchRequest, namespaces_from_list,
};
use crate::time::human_age_from_rfc3339;

#[derive(Clone, Debug, Default)]
pub(crate) struct ResourceQuotaResourcePanel {
    namespace_filter: Option<String>,
    search_text: String,
    namespaces: Vec<String>,
    namespace_status: LoadStatus,
    row_status: LoadStatus,
    rows: Vec<ResourceQuotaRow>,
    selected_rows: BTreeSet<String>,
    next_request_id: u64,
    namespace_request_id: Option<u64>,
    row_request_id: Option<u64>,
    namespace_watch_request_id: Option<u64>,
    row_watch_request_id: Option<u64>,
    last_cluster_id: Option<ClusterId>,
    describe_dialog: Option<ResourceQuotaDescribeDialog>,
    view_dialog: Option<ResourceQuotaViewDialog>,
    edit_dialog: Option<GenericEditDialog>,
    create_dialog: Option<GenericCreateDialog>,
    batch_delete_dialog: Option<GenericBatchDeleteDialog>,
    delete_dialog: Option<GenericDeleteDialog>,
    action_request_id: Option<u64>,
    action_error: Option<String>,
}

impl ResourceQuotaResourcePanel {
    pub(crate) fn show(
        &mut self,
        ui: &mut egui::Ui,
        cluster_id: Option<&ClusterId>,
    ) -> ResourcePanelRequests {
        let mut requests = ResourcePanelRequests::default();
        let Some(cluster_id) = cluster_id else {
            ui.centered_and_justified(|ui| {
                ui.label("Select a cluster to load resource quotas.");
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
                .push(self.request_resource_quota_watch(cluster_id.clone()));
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
                ResourceLoadKind::ResourceQuotas { .. } => {
                    if self.row_request_id != Some(request.request_id) {
                        return;
                    }
                    self.row_request_id = None;
                    match result {
                        Ok(list) => {
                            self.replace_rows(resource_quota_rows_from_list(&list.items));
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
                | ResourceLoadKind::Secrets { .. }
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
                ResourceLoadKind::ResourceQuotas { .. } => {
                    if self.row_watch_request_id != Some(request.request_id) {
                        return;
                    }
                    match result {
                        Ok(miku_api::ResourceEvent::Snapshot(list)) => {
                            self.replace_rows(resource_quota_rows_from_list(&list.items));
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
                | ResourceLoadKind::Secrets { .. }
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
                        self.upsert_row(ResourceQuotaRow::from_summary(&summary));
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
                            && resource == resource_quota_metadata().resource
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
            id_salt: "resource_quota_toolbar",
            namespaces: &self.namespaces,
            namespace_filter: &mut self.namespace_filter,
            search_text: &mut self.search_text,
            search_hint: "Search ResourceQuotas...",
            item_count,
            selected_count: self.selected_rows.len(),
            loading: matches!(self.row_status, LoadStatus::Loading),
        }
        .show(ui);

        if response.namespace_changed {
            requests
                .watches
                .push(self.request_resource_quota_watch(cluster_id.clone()));
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
                .push(self.request_resource_quota_watch(cluster_id.clone()));
        }
        if response.create_clicked {
            self.create_dialog = Some(GenericCreateDialog {
                yaml: default_resource_yaml(
                    resource_quota_metadata(),
                    self.namespace_filter.as_deref(),
                ),
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
                    ui.label("Loading resource quotas...");
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
                        ui.label("No resource quotas match the current filters.");
                    });
                    return;
                }
                let action =
                    show_resource_quota_table(ui, &self.rows, row_indices, &mut self.selected_rows);
                self.apply_table_action(action);
            }
        }
    }

    fn apply_table_action(&mut self, action: Option<ResourceQuotaTableAction>) {
        match action {
            Some(ResourceQuotaTableAction::Describe { key }) => {
                let Some((name, describe)) = self
                    .row_by_key(&key)
                    .map(|row| (row.name.clone(), resource_quota_describe_from_row(row)))
                else {
                    return;
                };
                self.describe_dialog = Some(ResourceQuotaDescribeDialog {
                    key,
                    name,
                    describe,
                });
            }
            Some(ResourceQuotaTableAction::View { key }) => {
                let Some((name, yaml)) = self
                    .row_by_key(&key)
                    .map(|row| (row.name.clone(), full_manifest_yaml(&row.raw)))
                else {
                    return;
                };
                self.view_dialog = Some(ResourceQuotaViewDialog { key, name, yaml });
            }
            Some(ResourceQuotaTableAction::Edit { key }) => {
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
            Some(ResourceQuotaTableAction::Delete { key }) => {
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
            egui::Id::new(("resource_quota-describe", &dialog.key)),
            format!("Describe {}", dialog.name),
            &mut open,
            |ui| {
                show_resource_quota_describe(ui, &dialog.describe);
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
            id: egui::Id::new(("resource_quota-view", &dialog.key)),
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
                metadata: resource_quota_metadata(),
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
                    resource_quota_metadata(),
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
                metadata: resource_quota_metadata(),
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
                    resource_quota_metadata(),
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
                metadata: resource_quota_metadata(),
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
                    resource_quota_metadata(),
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
                metadata: resource_quota_metadata(),
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
                    resource_quota_metadata(),
                    dialog.target,
                );
                self.action_request_id = Some(request.request_id);
                requests.push(request);
            }
        }
    }

    #[cfg(test)]
    fn request_resource_quotas(&mut self, cluster_id: ClusterId) -> ResourceLoadRequest {
        let request = ResourceLoadRequest {
            request_id: self.allocate_request_id(),
            cluster_id,
            kind: ResourceLoadKind::ResourceQuotas {
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

    fn request_resource_quota_watch(&mut self, cluster_id: ClusterId) -> ResourceWatchRequest {
        let request = ResourceWatchRequest {
            request_id: self.allocate_request_id(),
            cluster_id,
            kind: ResourceLoadKind::ResourceQuotas {
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

    fn row_by_key(&self, key: &str) -> Option<&ResourceQuotaRow> {
        self.rows.iter().find(|row| row.key == key)
    }

    fn replace_rows(&mut self, rows: Vec<ResourceQuotaRow>) {
        let targets = rows
            .iter()
            .map(ResourceQuotaRow::target)
            .collect::<Vec<_>>();
        let visible_keys = visible_keys(&targets);
        self.selected_rows.retain(|key| visible_keys.contains(key));
        self.rows = rows;
    }

    fn upsert_row(&mut self, row: ResourceQuotaRow) {
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
            .map(ResourceQuotaRow::target)
            .collect::<Vec<_>>();
        let visible_keys = visible_keys(&targets);
        self.selected_rows.retain(|key| visible_keys.contains(key));
    }

    fn selected_delete_targets(&self) -> Vec<super::ResourceDeleteTarget> {
        let targets = self
            .rows
            .iter()
            .map(ResourceQuotaRow::target)
            .collect::<Vec<_>>();
        selected_delete_targets(&targets, &self.selected_rows)
    }
}

fn show_resource_quota_table(
    ui: &mut egui::Ui,
    rows: &[ResourceQuotaRow],
    row_indices: Vec<usize>,
    selected_rows: &mut BTreeSet<String>,
) -> Option<ResourceQuotaTableAction> {
    let row_height = ui.spacing().interact_size.y;
    let widths = [240.0, 160.0, 180.0, 360.0, 360.0, 90.0];
    let table_width = SELECT_COLUMN_WIDTH
        + widths.iter().sum::<f32>()
        + ui.spacing().item_spacing.x * widths.len() as f32;
    let mut action = None;
    egui::ScrollArea::horizontal()
        .id_salt("resource_quota_table_horizontal")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.set_min_width(table_width);
            let mut table = TableBuilder::new(ui)
                .id_salt("resource_quota_table")
                .striped(true)
                .resizable(false)
                .sense(egui::Sense::click())
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .min_scrolled_height(0.0);
            table = table.column(Column::exact(SELECT_COLUMN_WIDTH));
            for width in widths {
                table = table.column(Column::exact(width));
            }
            table
                .header(row_height, |mut header| {
                    header.col(|_| {});
                    for label in ["Name", "Namespace", "Scopes", "Hard", "Used", "Age"] {
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
                            ui.label(&row.scopes);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.hard);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.used);
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
                                action = Some(ResourceQuotaTableAction::Describe {
                                    key: row.key.clone(),
                                });
                                ui.close();
                            }
                            if ui
                                .button(format!("{} View", egui_phosphor::regular::EYE))
                                .clicked()
                            {
                                action = Some(ResourceQuotaTableAction::View {
                                    key: row.key.clone(),
                                });
                                ui.close();
                            }
                            if ui
                                .button(format!("{} Edit", egui_phosphor::regular::PENCIL_SIMPLE))
                                .clicked()
                            {
                                action = Some(ResourceQuotaTableAction::Edit {
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
                                action = Some(ResourceQuotaTableAction::Delete {
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

fn row_matches_search(row: &ResourceQuotaRow, search_text: &str) -> bool {
    let needle = search_text.trim().to_lowercase();
    needle.is_empty()
        || row.name.to_lowercase().contains(&needle)
        || row.namespace.to_lowercase().contains(&needle)
        || row.scopes.to_lowercase().contains(&needle)
        || row.hard.to_lowercase().contains(&needle)
        || row.used.to_lowercase().contains(&needle)
}

#[cfg(test)]
fn filter_resource_quota_rows<'a>(
    rows: &'a [ResourceQuotaRow],
    search_text: &str,
) -> Vec<&'a ResourceQuotaRow> {
    rows.iter()
        .filter(|row| row_matches_search(row, search_text))
        .collect()
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

fn resource_quota_metadata() -> ResourceMetadata {
    ResourceMetadata {
        id: "resource_quota".to_owned(),
        title: "ResourceQuotas".to_owned(),
        api_version: "v1".to_owned(),
        kind: "ResourceQuota".to_owned(),
        resource: ResourceRef::core("v1", "resourcequotas"),
        namespaced: true,
    }
}

fn resource_quota_rows_from_list(items: &[ResourceSummary]) -> Vec<ResourceQuotaRow> {
    let mut rows = items
        .iter()
        .map(ResourceQuotaRow::from_summary)
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        left.namespace
            .cmp(&right.namespace)
            .then(left.name.cmp(&right.name))
    });
    rows
}

#[derive(Clone, Debug, PartialEq)]
struct ResourceQuotaRow {
    key: String,
    name: String,
    namespace: String,
    scopes: String,
    hard: String,
    used: String,
    age: String,
    raw: serde_json::Value,
}

impl ResourceQuotaRow {
    fn from_summary(summary: &ResourceSummary) -> Self {
        let raw = &summary.raw;
        let name = value_str(raw, &["metadata", "name"]).unwrap_or(&summary.name);
        let namespace = value_str(raw, &["metadata", "namespace"])
            .or(summary.namespace.as_deref())
            .unwrap_or("N/A");
        Self {
            key: namespaced_key(namespace, name),
            name: name.to_owned(),
            namespace: namespace.to_owned(),
            scopes: string_array(raw.pointer("/spec/scopes")),
            hard: resource_map(raw.pointer("/status/hard"))
                .or_else(|| resource_map(raw.pointer("/spec/hard")))
                .unwrap_or_else(|| "N/A".to_owned()),
            used: resource_map(raw.pointer("/status/used")).unwrap_or_else(|| "N/A".to_owned()),
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
enum ResourceQuotaTableAction {
    Describe { key: String },
    View { key: String },
    Edit { key: String },
    Delete { key: String },
}

#[derive(Clone, Debug, PartialEq)]
struct ResourceQuotaDescribeDialog {
    key: String,
    name: String,
    describe: ResourceQuotaDescribe,
}

#[derive(Clone, Debug, PartialEq)]
struct ResourceQuotaViewDialog {
    key: String,
    name: String,
    yaml: String,
}

#[derive(Clone, Debug, PartialEq)]
struct ResourceQuotaDescribe {
    summary: Vec<DescribeField>,
    hard: Vec<ResourceMapEntry>,
    used: Vec<ResourceMapEntry>,
    labels: Vec<ResourceMapEntry>,
    annotations: Vec<ResourceMapEntry>,
    raw_yaml: String,
}

fn resource_quota_describe_from_row(row: &ResourceQuotaRow) -> ResourceQuotaDescribe {
    let raw = &row.raw;
    ResourceQuotaDescribe {
        summary: vec![
            DescribeField::new("Name", row.name.clone()),
            DescribeField::new("Namespace", row.namespace.clone()),
            DescribeField::new("Scopes", row.scopes.clone()),
            DescribeField::new("Age", row.age.clone()),
        ],
        hard: resource_quota_hard_entries(raw),
        used: resource_map_entries(raw.pointer("/status/used")),
        labels: resource_map_entries(raw.pointer("/metadata/labels")),
        annotations: resource_map_entries(raw.pointer("/metadata/annotations")),
        raw_yaml: full_manifest_yaml(raw),
    }
}

fn show_resource_quota_describe(ui: &mut egui::Ui, describe: &ResourceQuotaDescribe) {
    describe_group(ui, egui_phosphor::regular::GAUGE, "ResourceQuota", |ui| {
        describe_fields(ui, &describe.summary);
    });

    ui.add_space(10.0);
    describe_group(ui, egui_phosphor::regular::GAUGE, "Usage", |ui| {
        ResourceMapView {
            id_salt: "resource-quota-describe-hard",
            icon: egui_phosphor::regular::GAUGE,
            title: "Hard",
            entries: &describe.hard,
            empty_label: "No hard quota values.",
        }
        .show(ui);
        ui.add_space(8.0);
        ResourceMapView {
            id_salt: "resource-quota-describe-used",
            icon: egui_phosphor::regular::GAUGE,
            title: "Used",
            entries: &describe.used,
            empty_label: "No used quota values.",
        }
        .show(ui);
    });

    ui.add_space(10.0);
    describe_group(ui, egui_phosphor::regular::TAG, "Metadata", |ui| {
        describe_metadata_maps(
            ui,
            "resource-quota-describe-metadata",
            &describe.labels,
            &describe.annotations,
        );
    });

    ui.add_space(10.0);
    describe_group(ui, egui_phosphor::regular::CODE, "Raw manifest", |ui| {
        describe_raw_manifest(
            ui,
            "resource-quota-describe-raw-manifest",
            &describe.raw_yaml,
        );
    });
}

fn resource_quota_hard_entries(raw: &serde_json::Value) -> Vec<ResourceMapEntry> {
    let status_hard = resource_map_entries(raw.pointer("/status/hard"));
    if status_hard.is_empty() {
        resource_map_entries(raw.pointer("/spec/hard"))
    } else {
        status_hard
    }
}

fn string_array(value: Option<&serde_json::Value>) -> String {
    let values = value
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    if values.is_empty() {
        "N/A".to_owned()
    } else {
        values.join(", ")
    }
}

fn resource_map(value: Option<&serde_json::Value>) -> Option<String> {
    let mut entries = value
        .and_then(serde_json::Value::as_object)?
        .iter()
        .map(|(key, value)| {
            let value = value
                .as_str()
                .map_or_else(|| value.to_string(), ToOwned::to_owned);
            format!("{key}={value}")
        })
        .collect::<Vec<_>>();
    entries.sort();
    (!entries.is_empty()).then(|| entries.join(", "))
}

fn value_str<'a>(value: &'a serde_json::Value, path: &[&str]) -> Option<&'a str> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    current.as_str()
}

fn full_manifest_yaml(raw: &serde_json::Value) -> String {
    serde_yaml::to_string(raw)
        .or_else(|_| serde_json::to_string_pretty(raw))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use miku_api::ResourceList;

    #[test]
    fn resource_quota_request_query_uses_selected_namespace() {
        let mut panel = ResourceQuotaResourcePanel {
            namespace_filter: Some("production".to_owned()),
            ..ResourceQuotaResourcePanel::default()
        };
        let query = panel
            .request_resource_quotas(ClusterId::new("local"))
            .query();
        assert_eq!(query.resource.plural, "resourcequotas");
        assert_eq!(query.namespace.as_deref(), Some("production"));
    }

    #[test]
    fn resource_quota_row_extracts_fields() {
        let row = ResourceQuotaRow::from_summary(&quota_summary());
        assert_eq!(row.name, "compute");
        assert_eq!(row.namespace, "default");
        assert_eq!(row.scopes, "BestEffort, NotTerminating");
        assert_eq!(row.hard, "limits.cpu=4, pods=10, requests.memory=8Gi");
        assert_eq!(row.used, "limits.cpu=2, pods=4, requests.memory=2Gi");
        assert!(row.age.ends_with(" ago"));
    }

    #[test]
    fn resource_quota_describe_extracts_hard_and_used_entries() {
        let row = ResourceQuotaRow::from_summary(&quota_summary());
        let describe = resource_quota_describe_from_row(&row);

        assert!(
            describe
                .hard
                .iter()
                .any(|entry| { entry.key == "limits.cpu" && entry.value == "4" })
        );
        assert!(
            describe
                .hard
                .iter()
                .any(|entry| { entry.key == "requests.memory" && entry.value == "8Gi" })
        );
        assert!(
            describe
                .used
                .iter()
                .any(|entry| { entry.key == "limits.cpu" && entry.value == "2" })
        );
        assert!(
            describe
                .used
                .iter()
                .any(|entry| { entry.key == "requests.memory" && entry.value == "2Gi" })
        );
    }

    #[test]
    fn resource_quota_describe_hard_entries_fall_back_to_spec() {
        let row = ResourceQuotaRow::from_summary(&ResourceSummary {
            name: "spec-only".to_owned(),
            namespace: Some("default".to_owned()),
            kind: "ResourceQuota".to_owned(),
            status: None,
            raw: serde_json::json!({
                "metadata": {"name": "spec-only", "namespace": "default"},
                "spec": {"hard": {"pods": "5"}}
            }),
        });

        let describe = resource_quota_describe_from_row(&row);

        assert_eq!(describe.hard, vec![ResourceMapEntry::new("pods", "5")]);
        assert!(describe.used.is_empty());
    }

    #[test]
    fn resource_quota_row_handles_missing_fields() {
        let row = ResourceQuotaRow::from_summary(&ResourceSummary {
            name: "minimal".to_owned(),
            namespace: Some("default".to_owned()),
            kind: "ResourceQuota".to_owned(),
            status: None,
            raw: serde_json::json!({"metadata": {"name": "minimal", "namespace": "default"}}),
        });
        assert_eq!(row.scopes, "N/A");
        assert_eq!(row.hard, "N/A");
        assert_eq!(row.used, "N/A");
        assert_eq!(row.age, "N/A");
    }

    #[test]
    fn resource_quota_rows_filter_and_sort() {
        let rows = resource_quota_rows_from_list(&[
            quota_summary_with_name("zeta", "worker"),
            quota_summary_with_name("default", "api-b"),
            quota_summary_with_name("default", "api-a"),
        ]);
        let keys = rows.iter().map(|row| row.key.as_str()).collect::<Vec<_>>();
        assert_eq!(keys, vec!["default/api-a", "default/api-b", "zeta/worker"]);
        assert_eq!(
            filter_resource_quota_rows(&rows, "REQUESTS.MEMORY").len(),
            3
        );
        assert_eq!(filter_resource_quota_rows(&rows, "ZETA").len(), 1);
    }

    #[test]
    fn stale_watch_events_do_not_replace_current_rows() {
        let mut panel = ResourceQuotaResourcePanel::default();
        let cluster_id = ClusterId::new("local");
        let first = panel.request_resource_quota_watch(cluster_id.clone());
        let second = panel.request_resource_quota_watch(cluster_id);
        panel.apply_event(ResourceUiEvent::ResourceWatchUpdated {
            request: first,
            result: Ok(miku_api::ResourceEvent::Snapshot(ResourceList {
                items: vec![quota_summary_with_name("default", "stale")],
                continue_token: None,
            })),
        });
        assert!(panel.rows.is_empty());
        panel.apply_event(ResourceUiEvent::ResourceWatchUpdated {
            request: second,
            result: Ok(miku_api::ResourceEvent::Snapshot(ResourceList {
                items: vec![quota_summary()],
                continue_token: None,
            })),
        });
        assert_eq!(panel.rows[0].name, "compute");
    }

    #[test]
    fn namespace_watch_events_from_shared_request_update_selector() {
        let mut panel = ResourceQuotaResourcePanel::default();
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
    fn edit_action_opens_edit_dialog_with_editable_yaml() {
        let mut panel = ResourceQuotaResourcePanel::default();
        let row = ResourceQuotaRow::from_summary(&with_server_fields(quota_summary()));
        let key = row.key.clone();
        panel.rows = vec![row];

        panel.apply_table_action(Some(ResourceQuotaTableAction::Edit { key }));

        let dialog = panel.edit_dialog.as_ref().unwrap();
        assert_eq!(dialog.target.namespace.as_deref(), Some("default"));
        assert_eq!(dialog.target.name, "compute");
        let manifest = serde_yaml::from_str::<serde_json::Value>(&dialog.yaml).unwrap();
        assert!(manifest.pointer("/metadata/creationTimestamp").is_none());
        assert!(manifest.pointer("/metadata/resourceVersion").is_none());
        assert!(manifest.pointer("/metadata/managedFields").is_none());
        assert!(manifest.pointer("/status").is_none());
    }

    #[test]
    fn delete_action_opens_delete_dialog() {
        let mut panel = ResourceQuotaResourcePanel::default();
        let row = ResourceQuotaRow::from_summary(&quota_summary());
        let key = row.key.clone();
        panel.rows = vec![row];

        panel.apply_table_action(Some(ResourceQuotaTableAction::Delete { key }));

        let dialog = panel.delete_dialog.as_ref().unwrap();
        assert_eq!(dialog.target.namespace.as_deref(), Some("default"));
        assert_eq!(dialog.target.name, "compute");
        assert_eq!(panel.action_error, None);
    }

    #[test]
    fn apply_completion_closes_edit_dialog_and_upserts_sorted_row() {
        let mut panel = ResourceQuotaResourcePanel::default();
        let row = ResourceQuotaRow::from_summary(&quota_summary());
        panel.rows = resource_quota_rows_from_list(&[
            quota_summary_with_name("zeta", "worker"),
            quota_summary_with_name("default", "api-b"),
        ]);
        panel.edit_dialog = Some(GenericEditDialog {
            target: row.target(),
            yaml: "kind: ResourceQuota".to_owned(),
            parse_error: None,
        });
        panel.action_request_id = Some(7);
        panel.action_error = Some("old error".to_owned());

        panel.apply_event(ResourceUiEvent::ResourceActionCompleted {
            request: super::super::ResourceActionRequest {
                request_id: 7,
                cluster_id: ClusterId::new("local"),
                kind: ResourceActionKind::ApplyResource {
                    resource: resource_quota_metadata().resource,
                    namespace: Some("default".to_owned()),
                    name: "api-a".to_owned(),
                    manifest: serde_json::json!({}),
                },
            },
            result: Ok(ResourceActionOutcome::Applied(quota_summary_with_name(
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
        let mut panel = ResourceQuotaResourcePanel::default();
        let row = ResourceQuotaRow::from_summary(&quota_summary());
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
                    resource: resource_quota_metadata().resource,
                    namespace: Some("default".to_owned()),
                    name: "compute".to_owned(),
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
        let row = ResourceQuotaRow::from_summary(&quota_summary());
        let mut edit_panel = ResourceQuotaResourcePanel {
            edit_dialog: Some(GenericEditDialog {
                target: row.target(),
                yaml: "kind: ResourceQuota".to_owned(),
                parse_error: None,
            }),
            action_request_id: Some(7),
            ..ResourceQuotaResourcePanel::default()
        };

        edit_panel.apply_event(ResourceUiEvent::ResourceActionCompleted {
            request: super::super::ResourceActionRequest {
                request_id: 7,
                cluster_id: ClusterId::new("local"),
                kind: ResourceActionKind::ApplyResource {
                    resource: resource_quota_metadata().resource,
                    namespace: Some("default".to_owned()),
                    name: "compute".to_owned(),
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

        let mut delete_panel = ResourceQuotaResourcePanel {
            delete_dialog: Some(GenericDeleteDialog {
                target: row.delete_target(),
            }),
            action_request_id: Some(9),
            ..ResourceQuotaResourcePanel::default()
        };

        delete_panel.apply_event(ResourceUiEvent::ResourceActionCompleted {
            request: super::super::ResourceActionRequest {
                request_id: 9,
                cluster_id: ClusterId::new("local"),
                kind: ResourceActionKind::DeleteResource {
                    resource: resource_quota_metadata().resource,
                    namespace: Some("default".to_owned()),
                    name: "compute".to_owned(),
                },
            },
            result: Err("delete denied".to_owned()),
        });

        assert!(delete_panel.delete_dialog.is_some());
        assert_eq!(delete_panel.action_error.as_deref(), Some("delete denied"));
    }

    #[test]
    fn cluster_change_clears_edit_delete_batch_and_pending_action() {
        let row = ResourceQuotaRow::from_summary(&quota_summary());
        let mut panel = ResourceQuotaResourcePanel {
            last_cluster_id: Some(ClusterId::new("old")),
            edit_dialog: Some(GenericEditDialog {
                target: row.target(),
                yaml: "kind: ResourceQuota".to_owned(),
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
            ..ResourceQuotaResourcePanel::default()
        };

        panel.reset_for_cluster_change(&ClusterId::new("new"));

        assert!(panel.edit_dialog.is_none());
        assert!(panel.delete_dialog.is_none());
        assert!(panel.batch_delete_dialog.is_none());
        assert_eq!(panel.action_request_id, None);
        assert_eq!(panel.action_error, None);
    }

    fn quota_summary() -> ResourceSummary {
        quota_summary_with_name("default", "compute")
    }

    fn quota_summary_with_name(namespace: &str, name: &str) -> ResourceSummary {
        ResourceSummary {
            name: name.to_owned(),
            namespace: Some(namespace.to_owned()),
            kind: "ResourceQuota".to_owned(),
            status: None,
            raw: serde_json::json!({
                "metadata": {
                    "name": name,
                    "namespace": namespace,
                    "creationTimestamp": "2026-05-18T10:00:00Z"
                },
                "spec": {
                    "scopes": ["BestEffort", "NotTerminating"],
                    "hard": {"pods": "10"}
                },
                "status": {
                    "hard": {"pods": "10", "limits.cpu": "4", "requests.memory": "8Gi"},
                    "used": {"pods": "4", "limits.cpu": "2", "requests.memory": "2Gi"}
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
        summary
    }
}
