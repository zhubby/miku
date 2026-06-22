use std::collections::BTreeSet;

use eframe::egui;
use egui_extras::{Column, TableBuilder};
use miku_api::ResourceSummary;
use miku_core::{ClusterId, ResourceRef};

#[cfg(test)]
use super::ResourceLoadRequest;
use super::components::{
    ContainerTemplateDescribe, DescribeCondition, DescribeField, GenericBatchDeleteDialog,
    GenericCreateDialog, GenericDeleteDialog, GenericEditDialog, ResourceBatchDeleteDialogInput,
    ResourceCreateDialogInput, ResourceCreateDialogResponse, ResourceDeleteDialogInput,
    ResourceDeleteDialogResponse, ResourceEditDialogInput, ResourceEditDialogResponse,
    ResourceMapEntry, ResourceMapView, ResourceMetadata, ResourceRowTarget, ResourceToolbar,
    ResourceYamlViewDialog, SELECT_COLUMN_WIDTH, apply_resource_request,
    batch_delete_resource_request, condition_describes, container_template_describes,
    default_resource_yaml, delete_resource_request, describe_conditions,
    describe_container_templates, describe_fields, describe_group, describe_raw_manifest,
    edit_resource_request, editable_resource_yaml, patch_resource_request, selected_delete_targets,
    show_resource_batch_delete_dialog, show_resource_create_dialog, show_resource_delete_dialog,
    show_resource_describe_window, show_resource_edit_dialog, show_row_selection_checkbox,
    visible_keys,
};
use super::{
    LoadStatus, ResourceActionKind, ResourceActionOutcome, ResourceLoadKind, ResourcePanelRequests,
    ResourceUiEvent, ResourceWatchRequest, namespaces_from_list,
};
use crate::time::{human_age_from_rfc3339, utc_now_rfc3339_seconds};

#[derive(Clone, Debug, Default)]
pub(crate) struct DeploymentResourcePanel {
    namespace_filter: Option<String>,
    search_text: String,
    namespaces: Vec<String>,
    namespace_status: LoadStatus,
    row_status: LoadStatus,
    rows: Vec<DeploymentRow>,
    selected_rows: BTreeSet<String>,
    next_request_id: u64,
    namespace_request_id: Option<u64>,
    row_request_id: Option<u64>,
    namespace_watch_request_id: Option<u64>,
    row_watch_request_id: Option<u64>,
    last_cluster_id: Option<ClusterId>,
    describe_dialog: Option<DeploymentDescribeDialog>,
    view_dialog: Option<DeploymentViewDialog>,
    edit_dialog: Option<GenericEditDialog>,
    create_dialog: Option<GenericCreateDialog>,
    batch_delete_dialog: Option<GenericBatchDeleteDialog>,
    delete_dialog: Option<GenericDeleteDialog>,
    scale_dialog: Option<DeploymentScaleDialog>,
    action_request_id: Option<u64>,
    action_error: Option<String>,
}

