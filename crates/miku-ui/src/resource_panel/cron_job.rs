use std::collections::BTreeSet;

use eframe::egui;
use egui_extras::{Column, TableBuilder};
use miku_api::ResourceSummary;
use miku_core::{ClusterId, ResourceRef};

#[cfg(test)]
use super::ResourceLoadRequest;
#[cfg(test)]
use super::components::parse_resource_apply_yaml;
use super::components::{
    ContainerTemplateDescribe, DescribeCondition, DescribeField, GenericBatchDeleteDialog,
    GenericCreateDialog, GenericDeleteDialog, GenericEditDialog, ResourceBatchDeleteDialogInput,
    ResourceCreateDialogInput, ResourceCreateDialogResponse, ResourceDeleteDialogInput,
    ResourceDeleteDialogResponse, ResourceEditDialogInput, ResourceEditDialogResponse,
    ResourceMapEntry, ResourceMapView, ResourceMetadata, ResourceRowTarget, ResourceToolbar,
    ResourceYamlViewDialog, SELECT_COLUMN_WIDTH, apply_resource_request,
    batch_delete_resource_request, condition_describes, container_template_describes,
    delete_resource_request, describe_conditions, describe_container_templates, describe_fields,
    describe_group, describe_raw_manifest, edit_resource_request, editable_resource_yaml,
    selected_delete_targets, show_resource_batch_delete_dialog, show_resource_create_dialog,
    show_resource_delete_dialog, show_resource_describe_window, show_resource_edit_dialog,
    show_row_selection_checkbox, visible_keys,
};
use super::{
    LoadStatus, ResourceActionKind, ResourceActionOutcome, ResourceLoadKind, ResourcePanelRequests,
    ResourceUiEvent, ResourceWatchRequest, namespaces_from_list,
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
    selected_rows: BTreeSet<String>,
    next_request_id: u64,
    namespace_request_id: Option<u64>,
    row_request_id: Option<u64>,
    namespace_watch_request_id: Option<u64>,
    row_watch_request_id: Option<u64>,
    last_cluster_id: Option<ClusterId>,
    describe_dialog: Option<CronJobDescribeDialog>,
    view_dialog: Option<CronJobViewDialog>,
    edit_dialog: Option<GenericEditDialog>,
    create_dialog: Option<GenericCreateDialog>,
    batch_delete_dialog: Option<GenericBatchDeleteDialog>,
    delete_dialog: Option<GenericDeleteDialog>,
    action_request_id: Option<u64>,
    action_error: Option<String>,
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
                ResourceLoadKind::CronJobs { .. } => {
                    if self.row_request_id != Some(request.request_id) {
                        return;
                    }
                    self.row_request_id = None;
                    match result {
                        Ok(list) => {
                            self.replace_rows(cron_job_rows_from_list(&list.items));
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
                ResourceLoadKind::CronJobs { .. } => {
                    if self.row_watch_request_id != Some(request.request_id) {
                        return;
                    }
                    match result {
                        Ok(miku_api::ResourceEvent::Snapshot(list)) => {
                            self.replace_rows(cron_job_rows_from_list(&list.items));
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
                        self.upsert_row(CronJobRow::from_summary(&summary));
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
                            && resource == cron_job_metadata().resource
                        {
                            let key = cron_job_key(namespace.as_deref().unwrap_or(""), &name);
                            self.rows.retain(|row| row.key != key);
                            self.selected_rows.remove(&key);
                        }
                        self.delete_dialog = None;
                        self.action_error = None;
                    }
                    Ok(ResourceActionOutcome::BatchDeleted(targets)) => {
                        for target in targets {
                            let key = cron_job_key(
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
            id_salt: "cron_job_resource_toolbar",
            namespaces: &self.namespaces,
            namespace_filter: &mut self.namespace_filter,
            search_text: &mut self.search_text,
            search_hint: "Search CronJobs...",
            item_count,
            selected_count: self.selected_rows.len(),
            loading: matches!(self.row_status, LoadStatus::Loading),
        }
        .show(ui);

        if response.namespace_changed {
            requests
                .watches
                .push(self.request_cron_job_watch(cluster_id.clone()));
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
                .push(self.request_cron_job_watch(cluster_id.clone()));
        }
        if response.create_clicked {
            self.create_dialog = Some(GenericCreateDialog {
                yaml: default_cron_job_yaml(self.namespace_filter.as_deref()),
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

                let action =
                    show_cron_job_table(ui, &self.rows, row_indices, &mut self.selected_rows);
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
            Some(CronJobTableAction::Edit { key }) => {
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
            Some(CronJobTableAction::Delete { key }) => {
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
            egui::Id::new(("cron_job-describe-dialog", &dialog.key)),
            format!("Describe {}", dialog.name),
            &mut open,
            |ui| {
                show_cron_job_describe(ui, &dialog.describe);
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
                metadata: cron_job_metadata(),
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
                    cron_job_metadata(),
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
                metadata: cron_job_metadata(),
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
                    cron_job_metadata(),
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
                metadata: cron_job_metadata(),
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
                    cron_job_metadata(),
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
                metadata: cron_job_metadata(),
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
                    cron_job_metadata(),
                    dialog.target,
                );
                self.action_request_id = Some(request.request_id);
                requests.push(request);
            }
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

    fn replace_rows(&mut self, rows: Vec<CronJobRow>) {
        let targets = rows.iter().map(CronJobRow::target).collect::<Vec<_>>();
        let visible_keys = visible_keys(&targets);
        self.selected_rows.retain(|key| visible_keys.contains(key));
        self.rows = rows;
    }

    fn upsert_row(&mut self, row: CronJobRow) {
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
            .map(CronJobRow::target)
            .collect::<Vec<_>>();
        let visible_keys = visible_keys(&targets);
        self.selected_rows.retain(|key| visible_keys.contains(key));
    }

    fn selected_delete_targets(&self) -> Vec<super::ResourceDeleteTarget> {
        let targets = self.rows.iter().map(CronJobRow::target).collect::<Vec<_>>();
        selected_delete_targets(&targets, &self.selected_rows)
    }
}

fn show_cron_job_table(
    ui: &mut egui::Ui,
    rows: &[CronJobRow],
    row_indices: Vec<usize>,
    selected_rows: &mut BTreeSet<String>,
) -> Option<CronJobTableAction> {
    let row_height = ui.spacing().interact_size.y;
    let table_width: f32 = SELECT_COLUMN_WIDTH
        + CRON_JOB_COLUMN_WIDTHS.iter().sum::<f32>()
        + ui.spacing().item_spacing.x * CRON_JOB_COLUMNS.len() as f32;
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

            table = table.column(Column::exact(SELECT_COLUMN_WIDTH));
            for width in CRON_JOB_COLUMN_WIDTHS {
                table = table.column(Column::exact(width));
            }

            table
                .header(row_height, |mut header| {
                    header.col(|_| {});
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
                            if ui
                                .button(format!("{} Edit", egui_phosphor::regular::PENCIL_SIMPLE))
                                .clicked()
                            {
                                action = Some(CronJobTableAction::Edit {
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
                                action = Some(CronJobTableAction::Delete {
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

fn cron_job_metadata() -> ResourceMetadata {
    ResourceMetadata {
        id: "cron_job".to_owned(),
        title: "CronJobs".to_owned(),
        api_version: "batch/v1".to_owned(),
        kind: "CronJob".to_owned(),
        resource: ResourceRef::grouped("batch", "v1", "cronjobs"),
        namespaced: true,
    }
}

fn default_cron_job_yaml(namespace: Option<&str>) -> String {
    let namespace = namespace.unwrap_or("default");
    format!(
        r#"apiVersion: batch/v1
kind: CronJob
metadata:
  name: example-cron-job
  namespace: {namespace}
spec:
  schedule: "*/5 * * * *"
  concurrencyPolicy: Forbid
  successfulJobsHistoryLimit: 3
  failedJobsHistoryLimit: 1
  jobTemplate:
    spec:
      template:
        spec:
          restartPolicy: OnFailure
          containers:
            - name: app
              image: busybox:latest
              command:
                - /bin/sh
                - -c
                - date
"#
    )
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
enum CronJobTableAction {
    Describe { key: String },
    View { key: String },
    Edit { key: String },
    Delete { key: String },
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
    containers: Vec<ContainerTemplateDescribe>,
    conditions: Vec<DescribeCondition>,
    labels: Vec<ResourceMapEntry>,
    annotations: Vec<ResourceMapEntry>,
    raw_yaml: String,
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
        describe_container_templates(ui, &describe.containers);
    });

    ui.add_space(10.0);
    describe_group(
        ui,
        egui_phosphor::regular::CHECK_CIRCLE,
        "Conditions",
        |ui| {
            describe_conditions(ui, "cron_job-describe-conditions", &describe.conditions);
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
        describe_raw_manifest(
            ui,
            "cron_job-describe-raw-manifest-content",
            &describe.raw_yaml,
        );
    });
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
        containers: container_template_describes(
            raw,
            "/spec/jobTemplate/spec/template/spec/containers",
        ),
        conditions: condition_describes(raw.pointer("/status/conditions")),
        labels: string_map_entries(raw.pointer("/metadata/labels")),
        annotations: string_map_entries(raw.pointer("/metadata/annotations")),
        raw_yaml: full_manifest_yaml(raw),
    }
}

fn cron_job_key(namespace: &str, name: &str) -> String {
    format!("{namespace}/{name}")
}

fn namespace_value(namespace: &str) -> Option<String> {
    if namespace.is_empty() || namespace == "N/A" {
        None
    } else {
        Some(namespace.to_owned())
    }
}

fn cron_job_status_summary(schedule: &str, active: usize, concurrency: &str) -> String {
    format!("schedule={schedule}, active={active}, concurrency={concurrency}")
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
    fn default_cron_job_yaml_is_applyable() {
        let parsed = parse_resource_apply_yaml(
            &default_cron_job_yaml(Some("production")),
            cron_job_metadata().namespaced,
            Some("default"),
        )
        .unwrap();

        assert_eq!(parsed.namespace.as_deref(), Some("production"));
        assert_eq!(parsed.name, "example-cron-job");
        assert_eq!(parsed.manifest["apiVersion"], "batch/v1");
        assert_eq!(parsed.manifest["kind"], "CronJob");
        assert_eq!(parsed.manifest["spec"]["schedule"], "*/5 * * * *");
        assert_eq!(
            parsed.manifest["spec"]["jobTemplate"]["spec"]["template"]["spec"]["restartPolicy"],
            "OnFailure"
        );
        assert_eq!(
            parsed.manifest["spec"]["jobTemplate"]["spec"]["template"]["spec"]["containers"][0]["image"],
            "busybox:latest"
        );
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

    #[test]
    fn cron_job_delete_target_uses_namespace_and_name() {
        let row = CronJobRow::from_summary(&cron_job_summary());
        let target = row.delete_target();

        assert_eq!(target.namespace.as_deref(), Some("default"));
        assert_eq!(target.name, "backup");
    }

    #[test]
    fn edit_action_opens_edit_dialog_with_editable_yaml() {
        let mut panel = CronJobResourcePanel::default();
        let row = CronJobRow::from_summary(&cron_job_summary());
        let key = row.key.clone();
        panel.rows = vec![row];

        panel.apply_table_action(Some(CronJobTableAction::Edit { key }));

        let dialog = panel.edit_dialog.as_ref().unwrap();
        assert_eq!(dialog.target.namespace.as_deref(), Some("default"));
        assert_eq!(dialog.target.name, "backup");
        let manifest = serde_yaml::from_str::<serde_json::Value>(&dialog.yaml).unwrap();
        assert!(manifest.pointer("/metadata/creationTimestamp").is_none());
        assert!(manifest.pointer("/status").is_none());
    }

    #[test]
    fn delete_action_opens_delete_dialog() {
        let mut panel = CronJobResourcePanel::default();
        let row = CronJobRow::from_summary(&cron_job_summary());
        let key = row.key.clone();
        panel.rows = vec![row];

        panel.apply_table_action(Some(CronJobTableAction::Delete { key }));

        let dialog = panel.delete_dialog.as_ref().unwrap();
        assert_eq!(dialog.target.namespace.as_deref(), Some("default"));
        assert_eq!(dialog.target.name, "backup");
        assert_eq!(panel.action_error, None);
    }

    #[test]
    fn apply_completion_closes_edit_dialog_and_updates_existing_row() {
        let mut panel = CronJobResourcePanel::default();
        let row = CronJobRow::from_summary(&cron_job_summary());
        panel.rows = vec![row.clone()];
        panel.edit_dialog = Some(GenericEditDialog {
            target: row.target(),
            yaml: "kind: CronJob".to_owned(),
            parse_error: None,
        });
        panel.action_request_id = Some(7);

        panel.apply_event(ResourceUiEvent::ResourceActionCompleted {
            request: super::super::ResourceActionRequest {
                request_id: 7,
                cluster_id: ClusterId::new("local"),
                kind: ResourceActionKind::ApplyResource {
                    resource: cron_job_metadata().resource,
                    namespace: Some("default".to_owned()),
                    name: "backup".to_owned(),
                    manifest: serde_json::json!({}),
                },
            },
            result: Ok(ResourceActionOutcome::Applied(cron_job_summary_with_name(
                "default", "backup",
            ))),
        });

        assert!(panel.edit_dialog.is_none());
        assert_eq!(panel.action_error, None);
        assert_eq!(panel.rows.len(), 1);
        assert_eq!(panel.rows[0].name, "backup");
    }

    #[test]
    fn delete_completion_closes_delete_dialog_and_removes_row() {
        let mut panel = CronJobResourcePanel::default();
        let row = CronJobRow::from_summary(&cron_job_summary());
        let key = row.key.clone();
        panel.rows = vec![row.clone()];
        panel.selected_rows.insert(key.clone());
        panel.delete_dialog = Some(GenericDeleteDialog {
            target: row.delete_target(),
        });
        panel.action_request_id = Some(7);

        panel.apply_event(ResourceUiEvent::ResourceActionCompleted {
            request: super::super::ResourceActionRequest {
                request_id: 7,
                cluster_id: ClusterId::new("local"),
                kind: ResourceActionKind::DeleteResource {
                    resource: cron_job_metadata().resource,
                    namespace: Some("default".to_owned()),
                    name: "backup".to_owned(),
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
    fn apply_error_keeps_edit_dialog() {
        let mut panel = CronJobResourcePanel::default();
        let row = CronJobRow::from_summary(&cron_job_summary());
        panel.edit_dialog = Some(GenericEditDialog {
            target: row.target(),
            yaml: "kind: CronJob".to_owned(),
            parse_error: None,
        });
        panel.action_request_id = Some(7);

        panel.apply_event(ResourceUiEvent::ResourceActionCompleted {
            request: super::super::ResourceActionRequest {
                request_id: 7,
                cluster_id: ClusterId::new("local"),
                kind: ResourceActionKind::ApplyResource {
                    resource: cron_job_metadata().resource,
                    namespace: Some("default".to_owned()),
                    name: "backup".to_owned(),
                    manifest: serde_json::json!({}),
                },
            },
            result: Err("field is immutable".to_owned()),
        });

        assert!(panel.edit_dialog.is_some());
        assert_eq!(panel.action_error.as_deref(), Some("field is immutable"));
    }

    #[test]
    fn delete_error_keeps_delete_dialog() {
        let mut panel = CronJobResourcePanel::default();
        let row = CronJobRow::from_summary(&cron_job_summary());
        panel.delete_dialog = Some(GenericDeleteDialog {
            target: row.delete_target(),
        });
        panel.action_request_id = Some(7);

        panel.apply_event(ResourceUiEvent::ResourceActionCompleted {
            request: super::super::ResourceActionRequest {
                request_id: 7,
                cluster_id: ClusterId::new("local"),
                kind: ResourceActionKind::DeleteResource {
                    resource: cron_job_metadata().resource,
                    namespace: Some("default".to_owned()),
                    name: "backup".to_owned(),
                },
            },
            result: Err("delete denied".to_owned()),
        });

        assert!(panel.delete_dialog.is_some());
        assert_eq!(panel.action_error.as_deref(), Some("delete denied"));
    }

    #[test]
    fn cluster_change_clears_edit_dialog() {
        let mut panel = CronJobResourcePanel::default();
        let row = CronJobRow::from_summary(&cron_job_summary());
        panel.last_cluster_id = Some(ClusterId::new("old"));
        panel.edit_dialog = Some(GenericEditDialog {
            target: row.target(),
            yaml: "kind: CronJob".to_owned(),
            parse_error: None,
        });
        panel.delete_dialog = Some(GenericDeleteDialog {
            target: row.delete_target(),
        });
        panel.action_request_id = Some(7);

        panel.reset_for_cluster_change(&ClusterId::new("new"));

        assert!(panel.edit_dialog.is_none());
        assert!(panel.delete_dialog.is_none());
        assert_eq!(panel.action_request_id, None);
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