impl DeploymentResourcePanel {
    pub(crate) fn show(
        &mut self,
        ui: &mut egui::Ui,
        cluster_id: Option<&ClusterId>,
    ) -> ResourcePanelRequests {
        let mut requests = ResourcePanelRequests::default();
        let Some(cluster_id) = cluster_id else {
            ui.centered_and_justified(|ui| {
                ui.label("Select a cluster to load deployments.");
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
                .push(self.request_deployment_watch(cluster_id.clone()));
        }

        self.show_toolbar(ui, cluster_id, &mut requests);
        ui.separator();
        self.show_body(ui, cluster_id, &mut requests.actions);
        self.show_describe_dialog(ui.ctx());
        self.show_view_dialog(ui.ctx());
        self.show_edit_dialog(ui.ctx(), cluster_id, &mut requests.actions);
        self.show_create_dialog(ui.ctx(), cluster_id, &mut requests.actions);
        self.show_batch_delete_dialog(ui.ctx(), cluster_id, &mut requests.actions);
        self.show_delete_dialog(ui.ctx(), cluster_id, &mut requests.actions);
        self.show_scale_dialog(ui.ctx(), cluster_id, &mut requests.actions);

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
                ResourceLoadKind::Deployments { .. } => {
                    if self.row_request_id != Some(request.request_id) {
                        return;
                    }
                    self.row_request_id = None;
                    match result {
                        Ok(list) => {
                            self.replace_rows(deployment_rows_from_list(&list.items));
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
                ResourceLoadKind::Deployments { .. } => {
                    if self.row_watch_request_id != Some(request.request_id) {
                        return;
                    }
                    match result {
                        Ok(miku_api::ResourceEvent::Snapshot(list)) => {
                            self.replace_rows(deployment_rows_from_list(&list.items));
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
                    Ok(ResourceActionOutcome::Applied(summary)) => {
                        self.upsert_row(DeploymentRow::from_summary(&summary));
                        self.create_dialog = None;
                        self.edit_dialog = None;
                        self.action_error = None;
                    }
                    Ok(ResourceActionOutcome::Patched(summary)) => {
                        self.upsert_row(DeploymentRow::from_summary(&summary));
                        self.scale_dialog = None;
                        self.action_error = None;
                    }
                    Ok(ResourceActionOutcome::Deleted) => {
                        if let ResourceActionKind::DeleteResource {
                            resource,
                            namespace,
                            name,
                        } = request.kind
                            && resource == deployment_metadata().resource
                        {
                            let key = deployment_key(namespace.as_deref().unwrap_or(""), &name);
                            self.rows.retain(|row| row.key != key);
                            self.selected_rows.remove(&key);
                        }
                        self.delete_dialog = None;
                        self.action_error = None;
                    }
                    Ok(ResourceActionOutcome::BatchDeleted(targets)) => {
                        for target in targets {
                            let key = deployment_key(
                                target.namespace.as_deref().unwrap_or(""),
                                &target.name,
                            );
                            self.rows.retain(|row| row.key != key);
                            self.selected_rows.remove(&key);
                        }
                        self.batch_delete_dialog = None;
                        self.action_error = None;
                    }
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
        self.scale_dialog = None;
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
            id_salt: "deployment_resource_toolbar",
            namespaces: &self.namespaces,
            namespace_filter: &mut self.namespace_filter,
            search_text: &mut self.search_text,
            search_hint: "Search Deployments...",
            item_count,
            selected_count: self.selected_rows.len(),
            loading: matches!(self.row_status, LoadStatus::Loading),
        }
        .show(ui);

        if response.namespace_changed {
            requests
                .watches
                .push(self.request_deployment_watch(cluster_id.clone()));
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
                .push(self.request_deployment_watch(cluster_id.clone()));
        }
        if response.create_clicked {
            self.create_dialog = Some(GenericCreateDialog {
                yaml: default_resource_yaml(
                    deployment_metadata(),
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

    fn show_body(
        &mut self,
        ui: &mut egui::Ui,
        cluster_id: &ClusterId,
        requests: &mut Vec<super::ResourceActionRequest>,
    ) {
        match &self.row_status {
            LoadStatus::Idle | LoadStatus::Loading if self.rows.is_empty() => {
                ui.centered_and_justified(|ui| {
                    ui.label("Loading deployments...");
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
                        ui.label("No deployments match the current filters.");
                    });
                    return;
                }

                let action =
                    show_deployment_table(ui, &self.rows, row_indices, &mut self.selected_rows);
                self.apply_table_action(action, cluster_id, requests);
            }
        }
    }

    fn apply_table_action(
        &mut self,
        action: Option<DeploymentTableAction>,
        cluster_id: &ClusterId,
        requests: &mut Vec<super::ResourceActionRequest>,
    ) {
        match action {
            Some(DeploymentTableAction::Describe { key }) => {
                let Some((name, describe)) = self
                    .row_by_key(&key)
                    .map(|row| (row.name.clone(), deployment_describe_from_row(row)))
                else {
                    return;
                };
                self.describe_dialog = Some(DeploymentDescribeDialog {
                    key,
                    name,
                    describe,
                });
            }
            Some(DeploymentTableAction::View { key }) => {
                let Some((name, yaml)) = self
                    .row_by_key(&key)
                    .map(|row| (row.name.clone(), full_manifest_yaml(&row.raw)))
                else {
                    return;
                };
                self.view_dialog = Some(DeploymentViewDialog { key, name, yaml });
            }
            Some(DeploymentTableAction::Edit { key }) => {
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
            Some(DeploymentTableAction::Delete { key }) => {
                let Some(row) = self.row_by_key(&key) else {
                    return;
                };
                self.delete_dialog = Some(GenericDeleteDialog {
                    target: row.delete_target(),
                });
                self.action_error = None;
            }
            Some(DeploymentTableAction::Scale { key }) => {
                let Some(row) = self.row_by_key(&key) else {
                    return;
                };
                self.scale_dialog = Some(DeploymentScaleDialog {
                    key,
                    name: row.name.clone(),
                    target: row.delete_target(),
                    replicas: desired_replicas(&row.raw)
                        .unwrap_or(0)
                        .min(MAX_WORKLOAD_REPLICAS),
                });
                self.action_error = None;
            }
            Some(DeploymentTableAction::Restart { key }) => {
                let Some(target) = self.row_by_key(&key).map(DeploymentRow::delete_target) else {
                    return;
                };
                let request = patch_resource_request(
                    self.allocate_request_id(),
                    cluster_id.clone(),
                    deployment_metadata(),
                    target,
                    restart_patch(),
                );
                self.action_request_id = Some(request.request_id);
                self.action_error = None;
                requests.push(request);
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
            egui::Id::new(("deployment-describe-dialog", &dialog.key)),
            format!("Describe {}", dialog.name),
            &mut open,
            |ui| {
                show_deployment_describe(ui, &dialog.describe);
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
            id: egui::Id::new(("deployment-view-dialog", &dialog.key)),
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
                metadata: deployment_metadata(),
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
                    deployment_metadata(),
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
                metadata: deployment_metadata(),
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
                    deployment_metadata(),
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
                metadata: deployment_metadata(),
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
                    deployment_metadata(),
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
                metadata: deployment_metadata(),
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
                    deployment_metadata(),
                    dialog.target,
                );
                self.action_request_id = Some(request.request_id);
                requests.push(request);
            }
        }
    }

    fn show_scale_dialog(
        &mut self,
        ctx: &egui::Context,
        cluster_id: &ClusterId,
        requests: &mut Vec<super::ResourceActionRequest>,
    ) {
        let Some(dialog) = self.scale_dialog.as_mut() else {
            return;
        };

        let mut cancel_clicked = false;
        let mut save_clicked = false;
        egui::Window::new(format!("Scale {}", dialog.name))
            .id(egui::Id::new(("deployment-scale-dialog", &dialog.key)))
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                if let Some(error) = self.action_error.as_deref() {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                    ui.separator();
                }
                ui.add(
                    egui::Slider::new(&mut dialog.replicas, 0..=MAX_WORKLOAD_REPLICAS)
                        .text("Replicas"),
                );
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        cancel_clicked = true;
                    }
                    if ui
                        .add_enabled(
                            self.action_request_id.is_none(),
                            egui::Button::new(format!(
                                "{} Save",
                                egui_phosphor::regular::FLOPPY_DISK
                            )),
                        )
                        .clicked()
                    {
                        save_clicked = true;
                    }
                });
            });

        if cancel_clicked {
            self.scale_dialog = None;
            self.action_error = None;
            return;
        }
        if save_clicked {
            let Some(dialog) = self.scale_dialog.clone() else {
                return;
            };
            let request = patch_resource_request(
                self.allocate_request_id(),
                cluster_id.clone(),
                deployment_metadata(),
                dialog.target,
                scale_patch(dialog.replicas),
            );
            self.action_request_id = Some(request.request_id);
            self.scale_dialog = None;
            self.action_error = None;
            requests.push(request);
        }
    }

    #[cfg(test)]
    fn request_deployments(&mut self, cluster_id: ClusterId) -> ResourceLoadRequest {
        let request = ResourceLoadRequest {
            request_id: self.allocate_request_id(),
            cluster_id,
            kind: ResourceLoadKind::Deployments {
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

    fn request_deployment_watch(&mut self, cluster_id: ClusterId) -> ResourceWatchRequest {
        let request = ResourceWatchRequest {
            request_id: self.allocate_request_id(),
            cluster_id,
            kind: ResourceLoadKind::Deployments {
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

    fn row_by_key(&self, key: &str) -> Option<&DeploymentRow> {
        self.rows.iter().find(|row| row.key == key)
    }

    fn replace_rows(&mut self, rows: Vec<DeploymentRow>) {
        let targets = rows.iter().map(DeploymentRow::target).collect::<Vec<_>>();
        let visible_keys = visible_keys(&targets);
        self.selected_rows.retain(|key| visible_keys.contains(key));
        self.rows = rows;
    }

    fn upsert_row(&mut self, row: DeploymentRow) {
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
            .map(DeploymentRow::target)
            .collect::<Vec<_>>();
        let visible_keys = visible_keys(&targets);
        self.selected_rows.retain(|key| visible_keys.contains(key));
    }

    fn selected_delete_targets(&self) -> Vec<super::ResourceDeleteTarget> {
        let targets = self
            .rows
            .iter()
            .map(DeploymentRow::target)
            .collect::<Vec<_>>();
        selected_delete_targets(&targets, &self.selected_rows)
    }
}

fn show_deployment_table(
    ui: &mut egui::Ui,
    rows: &[DeploymentRow],
    row_indices: Vec<usize>,
    selected_rows: &mut BTreeSet<String>,
) -> Option<DeploymentTableAction> {
    let row_height = ui.spacing().interact_size.y;
    let table_width: f32 = SELECT_COLUMN_WIDTH
        + DEPLOYMENT_COLUMN_WIDTHS.iter().sum::<f32>()
        + ui.spacing().item_spacing.x * DEPLOYMENT_COLUMNS.len() as f32;
    let mut action = None;

    egui::ScrollArea::horizontal()
        .id_salt("deployment_resource_table_horizontal")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.set_min_width(table_width);

            let mut table = TableBuilder::new(ui)
                .id_salt("deployment_resource_table")
                .striped(true)
                .resizable(false)
                .sense(egui::Sense::click())
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .min_scrolled_height(0.0);

            table = table.column(Column::exact(SELECT_COLUMN_WIDTH));
            for width in DEPLOYMENT_COLUMN_WIDTHS {
                table = table.column(Column::exact(width));
            }

            table
                .header(row_height, |mut header| {
                    header.col(|_| {});
                    for label in DEPLOYMENT_COLUMNS {
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
                            ui.label(&row.ready);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.up_to_date);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.available);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.replicas);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.strategy);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.selector);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.images);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.conditions);
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
                                action = Some(DeploymentTableAction::Describe {
                                    key: row.key.clone(),
                                });
                                ui.close();
                            }
                            if ui
                                .button(format!("{} View", egui_phosphor::regular::EYE))
                                .clicked()
                            {
                                action = Some(DeploymentTableAction::View {
                                    key: row.key.clone(),
                                });
                                ui.close();
                            }
                            if ui
                                .button(format!("{} Edit", egui_phosphor::regular::PENCIL_SIMPLE))
                                .clicked()
                            {
                                action = Some(DeploymentTableAction::Edit {
                                    key: row.key.clone(),
                                });
                                ui.close();
                            }
                            if ui
                                .button(format!("{} Scale", egui_phosphor::regular::ARROWS_OUT))
                                .clicked()
                            {
                                action = Some(DeploymentTableAction::Scale {
                                    key: row.key.clone(),
                                });
                                ui.close();
                            }
                            if ui
                                .button(format!(
                                    "{} Restart",
                                    egui_phosphor::regular::ARROWS_CLOCKWISE
                                ))
                                .clicked()
                            {
                                action = Some(DeploymentTableAction::Restart {
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
                                action = Some(DeploymentTableAction::Delete {
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

const DEPLOYMENT_COLUMNS: [&str; 11] = [
    "Name",
    "Namespace",
    "Ready",
    "Up-to-date",
    "Available",
    "Replicas",
    "Strategy",
    "Selector",
    "Images",
    "Conditions",
    "Age",
];
const DEPLOYMENT_COLUMN_WIDTHS: [f32; 11] = [
    240.0, 160.0, 100.0, 100.0, 100.0, 100.0, 120.0, 260.0, 320.0, 280.0, 90.0,
];
const MAX_WORKLOAD_REPLICAS: u32 = 100;
const RESTARTED_AT_ANNOTATION: &str = "kubectl.kubernetes.io/restartedAt";

#[cfg(test)]
fn filter_deployment_rows<'a>(
    rows: &'a [DeploymentRow],
    search_text: &str,
) -> Vec<&'a DeploymentRow> {
    rows.iter()
        .filter(|row| row_matches_search(row, search_text))
        .collect()
}

fn row_matches_search(row: &DeploymentRow, search_text: &str) -> bool {
    let needle = search_text.trim().to_lowercase();
    needle.is_empty()
        || row.name.to_lowercase().contains(&needle)
        || row.namespace.to_lowercase().contains(&needle)
        || row.selector.to_lowercase().contains(&needle)
        || row.images.to_lowercase().contains(&needle)
        || row.conditions.to_lowercase().contains(&needle)
}

fn deployment_metadata() -> ResourceMetadata {
    ResourceMetadata {
        id: "deployment".to_owned(),
        title: "Deployments".to_owned(),
        api_version: "apps/v1".to_owned(),
        kind: "Deployment".to_owned(),
        resource: ResourceRef::grouped("apps", "v1", "deployments"),
        namespaced: true,
    }
}

fn deployment_rows_from_list(items: &[ResourceSummary]) -> Vec<DeploymentRow> {
    let mut rows = items
        .iter()
        .map(DeploymentRow::from_summary)
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        left.namespace
            .cmp(&right.namespace)
            .then(left.name.cmp(&right.name))
    });
    rows
}

#[derive(Clone, Debug, PartialEq)]
struct DeploymentRow {
    key: String,
    name: String,
    namespace: String,
    ready: String,
    up_to_date: String,
    available: String,
    replicas: String,
    strategy: String,
    selector: String,
    images: String,
    conditions: String,
    age: String,
    raw: serde_json::Value,
}

impl DeploymentRow {
    fn from_summary(summary: &ResourceSummary) -> Self {
        let raw = &summary.raw;
        let name = value_str(raw, &["metadata", "name"]).unwrap_or(&summary.name);
        let namespace = value_str(raw, &["metadata", "namespace"])
            .or(summary.namespace.as_deref())
            .unwrap_or("N/A");
        let desired = value_u64(raw, &["spec", "replicas"]);
        let ready = value_u64(raw, &["status", "readyReplicas"]).unwrap_or(0);
        let updated = value_u64(raw, &["status", "updatedReplicas"]).unwrap_or(0);
        let available = value_u64(raw, &["status", "availableReplicas"]).unwrap_or(0);
        let current = value_u64(raw, &["status", "replicas"]).unwrap_or(0);

        Self {
            key: deployment_key(namespace, name),
            name: name.to_owned(),
            namespace: namespace.to_owned(),
            ready: replica_ratio(ready, desired),
            up_to_date: replica_ratio(updated, desired),
            available: replica_ratio(available, desired),
            replicas: replica_ratio(current, desired),
            strategy: value_str(raw, &["spec", "strategy", "type"])
                .unwrap_or("N/A")
                .to_owned(),
            selector: selector_label(raw),
            images: container_images(raw),
            conditions: condition_summary(raw),
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
enum DeploymentTableAction {
    Describe { key: String },
    View { key: String },
    Edit { key: String },
    Delete { key: String },
    Scale { key: String },
    Restart { key: String },
}

#[derive(Clone, Debug, PartialEq)]
struct DeploymentDescribeDialog {
    key: String,
    name: String,
    describe: DeploymentDescribe,
}

#[derive(Clone, Debug, PartialEq)]
struct DeploymentViewDialog {
    key: String,
    name: String,
    yaml: String,
}

#[derive(Clone, Debug, PartialEq)]
struct DeploymentScaleDialog {
    key: String,
    name: String,
    target: super::ResourceDeleteTarget,
    replicas: u32,
}

#[derive(Clone, Debug, PartialEq)]
struct DeploymentDescribe {
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

fn show_deployment_describe(ui: &mut egui::Ui, describe: &DeploymentDescribe) {
    describe_group(ui, egui_phosphor::regular::STACK, "Deployment", |ui| {
        describe_fields(ui, &describe.summary);
    });

    ui.add_space(10.0);
    describe_group(ui, egui_phosphor::regular::GAUGE, "Replicas", |ui| {
        describe_fields(ui, &describe.replicas);
    });

    ui.add_space(10.0);
    describe_group(
        ui,
        egui_phosphor::regular::ARROWS_CLOCKWISE,
        "Rollout",
        |ui| {
            describe_fields(ui, &describe.rollout);
        },
    );

    ui.add_space(10.0);
    describe_group(ui, egui_phosphor::regular::FUNNEL, "Selector", |ui| {
        ResourceMapView {
            id_salt: "deployment-describe-selector",
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
            id_salt: "deployment-describe-template-labels",
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
            describe_conditions(ui, "deployment-describe-conditions", &describe.conditions);
        },
    );

    ui.add_space(10.0);
    describe_group(ui, egui_phosphor::regular::TAG, "Metadata", |ui| {
        ResourceMapView {
            id_salt: "deployment-describe-labels",
            icon: egui_phosphor::regular::TAG,
            title: "Labels",
            entries: &describe.labels,
            empty_label: "No labels.",
        }
        .show(ui);
        ui.add_space(8.0);
        ResourceMapView {
            id_salt: "deployment-describe-annotations",
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
            "deployment-describe-raw-manifest-content",
            &describe.raw_yaml,
        );
    });
}

fn deployment_describe_from_row(row: &DeploymentRow) -> DeploymentDescribe {
    let raw = &row.raw;
    DeploymentDescribe {
        summary: vec![
            DescribeField::new("Name", row.name.clone()),
            DescribeField::new("Namespace", row.namespace.clone()),
            DescribeField::new("Age", row.age.clone()),
            DescribeField::new("Strategy", row.strategy.clone()),
        ],
        replicas: vec![
            DescribeField::new("Ready", row.ready.clone()),
            DescribeField::new("Up-to-date", row.up_to_date.clone()),
            DescribeField::new("Available", row.available.clone()),
            DescribeField::new("Replicas", row.replicas.clone()),
        ],
        rollout: vec![
            DescribeField::new(
                "Max surge",
                value_str(raw, &["spec", "strategy", "rollingUpdate", "maxSurge"]).unwrap_or("N/A"),
            ),
            DescribeField::new(
                "Max unavailable",
                value_str(
                    raw,
                    &["spec", "strategy", "rollingUpdate", "maxUnavailable"],
                )
                .unwrap_or("N/A"),
            ),
            DescribeField::new(
                "Min ready seconds",
                value_u64(raw, &["spec", "minReadySeconds"])
                    .map_or_else(|| "N/A".to_owned(), |value| value.to_string()),
            ),
            DescribeField::new(
                "Revision history",
                value_u64(raw, &["spec", "revisionHistoryLimit"])
                    .map_or_else(|| "N/A".to_owned(), |value| value.to_string()),
            ),
        ],
        selector: string_map_entries(raw.pointer("/spec/selector/matchLabels")),
        template_labels: string_map_entries(raw.pointer("/spec/template/metadata/labels")),
        containers: container_template_describes(raw, "/spec/template/spec/containers"),
        conditions: condition_describes(raw.pointer("/status/conditions")),
        labels: string_map_entries(raw.pointer("/metadata/labels")),
        annotations: string_map_entries(raw.pointer("/metadata/annotations")),
        raw_yaml: full_manifest_yaml(raw),
    }
}

fn deployment_key(namespace: &str, name: &str) -> String {
    format!("{namespace}/{name}")
}

fn namespace_value(namespace: &str) -> Option<String> {
    if namespace.is_empty() || namespace == "N/A" {
        None
    } else {
        Some(namespace.to_owned())
    }
}

fn replica_ratio(current: u64, desired: Option<u64>) -> String {
    match desired {
        Some(desired) => format!("{current}/{desired}"),
        None => format!("{current}/N/A"),
    }
}

fn desired_replicas(raw: &serde_json::Value) -> Option<u32> {
    value_u64(raw, &["spec", "replicas"]).and_then(|value| u32::try_from(value).ok())
}

fn scale_patch(replicas: u32) -> serde_json::Value {
    serde_json::json!({
        "spec": {
            "replicas": replicas,
        },
    })
}

fn restart_patch() -> serde_json::Value {
    restart_patch_with_timestamp(&restart_timestamp())
}

fn restart_patch_with_timestamp(timestamp: &str) -> serde_json::Value {
    serde_json::json!({
        "spec": {
            "template": {
                "metadata": {
                    "annotations": {
                        RESTARTED_AT_ANNOTATION: timestamp,
                    },
                },
            },
        },
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn restart_timestamp() -> String {
    utc_now_rfc3339_seconds()
}

#[cfg(target_arch = "wasm32")]
fn restart_timestamp() -> String {
    utc_now_rfc3339_seconds()
}

fn selector_label(raw: &serde_json::Value) -> String {
    let labels = string_map_lines(raw.pointer("/spec/selector/matchLabels"));
    if labels.is_empty() {
        "N/A".to_owned()
    } else {
        labels.join(", ")
    }
}

fn container_images(raw: &serde_json::Value) -> String {
    let images = raw
        .pointer("/spec/template/spec/containers")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|container| value_str(container, &["image"]))
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    if images.is_empty() {
        "N/A".to_owned()
    } else {
        images.join(", ")
    }
}

fn condition_summary(raw: &serde_json::Value) -> String {
    let conditions = condition_describes(raw.pointer("/status/conditions"))
        .into_iter()
        .map(|condition| format!("{}={}", condition.condition_type, condition.status))
        .collect::<Vec<_>>();
    if conditions.is_empty() {
        "N/A".to_owned()
    } else {
        conditions.join(", ")
    }
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

fn string_map_lines(value: Option<&serde_json::Value>) -> Vec<String> {
    string_map_entries(value)
        .into_iter()
        .map(|entry| format!("{}={}", entry.key, entry.value))
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

fn value_u64(value: &serde_json::Value, path: &[&str]) -> Option<u64> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    current.as_u64()
}

#[cfg(test)]
mod tests {
    use super::*;
    use miku_api::ResourceList;

    #[test]
    fn deployment_request_query_uses_selected_namespace() {
        let mut panel = DeploymentResourcePanel {
            namespace_filter: Some("production".to_owned()),
            ..DeploymentResourcePanel::default()
        };

        let request = panel.request_deployments(ClusterId::new("local"));
        let query = request.query();

        assert_eq!(query.resource.plural, "deployments");
        assert_eq!(query.resource.group.as_deref(), Some("apps"));
        assert_eq!(query.namespace.as_deref(), Some("production"));
    }

    #[test]
    fn deployment_row_extracts_table_fields_from_raw_summary() {
        let row = DeploymentRow::from_summary(&deployment_summary());

        assert_eq!(row.name, "api");
        assert_eq!(row.namespace, "default");
        assert_eq!(row.ready, "2/3");
        assert_eq!(row.up_to_date, "3/3");
        assert_eq!(row.available, "2/3");
        assert_eq!(row.replicas, "3/3");
        assert_eq!(row.strategy, "RollingUpdate");
        assert_eq!(row.selector, "app=api, tier=backend");
        assert_eq!(row.images, "ghcr.io/example/api:1.0.0, envoyproxy/envoy:v1");
        assert_eq!(row.conditions, "Available=True, Progressing=True");
        assert!(row.age.ends_with(" ago"));
    }

    #[test]
    fn deployment_row_handles_missing_optional_fields() {
        let row = DeploymentRow::from_summary(&ResourceSummary {
            name: "minimal".to_owned(),
            namespace: Some("default".to_owned()),
            kind: "Deployment".to_owned(),
            status: None,
            raw: serde_json::json!({"metadata": {"name": "minimal", "namespace": "default"}}),
        });

        assert_eq!(row.ready, "0/N/A");
        assert_eq!(row.replicas, "0/N/A");
        assert_eq!(row.strategy, "N/A");
        assert_eq!(row.selector, "N/A");
        assert_eq!(row.images, "N/A");
        assert_eq!(row.conditions, "N/A");
    }

    #[test]
    fn deployment_rows_filter_by_multiple_fields_case_insensitively() {
        let rows = vec![
            DeploymentRow::from_summary(&deployment_summary()),
            DeploymentRow::from_summary(&ResourceSummary {
                name: "worker".to_owned(),
                namespace: Some("production".to_owned()),
                kind: "Deployment".to_owned(),
                status: None,
                raw: serde_json::json!({
                    "metadata": {"name": "worker", "namespace": "production"},
                    "spec": {
                        "selector": {"matchLabels": {"app": "worker"}},
                        "template": {"spec": {"containers": [{"name": "worker", "image": "worker:1"}]}}
                    }
                }),
            }),
        ];

        assert_eq!(filter_deployment_rows(&rows, "BACKEND").len(), 1);
        assert_eq!(filter_deployment_rows(&rows, "PRODUCTION").len(), 1);
        assert_eq!(filter_deployment_rows(&rows, "envoy").len(), 1);
        assert_eq!(filter_deployment_rows(&rows, "Progressing").len(), 1);
    }

    #[test]
    fn deployment_rows_are_sorted_by_namespace_and_name() {
        let rows = deployment_rows_from_list(&[
            deployment_summary_with_name("zeta", "worker"),
            deployment_summary_with_name("default", "api"),
            deployment_summary_with_name("default", "scheduler"),
        ]);

        let keys = rows.into_iter().map(|row| row.key).collect::<Vec<_>>();
        assert_eq!(
            keys,
            vec!["default/api", "default/scheduler", "zeta/worker"]
        );
    }

    #[test]
    fn deployment_describe_extracts_details() {
        let row = DeploymentRow::from_summary(&deployment_summary());
        let describe = deployment_describe_from_row(&row);

        assert_eq!(describe.selector.len(), 2);
        assert_eq!(describe.template_labels.len(), 2);
        assert_eq!(describe.containers.len(), 2);
        assert_eq!(describe.containers[0].name, "api");
        assert_eq!(describe.containers[0].image, "ghcr.io/example/api:1.0.0");
        assert_eq!(describe.conditions.len(), 2);
        assert!(describe.labels.iter().any(|entry| entry.key == "app"));
        assert!(
            describe
                .annotations
                .iter()
                .any(|entry| entry.key == "deployment.kubernetes.io/revision")
        );
    }

    #[test]
    fn stale_resource_events_do_not_replace_current_rows() {
        let mut panel = DeploymentResourcePanel::default();
        let cluster_id = ClusterId::new("local");
        let first = panel.request_deployments(cluster_id.clone());
        let second = panel.request_deployments(cluster_id);

        panel.apply_event(ResourceUiEvent::ResourcesLoaded {
            request: first,
            result: Ok(ResourceList {
                items: vec![deployment_summary_with_name("default", "stale")],
                continue_token: None,
            }),
        });
        assert!(panel.rows.is_empty());

        panel.apply_event(ResourceUiEvent::ResourcesLoaded {
            request: second,
            result: Ok(ResourceList {
                items: vec![deployment_summary()],
                continue_token: None,
            }),
        });

        assert_eq!(panel.rows.len(), 1);
        assert_eq!(panel.rows[0].name, "api");
    }

    #[test]
    fn stale_watch_events_do_not_replace_current_rows() {
        let mut panel = DeploymentResourcePanel::default();
        let cluster_id = ClusterId::new("local");
        let first = panel.request_deployment_watch(cluster_id.clone());
        let second = panel.request_deployment_watch(cluster_id);

        panel.apply_event(ResourceUiEvent::ResourceWatchUpdated {
            request: first,
            result: Ok(miku_api::ResourceEvent::Snapshot(ResourceList {
                items: vec![deployment_summary_with_name("default", "stale")],
                continue_token: None,
            })),
        });
        assert!(panel.rows.is_empty());

        panel.apply_event(ResourceUiEvent::ResourceWatchUpdated {
            request: second,
            result: Ok(miku_api::ResourceEvent::Snapshot(ResourceList {
                items: vec![deployment_summary()],
                continue_token: None,
            })),
        });

        assert_eq!(panel.rows.len(), 1);
        assert_eq!(panel.rows[0].name, "api");
    }

    #[test]
    fn namespace_watch_events_from_shared_request_update_selector() {
        let mut panel = DeploymentResourcePanel::default();
        let request = ResourceWatchRequest {
            request_id: 42,
            cluster_id: ClusterId::new("local"),
            kind: ResourceLoadKind::Namespaces,
        };

        panel.apply_event(ResourceUiEvent::ResourceWatchUpdated {
            request,
            result: Ok(miku_api::ResourceEvent::Snapshot(ResourceList {
                items: vec![namespace_summary("production")],
                continue_token: None,
            })),
        });

        assert_eq!(panel.namespaces, vec!["production".to_owned()]);
        assert_eq!(panel.namespace_status, LoadStatus::Loaded);
    }

    #[test]
    fn scale_patch_sets_spec_replicas() {
        assert_eq!(scale_patch(7), serde_json::json!({"spec": {"replicas": 7}}));
    }

    #[test]
    fn restart_patch_sets_restarted_at_annotation() {
        let patch = restart_patch_with_timestamp("12345");

        assert_eq!(
            patch["spec"]["template"]["metadata"]["annotations"][RESTARTED_AT_ANNOTATION],
            "12345"
        );
    }

    #[test]
    fn deployment_delete_target_uses_namespace_and_name() {
        let row = DeploymentRow::from_summary(&deployment_summary());
        let target = row.delete_target();

        assert_eq!(target.namespace.as_deref(), Some("default"));
        assert_eq!(target.name, "api");
    }

    #[test]
    fn patch_completion_updates_existing_row() {
        let mut panel = DeploymentResourcePanel::default();
        let cluster_id = ClusterId::new("local");
        panel.rows = vec![DeploymentRow::from_summary(&deployment_summary())];
        panel.action_request_id = Some(7);

        panel.apply_event(ResourceUiEvent::ResourceActionCompleted {
            request: super::super::ResourceActionRequest {
                request_id: 7,
                cluster_id,
                kind: ResourceActionKind::PatchResource {
                    resource: deployment_metadata().resource,
                    namespace: Some("default".to_owned()),
                    name: "api".to_owned(),
                    patch: scale_patch(5),
                },
            },
            result: Ok(ResourceActionOutcome::Patched(
                deployment_summary_with_replicas(5),
            )),
        });

        assert_eq!(panel.rows.len(), 1);
        assert_eq!(panel.rows[0].replicas, "3/5");
    }

    #[test]
    fn edit_action_opens_edit_dialog_with_editable_yaml() {
        let mut panel = DeploymentResourcePanel::default();
        let row = DeploymentRow::from_summary(&deployment_summary());
        let key = row.key.clone();
        panel.rows = vec![row];
        let mut actions = Vec::new();

        panel.apply_table_action(
            Some(DeploymentTableAction::Edit { key }),
            &ClusterId::new("local"),
            &mut actions,
        );

        assert!(actions.is_empty());
        let dialog = panel.edit_dialog.as_ref().unwrap();
        assert_eq!(dialog.target.namespace.as_deref(), Some("default"));
        assert_eq!(dialog.target.name, "api");
        let manifest = serde_yaml::from_str::<serde_json::Value>(&dialog.yaml).unwrap();
        assert!(manifest.pointer("/metadata/creationTimestamp").is_none());
        assert!(manifest.pointer("/status").is_none());
    }

    #[test]
    fn apply_completion_closes_edit_dialog_and_updates_existing_row() {
        let mut panel = DeploymentResourcePanel::default();
        let row = DeploymentRow::from_summary(&deployment_summary());
        panel.rows = vec![row.clone()];
        panel.edit_dialog = Some(GenericEditDialog {
            target: row.target(),
            yaml: "kind: Deployment".to_owned(),
            parse_error: None,
        });
        panel.action_request_id = Some(7);

        panel.apply_event(ResourceUiEvent::ResourceActionCompleted {
            request: super::super::ResourceActionRequest {
                request_id: 7,
                cluster_id: ClusterId::new("local"),
                kind: ResourceActionKind::ApplyResource {
                    resource: deployment_metadata().resource,
                    namespace: Some("default".to_owned()),
                    name: "api".to_owned(),
                    manifest: serde_json::json!({}),
                },
            },
            result: Ok(ResourceActionOutcome::Applied(
                deployment_summary_with_replicas(5),
            )),
        });

        assert!(panel.edit_dialog.is_none());
        assert_eq!(panel.action_error, None);
        assert_eq!(panel.rows.len(), 1);
        assert_eq!(panel.rows[0].replicas, "3/5");
    }

    #[test]
    fn apply_error_keeps_edit_dialog() {
        let mut panel = DeploymentResourcePanel::default();
        let row = DeploymentRow::from_summary(&deployment_summary());
        panel.edit_dialog = Some(GenericEditDialog {
            target: row.target(),
            yaml: "kind: Deployment".to_owned(),
            parse_error: None,
        });
        panel.action_request_id = Some(7);

        panel.apply_event(ResourceUiEvent::ResourceActionCompleted {
            request: super::super::ResourceActionRequest {
                request_id: 7,
                cluster_id: ClusterId::new("local"),
                kind: ResourceActionKind::ApplyResource {
                    resource: deployment_metadata().resource,
                    namespace: Some("default".to_owned()),
                    name: "api".to_owned(),
                    manifest: serde_json::json!({}),
                },
            },
            result: Err("field is immutable".to_owned()),
        });

        assert!(panel.edit_dialog.is_some());
        assert_eq!(panel.action_error.as_deref(), Some("field is immutable"));
    }

    #[test]
    fn cluster_change_clears_edit_dialog() {
        let mut panel = DeploymentResourcePanel::default();
        let row = DeploymentRow::from_summary(&deployment_summary());
        panel.last_cluster_id = Some(ClusterId::new("old"));
        panel.edit_dialog = Some(GenericEditDialog {
            target: row.target(),
            yaml: "kind: Deployment".to_owned(),
            parse_error: None,
        });
        panel.action_request_id = Some(7);

        panel.reset_for_cluster_change(&ClusterId::new("new"));

        assert!(panel.edit_dialog.is_none());
        assert_eq!(panel.action_request_id, None);
    }

    fn deployment_summary() -> ResourceSummary {
        deployment_summary_with_name("default", "api")
    }

    fn deployment_summary_with_name(namespace: &str, name: &str) -> ResourceSummary {
        deployment_summary_with_replicas_for_name(namespace, name, 3)
    }

    fn deployment_summary_with_replicas(replicas: u64) -> ResourceSummary {
        deployment_summary_with_replicas_for_name("default", "api", replicas)
    }

    fn deployment_summary_with_replicas_for_name(
        namespace: &str,
        name: &str,
        replicas: u64,
    ) -> ResourceSummary {
        ResourceSummary {
            name: name.to_owned(),
            namespace: Some(namespace.to_owned()),
            kind: "Deployment".to_owned(),
            status: None,
            raw: serde_json::json!({
                "metadata": {
                    "name": name,
                    "namespace": namespace,
                    "creationTimestamp": "2026-05-18T10:00:00Z",
                    "labels": {"app": name},
                    "annotations": {"deployment.kubernetes.io/revision": "3"}
                },
                "spec": {
                    "replicas": replicas,
                    "minReadySeconds": 5,
                    "revisionHistoryLimit": 10,
                    "strategy": {
                        "type": "RollingUpdate",
                        "rollingUpdate": {
                            "maxSurge": "25%",
                            "maxUnavailable": "25%"
                        }
                    },
                    "selector": {
                        "matchLabels": {
                            "app": name,
                            "tier": "backend"
                        }
                    },
                    "template": {
                        "metadata": {
                            "labels": {
                                "app": name,
                                "tier": "backend"
                            }
                        },
                        "spec": {
                            "containers": [
                                {"name": name, "image": "ghcr.io/example/api:1.0.0"},
                                {"name": "sidecar", "image": "envoyproxy/envoy:v1"}
                            ]
                        }
                    }
                },
                "status": {
                    "replicas": 3,
                    "readyReplicas": 2,
                    "updatedReplicas": 3,
                    "availableReplicas": 2,
                    "conditions": [
                        {
                            "type": "Available",
                            "status": "True",
                            "reason": "MinimumReplicasAvailable",
                            "message": "Deployment has minimum availability."
                        },
                        {
                            "type": "Progressing",
                            "status": "True",
                            "reason": "NewReplicaSetAvailable",
                            "message": "ReplicaSet has successfully progressed."
                        }
                    ]
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
