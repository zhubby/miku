use std::collections::BTreeSet;

use eframe::egui;
use egui_extras::{Column, TableBuilder};
use miku_api::{LogLine, PodAttachInput, PodAttachOutput, ResourceSummary};
use miku_core::ClusterId;

#[cfg(test)]
use super::ResourceLoadRequest;
use super::components::{ResourceToolbar, ResourceYamlEditDialog, ResourceYamlViewDialog};
use super::{
    LoadStatus, PodAttachInputRequest, PodAttachRequest, PodLogRequest, ResourceActionKind,
    ResourceActionOutcome, ResourceActionRequest, ResourceDeleteTarget, ResourceLoadKind,
    ResourcePanelRequests, ResourceUiEvent, ResourceWatchRequest, namespaces_from_list,
};
use crate::time::human_age_from_rfc3339;

#[derive(Clone, Debug, Default)]
pub(crate) struct PodResourcePanel {
    namespace_filter: Option<String>,
    search_text: String,
    namespaces: Vec<String>,
    namespace_status: LoadStatus,
    row_status: LoadStatus,
    rows: Vec<PodRow>,
    selected_rows: BTreeSet<String>,
    next_request_id: u64,
    namespace_request_id: Option<u64>,
    row_request_id: Option<u64>,
    namespace_watch_request_id: Option<u64>,
    row_watch_request_id: Option<u64>,
    action_request_id: Option<u64>,
    log_request_id: Option<u64>,
    last_cluster_id: Option<ClusterId>,
    create_dialog: Option<PodCreateDialog>,
    describe_dialog: Option<PodDescribeDialog>,
    view_dialog: Option<PodViewDialog>,
    edit_dialog: Option<PodEditDialog>,
    delete_dialog: Option<PodDeleteDialog>,
    batch_delete_dialog: Option<PodBatchDeleteDialog>,
    evict_dialog: Option<PodEvictDialog>,
    log_dialog: Option<PodLogDialog>,
    attach_dialog: Option<PodAttachDialog>,
    action_error: Option<String>,
}

impl PodResourcePanel {
    pub(crate) fn show(
        &mut self,
        ui: &mut egui::Ui,
        cluster_id: Option<&ClusterId>,
    ) -> ResourcePanelRequests {
        let mut requests = ResourcePanelRequests::default();
        let Some(cluster_id) = cluster_id else {
            ui.centered_and_justified(|ui| {
                ui.label("Select a cluster to load pods.");
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
                .push(self.request_pod_watch(cluster_id.clone()));
        }
        self.show_toolbar(ui, cluster_id, &mut requests);
        ui.separator();
        self.show_body(ui);
        self.show_create_dialog(ui.ctx(), cluster_id, &mut requests.actions);
        self.show_describe_dialog(ui.ctx());
        self.show_view_dialog(ui.ctx());
        self.show_edit_dialog(ui.ctx(), cluster_id, &mut requests.actions);
        self.show_delete_dialog(ui.ctx(), cluster_id, &mut requests.actions);
        self.show_batch_delete_dialog(ui.ctx(), cluster_id, &mut requests.actions);
        self.show_evict_dialog(ui.ctx(), cluster_id, &mut requests.actions);
        self.show_log_dialog(ui.ctx(), cluster_id, &mut requests.logs);
        self.show_attach_dialog(
            ui.ctx(),
            cluster_id,
            &mut requests.attaches,
            &mut requests.attach_inputs,
        );

        requests
    }

    pub(crate) fn apply_event(&mut self, event: ResourceUiEvent) {
        match event {
            ResourceUiEvent::ResourcesLoaded { request, result } => match request.kind {
                ResourceLoadKind::Namespaces => {
                    if self.namespace_request_id != Some(request.request_id) {
                        return;
                    }
                    self.namespace_request_id = None;
                    match result {
                        Ok(list) => {
                            self.namespaces = namespaces_from_list(&list);
                            self.namespace_status = LoadStatus::Loaded;
                        }
                        Err(error) => self.namespace_status = LoadStatus::Error(error),
                    }
                }
                ResourceLoadKind::Pods { .. } => {
                    if self.row_request_id != Some(request.request_id) {
                        return;
                    }
                    self.row_request_id = None;
                    match result {
                        Ok(list) => {
                            self.rows = pod_rows_from_list(&list.items);
                            self.row_status = LoadStatus::Loaded;
                        }
                        Err(error) => self.row_status = LoadStatus::Error(error),
                    }
                }
            },
            ResourceUiEvent::ResourceActionCompleted { request, result } => {
                if self.action_request_id != Some(request.request_id) {
                    return;
                }
                self.action_request_id = None;
                match result {
                    Ok(ResourceActionOutcome::Applied(_)) => {
                        self.create_dialog = None;
                        self.edit_dialog = None;
                        self.action_error = None;
                    }
                    Ok(ResourceActionOutcome::Deleted) => {
                        if let ResourceActionKind::DeletePod { namespace, name } = request.kind {
                            let key = pod_key(namespace.as_deref().unwrap_or(""), &name);
                            self.rows.retain(|row| row.key != key);
                            self.selected_rows.remove(&key);
                        }
                        self.delete_dialog = None;
                        self.action_error = None;
                    }
                    Ok(ResourceActionOutcome::BatchDeleted(targets)) => {
                        for target in targets {
                            let key =
                                pod_key(target.namespace.as_deref().unwrap_or(""), &target.name);
                            self.rows.retain(|row| row.key != key);
                            self.selected_rows.remove(&key);
                        }
                        self.batch_delete_dialog = None;
                        self.action_error = None;
                    }
                    Ok(ResourceActionOutcome::Evicted) => {
                        self.evict_dialog = None;
                        self.action_error = None;
                    }
                    Err(error) => self.action_error = Some(error),
                }
            }
            ResourceUiEvent::PodLogsLoaded { request, result } => {
                if self.log_request_id != Some(request.request_id) {
                    return;
                }
                self.log_request_id = None;
                if let Some(dialog) = self.log_dialog.as_mut() {
                    match result {
                        Ok(lines) => {
                            dialog.lines = lines;
                            dialog.status = LoadStatus::Loaded;
                            dialog.error = None;
                        }
                        Err(error) => {
                            dialog.status = LoadStatus::Error(error.clone());
                            dialog.error = Some(error);
                        }
                    }
                }
            }
            ResourceUiEvent::PodAttachConnected { request, result } => {
                let Some(dialog) = self.attach_dialog.as_mut() else {
                    return;
                };
                if dialog.request_id != Some(request.request_id) {
                    return;
                }
                match result {
                    Ok(_) => {
                        dialog.status = PodAttachStatus::Attached;
                        dialog.error = None;
                        dialog.output.push_str("attached\n");
                    }
                    Err(error) => {
                        dialog.status = PodAttachStatus::Error(error.clone());
                        dialog.error = Some(error);
                        dialog.request_id = None;
                    }
                }
            }
            ResourceUiEvent::PodAttachOutput { request, result } => {
                let Some(dialog) = self.attach_dialog.as_mut() else {
                    return;
                };
                if dialog.request_id != Some(request.request_id) {
                    return;
                }
                match result {
                    Ok(PodAttachOutput::Stdout(bytes)) | Ok(PodAttachOutput::Stderr(bytes)) => {
                        dialog.output.push_str(&String::from_utf8_lossy(&bytes));
                    }
                    Ok(PodAttachOutput::Closed) => {
                        dialog.output.push_str("\ndisconnected\n");
                        dialog.status = PodAttachStatus::Disconnected;
                        dialog.request_id = None;
                    }
                    Err(error) => {
                        dialog.status = PodAttachStatus::Error(error.clone());
                        dialog.error = Some(error);
                        dialog.request_id = None;
                    }
                }
            }
            ResourceUiEvent::ResourceWatchUpdated { request, result } => match request.kind {
                ResourceLoadKind::Namespaces => {
                    if self.namespace_watch_request_id != Some(request.request_id) {
                        return;
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
                ResourceLoadKind::Pods { .. } => {
                    if self.row_watch_request_id != Some(request.request_id) {
                        return;
                    }
                    match result {
                        Ok(miku_api::ResourceEvent::Snapshot(list)) => {
                            self.replace_rows(pod_rows_from_list(&list.items));
                            self.row_status = LoadStatus::Loaded;
                        }
                        Ok(_) => {}
                        Err(error) => self.row_status = LoadStatus::Error(error),
                    }
                }
            },
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
        self.action_request_id = None;
        self.log_request_id = None;
        self.create_dialog = None;
        self.describe_dialog = None;
        self.view_dialog = None;
        self.edit_dialog = None;
        self.delete_dialog = None;
        self.batch_delete_dialog = None;
        self.evict_dialog = None;
        self.log_dialog = None;
        self.attach_dialog = None;
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
            id_salt: "pod_resource_toolbar",
            namespaces: &self.namespaces,
            namespace_filter: &mut self.namespace_filter,
            search_text: &mut self.search_text,
            search_hint: "Search Pods...",
            item_count,
            selected_count: self.selected_rows.len(),
            loading: matches!(self.row_status, LoadStatus::Loading),
        }
        .show(ui);

        if response.namespace_changed {
            requests
                .watches
                .push(self.request_pod_watch(cluster_id.clone()));
        }

        if response.search_changed {
            let visible_keys = self
                .filtered_row_indices()
                .into_iter()
                .map(|index| self.rows[index].key.clone())
                .collect::<BTreeSet<_>>();
            self.selected_rows.retain(|key| visible_keys.contains(key));
        }

        if response.refresh_clicked {
            requests
                .watches
                .push(self.request_namespace_watch(cluster_id.clone()));
            requests
                .watches
                .push(self.request_pod_watch(cluster_id.clone()));
        }

        if response.create_clicked {
            self.create_dialog = Some(PodCreateDialog {
                yaml: default_pod_yaml(self.namespace_filter.as_deref()),
                parse_error: None,
            });
            self.action_error = None;
        }

        if response.batch_delete_clicked {
            let targets = self.selected_delete_targets();
            if !targets.is_empty() {
                self.batch_delete_dialog = Some(PodBatchDeleteDialog { targets });
                self.action_error = None;
            }
        }
    }

    fn show_body(&mut self, ui: &mut egui::Ui) {
        match &self.row_status {
            LoadStatus::Idle | LoadStatus::Loading if self.rows.is_empty() => {
                ui.centered_and_justified(|ui| {
                    ui.label("Loading pods...");
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
                        ui.label("No pods match the current filters.");
                    });
                    return;
                }

                let action = show_pod_table(ui, &self.rows, row_indices, &mut self.selected_rows);
                self.apply_table_action(action);
            }
        }
    }

    fn apply_table_action(&mut self, action: Option<PodTableAction>) {
        match action {
            Some(PodTableAction::Logs { key }) => {
                let Some((namespace, name, containers)) = self.row_by_key(&key).map(|row| {
                    (
                        empty_to_none(row.namespace.clone())
                            .unwrap_or_else(|| "default".to_owned()),
                        row.name.clone(),
                        row.container_names.clone(),
                    )
                }) else {
                    return;
                };
                let selected_container = containers.first().cloned();
                self.log_dialog = Some(PodLogDialog {
                    key,
                    namespace,
                    pod: name,
                    containers,
                    selected_container,
                    lines: Vec::new(),
                    status: LoadStatus::Idle,
                    error: None,
                });
            }
            Some(PodTableAction::Attach { key }) => {
                let Some((namespace, name, containers)) = self.row_by_key(&key).map(|row| {
                    (
                        empty_to_none(row.namespace.clone())
                            .unwrap_or_else(|| "default".to_owned()),
                        row.name.clone(),
                        row.container_names.clone(),
                    )
                }) else {
                    return;
                };
                let selected_container = containers.first().cloned();
                self.attach_dialog = Some(PodAttachDialog {
                    key,
                    namespace,
                    pod: name,
                    containers,
                    selected_container,
                    input: String::new(),
                    output: String::new(),
                    request_id: None,
                    status: PodAttachStatus::Disconnected,
                    error: None,
                });
            }
            Some(PodTableAction::Evict { key }) => {
                let Some((namespace, name)) = self.row_by_key(&key).map(|row| {
                    (
                        empty_to_none(row.namespace.clone())
                            .unwrap_or_else(|| "default".to_owned()),
                        row.name.clone(),
                    )
                }) else {
                    return;
                };
                self.evict_dialog = Some(PodEvictDialog {
                    key,
                    namespace,
                    name,
                });
                self.action_error = None;
            }
            Some(PodTableAction::Describe { key }) => {
                let Some((name, describe)) = self
                    .row_by_key(&key)
                    .map(|row| (row.name.clone(), pod_describe_from_row(row)))
                else {
                    return;
                };
                self.describe_dialog = Some(PodDescribeDialog {
                    key,
                    name,
                    describe,
                });
            }
            Some(PodTableAction::View { key }) => {
                let Some((name, yaml)) = self
                    .row_by_key(&key)
                    .map(|row| (row.name.clone(), full_manifest_yaml(&row.raw)))
                else {
                    return;
                };
                self.view_dialog = Some(PodViewDialog { key, name, yaml });
            }
            Some(PodTableAction::Edit { key }) => {
                let Some((namespace, name, yaml)) = self.row_by_key(&key).map(|row| {
                    (
                        empty_to_none(row.namespace.clone()),
                        row.name.clone(),
                        editable_manifest_yaml(&row.raw),
                    )
                }) else {
                    return;
                };
                self.edit_dialog = Some(PodEditDialog {
                    key,
                    namespace,
                    name,
                    yaml,
                    parse_error: None,
                });
                self.action_error = None;
            }
            Some(PodTableAction::Delete { key }) => {
                let Some((namespace, name)) = self
                    .row_by_key(&key)
                    .map(|row| (empty_to_none(row.namespace.clone()), row.name.clone()))
                else {
                    return;
                };
                self.delete_dialog = Some(PodDeleteDialog {
                    key,
                    namespace,
                    name,
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
        egui::Window::new(format!("Describe {}", dialog.name))
            .id(egui::Id::new(("pod-describe-dialog", &dialog.key)))
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .open(&mut open)
            .collapsible(false)
            .fixed_size([POD_DESCRIBE_DIALOG_WIDTH, POD_DESCRIBE_DIALOG_HEIGHT])
            .show(ctx, |ui| {
                ui.set_width(POD_DESCRIBE_DIALOG_WIDTH);
                ui.set_height(POD_DESCRIBE_CONTENT_HEIGHT);
                egui::ScrollArea::vertical()
                    .id_salt(("pod-describe-content", &dialog.key))
                    .max_width(POD_DESCRIBE_DIALOG_WIDTH)
                    .max_height(POD_DESCRIBE_CONTENT_HEIGHT)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_width(POD_DESCRIBE_DIALOG_WIDTH - POD_DESCRIBE_CONTENT_INSET);
                        show_pod_describe(ui, &dialog.describe);
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
            id: egui::Id::new(("pod-view-dialog", &dialog.key)),
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
        requests: &mut Vec<ResourceActionRequest>,
    ) {
        let Some(dialog) = self.create_dialog.as_mut() else {
            return;
        };

        let response = ResourceYamlEditDialog {
            id: egui::Id::new("pod-create-dialog"),
            title: "Create Pod".to_owned(),
            yaml: &mut dialog.yaml,
            error: self
                .action_error
                .as_deref()
                .or(dialog.parse_error.as_deref()),
            save_enabled: self.action_request_id.is_none(),
            save_label: "Confirm",
        }
        .show(ctx);

        if response.cancel_clicked {
            self.create_dialog = None;
            self.action_error = None;
            return;
        }

        if !response.save_clicked {
            return;
        }

        match pod_apply_parts_from_yaml(&dialog.yaml) {
            Ok((namespace, name, manifest)) => {
                dialog.parse_error = None;
                let request = ResourceActionRequest {
                    request_id: self.allocate_request_id(),
                    cluster_id: cluster_id.clone(),
                    kind: ResourceActionKind::ApplyPod {
                        namespace,
                        name,
                        manifest,
                    },
                };
                self.action_request_id = Some(request.request_id);
                requests.push(request);
            }
            Err(error) => dialog.parse_error = Some(error),
        }
    }

    fn show_edit_dialog(
        &mut self,
        ctx: &egui::Context,
        cluster_id: &ClusterId,
        requests: &mut Vec<ResourceActionRequest>,
    ) {
        let Some(dialog) = self.edit_dialog.as_mut() else {
            return;
        };

        let response = ResourceYamlEditDialog {
            id: egui::Id::new(("pod-edit-dialog", &dialog.key)),
            title: format!("Edit {}", dialog.name),
            yaml: &mut dialog.yaml,
            error: self
                .action_error
                .as_deref()
                .or(dialog.parse_error.as_deref()),
            save_enabled: self.action_request_id.is_none(),
            save_label: "Save",
        }
        .show(ctx);

        if response.cancel_clicked {
            self.edit_dialog = None;
            self.action_error = None;
            return;
        }

        if !response.save_clicked {
            return;
        }

        match serde_yaml::from_str::<serde_json::Value>(&dialog.yaml) {
            Ok(manifest) => {
                dialog.parse_error = None;
                let namespace = dialog.namespace.clone();
                let name = dialog.name.clone();
                let request = ResourceActionRequest {
                    request_id: self.allocate_request_id(),
                    cluster_id: cluster_id.clone(),
                    kind: ResourceActionKind::ApplyPod {
                        namespace,
                        name,
                        manifest,
                    },
                };
                self.action_request_id = Some(request.request_id);
                requests.push(request);
            }
            Err(error) => dialog.parse_error = Some(error.to_string()),
        }
    }

    fn show_delete_dialog(
        &mut self,
        ctx: &egui::Context,
        cluster_id: &ClusterId,
        requests: &mut Vec<ResourceActionRequest>,
    ) {
        let Some(dialog) = self.delete_dialog.clone() else {
            return;
        };

        let mut cancel_clicked = false;
        let mut delete_clicked = false;
        egui::Window::new(format!("Delete {}", dialog.name))
            .id(egui::Id::new(("pod-delete-dialog", &dialog.key)))
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                if let Some(error) = self.action_error.as_deref() {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                    ui.separator();
                }
                ui.label(format!("Delete Pod {}?", dialog.name));
                if let Some(namespace) = &dialog.namespace {
                    ui.label(format!("Namespace: {namespace}"));
                }
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        cancel_clicked = true;
                    }
                    let delete_text =
                        egui::RichText::new(format!("{} Delete", egui_phosphor::regular::TRASH))
                            .color(ui.visuals().error_fg_color);
                    if ui
                        .add_enabled(
                            self.action_request_id.is_none(),
                            egui::Button::new(delete_text),
                        )
                        .clicked()
                    {
                        delete_clicked = true;
                    }
                });
            });

        if cancel_clicked {
            self.delete_dialog = None;
            self.action_error = None;
            return;
        }

        if delete_clicked {
            let request = ResourceActionRequest {
                request_id: self.allocate_request_id(),
                cluster_id: cluster_id.clone(),
                kind: ResourceActionKind::DeletePod {
                    namespace: dialog.namespace,
                    name: dialog.name,
                },
            };
            self.action_request_id = Some(request.request_id);
            requests.push(request);
        }
    }

    fn show_batch_delete_dialog(
        &mut self,
        ctx: &egui::Context,
        cluster_id: &ClusterId,
        requests: &mut Vec<ResourceActionRequest>,
    ) {
        let Some(dialog) = self.batch_delete_dialog.clone() else {
            return;
        };

        let mut cancel_clicked = false;
        let mut delete_clicked = false;
        egui::Window::new("Delete selected Pods")
            .id(egui::Id::new("pod-batch-delete-dialog"))
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                if let Some(error) = self.action_error.as_deref() {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                    ui.separator();
                }
                ui.label(format!("Delete {} selected Pods?", dialog.targets.len()));
                ui.separator();
                egui::ScrollArea::vertical()
                    .id_salt("pod-batch-delete-targets")
                    .max_height(160.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        for target in &dialog.targets {
                            let namespace = target.namespace.as_deref().unwrap_or("default");
                            ui.label(format!("{namespace}/{}", target.name));
                        }
                    });
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        cancel_clicked = true;
                    }
                    let delete_text =
                        egui::RichText::new(format!("{} Delete", egui_phosphor::regular::TRASH))
                            .color(ui.visuals().error_fg_color);
                    if ui
                        .add_enabled(
                            self.action_request_id.is_none(),
                            egui::Button::new(delete_text),
                        )
                        .clicked()
                    {
                        delete_clicked = true;
                    }
                });
            });

        if cancel_clicked {
            self.batch_delete_dialog = None;
            self.action_error = None;
            return;
        }

        if delete_clicked {
            let request = ResourceActionRequest {
                request_id: self.allocate_request_id(),
                cluster_id: cluster_id.clone(),
                kind: ResourceActionKind::BatchDeletePods {
                    targets: dialog.targets,
                },
            };
            self.action_request_id = Some(request.request_id);
            requests.push(request);
        }
    }

    fn show_evict_dialog(
        &mut self,
        ctx: &egui::Context,
        cluster_id: &ClusterId,
        requests: &mut Vec<ResourceActionRequest>,
    ) {
        let Some(dialog) = self.evict_dialog.clone() else {
            return;
        };

        let mut cancel_clicked = false;
        let mut evict_clicked = false;
        egui::Window::new(format!("Evict {}", dialog.name))
            .id(egui::Id::new(("pod-evict-dialog", &dialog.key)))
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                if let Some(error) = self.action_error.as_deref() {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                    ui.separator();
                }
                ui.label(format!("Evict Pod {}?", dialog.name));
                ui.label(format!("Namespace: {}", dialog.namespace));
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        cancel_clicked = true;
                    }
                    let evict_text =
                        egui::RichText::new(format!("{} Evict", egui_phosphor::regular::WARNING))
                            .color(evict_color());
                    if ui
                        .add_enabled(
                            self.action_request_id.is_none(),
                            egui::Button::new(evict_text),
                        )
                        .clicked()
                    {
                        evict_clicked = true;
                    }
                });
            });

        if cancel_clicked {
            self.evict_dialog = None;
            self.action_error = None;
            return;
        }

        if evict_clicked {
            let request = ResourceActionRequest {
                request_id: self.allocate_request_id(),
                cluster_id: cluster_id.clone(),
                kind: ResourceActionKind::EvictPod {
                    namespace: dialog.namespace,
                    name: dialog.name,
                },
            };
            self.action_request_id = Some(request.request_id);
            requests.push(request);
        }
    }

    fn show_log_dialog(
        &mut self,
        ctx: &egui::Context,
        cluster_id: &ClusterId,
        requests: &mut Vec<PodLogRequest>,
    ) {
        if self.log_dialog.is_some() && self.log_request_id.is_none() {
            let should_load = self
                .log_dialog
                .as_ref()
                .is_some_and(|dialog| matches!(dialog.status, LoadStatus::Idle));
            if should_load {
                let request = self.request_logs(cluster_id.clone());
                requests.push(request);
            }
        }

        let Some(dialog) = self.log_dialog.as_mut() else {
            return;
        };

        let mut open = true;
        let mut reload_clicked = false;
        let mut container_changed = false;
        let log_request_in_flight = self.log_request_id.is_some();
        egui::Window::new(format!("Logs {}", dialog.pod))
            .id(egui::Id::new(("pod-log-dialog", &dialog.key)))
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(780.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Container");
                    let selected_label = dialog
                        .selected_container
                        .as_deref()
                        .unwrap_or("default")
                        .to_owned();
                    egui::ComboBox::from_id_salt(("pod-log-container", &dialog.key))
                        .selected_text(selected_label)
                        .show_ui(ui, |ui| {
                            for container in &dialog.containers {
                                if ui
                                    .selectable_value(
                                        &mut dialog.selected_container,
                                        Some(container.clone()),
                                        container,
                                    )
                                    .changed()
                                {
                                    container_changed = true;
                                }
                            }
                        });
                    if ui
                        .add_enabled(!log_request_in_flight, egui::Button::new("Refresh"))
                        .clicked()
                    {
                        reload_clicked = true;
                    }
                });
                if let Some(error) = dialog.error.as_deref() {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }
                ui.separator();
                ui.allocate_ui(
                    [ui.available_width(), POD_LOG_CONTENT_HEIGHT].into(),
                    |ui| {
                        egui::ScrollArea::both()
                            .id_salt(("pod-log-lines", &dialog.key))
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                if matches!(dialog.status, LoadStatus::Loading)
                                    && dialog.lines.is_empty()
                                {
                                    ui.label("Loading logs...");
                                } else if dialog.lines.is_empty() {
                                    ui.label("No log lines.");
                                } else {
                                    let text = dialog
                                        .lines
                                        .iter()
                                        .map(|line| line.text.as_str())
                                        .collect::<Vec<_>>()
                                        .join("\n");
                                    ui.add(
                                        egui::Label::new(egui::RichText::new(text).monospace())
                                            .selectable(true),
                                    );
                                }
                            });
                    },
                );
            });

        if !open {
            self.log_dialog = None;
            self.log_request_id = None;
            return;
        }

        if (container_changed || reload_clicked) && self.log_request_id.is_none() {
            if let Some(dialog) = self.log_dialog.as_mut() {
                dialog.status = LoadStatus::Idle;
                dialog.lines.clear();
                dialog.error = None;
            }
            let request = self.request_logs(cluster_id.clone());
            requests.push(request);
        }
    }

    fn show_attach_dialog(
        &mut self,
        ctx: &egui::Context,
        cluster_id: &ClusterId,
        requests: &mut Vec<PodAttachRequest>,
        input_requests: &mut Vec<PodAttachInputRequest>,
    ) {
        let Some(dialog) = self.attach_dialog.as_mut() else {
            return;
        };

        let mut open = true;
        let mut attach_clicked = false;
        let mut disconnect_clicked = false;
        let mut clear_clicked = false;
        let mut send_clicked = false;
        let connected = matches!(dialog.status, PodAttachStatus::Attached);
        let connecting = matches!(dialog.status, PodAttachStatus::Connecting);

        egui::Window::new(format!("Attach {}", dialog.pod))
            .id(egui::Id::new(("pod-attach-dialog", &dialog.key)))
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .fixed_size([POD_ATTACH_DIALOG_WIDTH, POD_ATTACH_DIALOG_HEIGHT])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Container");
                    let selected_label = dialog
                        .selected_container
                        .as_deref()
                        .unwrap_or("default")
                        .to_owned();
                    ui.add_enabled_ui(!connected && !connecting, |ui| {
                        egui::ComboBox::from_id_salt(("pod-attach-container", &dialog.key))
                            .selected_text(selected_label)
                            .show_ui(ui, |ui| {
                                for container in &dialog.containers {
                                    ui.selectable_value(
                                        &mut dialog.selected_container,
                                        Some(container.clone()),
                                        container,
                                    );
                                }
                            });
                    });
                    ui.separator();
                    ui.label(pod_attach_status_label(&dialog.status));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        clear_clicked = ui
                            .button(egui_phosphor::regular::BROOM)
                            .on_hover_text("Clear")
                            .clicked();
                        disconnect_clicked = ui
                            .add_enabled(
                                connected || connecting,
                                egui::Button::new(egui_phosphor::regular::PLUGS_CONNECTED),
                            )
                            .on_hover_text("Disconnect")
                            .clicked();
                        attach_clicked = ui
                            .add_enabled(
                                !connected && !connecting,
                                egui::Button::new(egui_phosphor::regular::PLUG),
                            )
                            .on_hover_text("Attach")
                            .clicked();
                    });
                });
                if let Some(error) = dialog.error.as_deref() {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }
                ui.separator();
                ui.allocate_ui(
                    [POD_ATTACH_OUTPUT_WIDTH, POD_ATTACH_OUTPUT_HEIGHT].into(),
                    |ui| {
                        egui::ScrollArea::both()
                            .id_salt(("pod-attach-output", &dialog.key))
                            .auto_shrink([false, false])
                            .stick_to_bottom(true)
                            .show(ui, |ui| {
                                ui.set_min_width(POD_ATTACH_OUTPUT_WIDTH - 16.0);
                                let output = if dialog.output.is_empty() {
                                    "No output."
                                } else {
                                    &dialog.output
                                };
                                ui.add(
                                    egui::Label::new(egui::RichText::new(output).monospace())
                                        .selectable(true),
                                );
                            });
                    },
                );
                ui.separator();
                ui.horizontal(|ui| {
                    let response = ui.add_enabled(
                        connected,
                        egui::TextEdit::singleline(&mut dialog.input)
                            .desired_width(POD_ATTACH_INPUT_WIDTH)
                            .hint_text("stdin"),
                    );
                    send_clicked = ui
                        .add_enabled(
                            connected,
                            egui::Button::new(egui_phosphor::regular::PAPER_PLANE_TILT),
                        )
                        .on_hover_text("Send")
                        .clicked()
                        || (response.lost_focus()
                            && ui.input(|input| input.key_pressed(egui::Key::Enter)));
                });
            });

        if !open {
            if let Some(request_id) = dialog.request_id {
                input_requests.push(PodAttachInputRequest {
                    request_id,
                    input: PodAttachInput::Close,
                });
            }
            self.attach_dialog = None;
            return;
        }

        if clear_clicked {
            dialog.output.clear();
        }

        if disconnect_clicked {
            if let Some(request_id) = dialog.request_id.take() {
                input_requests.push(PodAttachInputRequest {
                    request_id,
                    input: PodAttachInput::Close,
                });
            }
            dialog.status = PodAttachStatus::Disconnected;
        }

        if attach_clicked {
            self.next_request_id += 1;
            let request_id = self.next_request_id;
            let request = PodAttachRequest {
                request_id,
                cluster_id: cluster_id.clone(),
                namespace: dialog.namespace.clone(),
                pod: dialog.pod.clone(),
                container: dialog.selected_container.clone(),
                tty: true,
            };
            dialog.request_id = Some(request.request_id);
            dialog.status = PodAttachStatus::Connecting;
            dialog.error = None;
            requests.push(request);
        }

        if send_clicked
            && let Some(request_id) = dialog.request_id
            && !dialog.input.is_empty()
        {
            let mut bytes = std::mem::take(&mut dialog.input).into_bytes();
            bytes.push(b'\n');
            input_requests.push(PodAttachInputRequest {
                request_id,
                input: PodAttachInput::Bytes(bytes),
            });
        }
    }

    #[cfg(test)]
    fn request_pods(&mut self, cluster_id: ClusterId) -> ResourceLoadRequest {
        let request = ResourceLoadRequest {
            request_id: self.allocate_request_id(),
            cluster_id,
            kind: ResourceLoadKind::Pods {
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

    fn request_pod_watch(&mut self, cluster_id: ClusterId) -> ResourceWatchRequest {
        let request = ResourceWatchRequest {
            request_id: self.allocate_request_id(),
            cluster_id,
            kind: ResourceLoadKind::Pods {
                namespace: self.namespace_filter.clone(),
            },
        };
        self.row_watch_request_id = Some(request.request_id);
        self.row_status = LoadStatus::Loading;
        request
    }

    fn request_logs(&mut self, cluster_id: ClusterId) -> PodLogRequest {
        let (namespace, pod, container) = self
            .log_dialog
            .as_ref()
            .map(|dialog| {
                (
                    dialog.namespace.clone(),
                    dialog.pod.clone(),
                    dialog.selected_container.clone(),
                )
            })
            .unwrap_or_else(|| ("default".to_owned(), String::new(), None));
        let request = PodLogRequest {
            request_id: self.allocate_request_id(),
            cluster_id,
            namespace,
            pod,
            container,
            tail_lines: Some(500),
        };
        if let Some(dialog) = self.log_dialog.as_mut() {
            dialog.status = LoadStatus::Loading;
            dialog.error = None;
        }
        self.log_request_id = Some(request.request_id);
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

    fn replace_rows(&mut self, rows: Vec<PodRow>) {
        let visible_keys = rows
            .iter()
            .map(|row| row.key.clone())
            .collect::<BTreeSet<_>>();
        self.selected_rows.retain(|key| visible_keys.contains(key));
        self.rows = rows;
    }

    fn row_by_key(&self, key: &str) -> Option<&PodRow> {
        self.rows.iter().find(|row| row.key == key)
    }

    fn selected_delete_targets(&self) -> Vec<ResourceDeleteTarget> {
        self.rows
            .iter()
            .filter(|row| self.selected_rows.contains(&row.key))
            .map(|row| ResourceDeleteTarget {
                namespace: empty_to_none(row.namespace.clone()),
                name: row.name.clone(),
            })
            .collect()
    }
}

fn show_pod_table(
    ui: &mut egui::Ui,
    rows: &[PodRow],
    row_indices: Vec<usize>,
    selected_rows: &mut BTreeSet<String>,
) -> Option<PodTableAction> {
    let row_height = ui.spacing().interact_size.y;
    let table_width: f32 = POD_COLUMN_WIDTHS.iter().sum::<f32>()
        + ui.spacing().item_spacing.x * POD_COLUMN_WIDTHS.len().saturating_sub(1) as f32;
    let mut action = None;

    egui::ScrollArea::horizontal()
        .id_salt("pod_resource_table_horizontal")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.set_min_width(table_width);

            let mut table = TableBuilder::new(ui)
                .id_salt("pod_resource_table")
                .striped(true)
                .resizable(false)
                .sense(egui::Sense::click())
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .min_scrolled_height(0.0);

            for width in POD_COLUMN_WIDTHS {
                table = table.column(Column::exact(width));
            }

            table
                .header(row_height, |mut header| {
                    for label in POD_COLUMNS {
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
                            let mut selected = row_selected;
                            if ui.checkbox(&mut selected, "").changed() {
                                checkbox_changed = true;
                                if selected {
                                    selected_rows.insert(row.key.clone());
                                } else {
                                    selected_rows.remove(&row.key);
                                }
                            }
                        });
                        table_row.col(|ui| {
                            ui.label(&row.name);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.namespace);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.cpu);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.memory);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.containers);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.restarts);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.controlled_by);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.node);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.qos);
                        });
                        table_row.col(|ui| {
                            ui.colored_label(status_color(ui, &row.status), &row.status);
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
                                .button(format!("{} Logs", egui_phosphor::regular::TERMINAL_WINDOW))
                                .clicked()
                            {
                                action = Some(PodTableAction::Logs {
                                    key: row.key.clone(),
                                });
                                ui.close();
                            }
                            if ui
                                .button(format!("{} Attach", egui_phosphor::regular::TERMINAL))
                                .clicked()
                            {
                                action = Some(PodTableAction::Attach {
                                    key: row.key.clone(),
                                });
                                ui.close();
                            }
                            if ui
                                .button(format!("{} Describe", egui_phosphor::regular::INFO))
                                .clicked()
                            {
                                action = Some(PodTableAction::Describe {
                                    key: row.key.clone(),
                                });
                                ui.close();
                            }
                            let evict_text = egui::RichText::new(format!(
                                "{} Evict",
                                egui_phosphor::regular::WARNING
                            ))
                            .color(evict_color());
                            if ui.button(evict_text).clicked() {
                                action = Some(PodTableAction::Evict {
                                    key: row.key.clone(),
                                });
                                ui.close();
                            }
                            ui.separator();
                            if ui
                                .button(format!("{} View", egui_phosphor::regular::EYE))
                                .clicked()
                            {
                                action = Some(PodTableAction::View {
                                    key: row.key.clone(),
                                });
                                ui.close();
                            }
                            if ui
                                .button(format!("{} Edit", egui_phosphor::regular::PENCIL_SIMPLE))
                                .clicked()
                            {
                                action = Some(PodTableAction::Edit {
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
                                action = Some(PodTableAction::Delete {
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

const POD_COLUMNS: [&str; 12] = [
    "",
    "Name",
    "Namespace",
    "CPU",
    "Memory",
    "Container",
    "Restarts",
    "Controlled By",
    "Node",
    "QoS",
    "Status",
    "Age",
];

const POD_COLUMN_WIDTHS: [f32; 12] = [
    32.0, 260.0, 180.0, 110.0, 130.0, 100.0, 100.0, 170.0, 180.0, 100.0, 120.0, 90.0,
];

const POD_LOG_CONTENT_HEIGHT: f32 = 360.0;
const POD_ATTACH_DIALOG_WIDTH: f32 = 820.0;
const POD_ATTACH_DIALOG_HEIGHT: f32 = 520.0;
const POD_ATTACH_OUTPUT_WIDTH: f32 = 780.0;
const POD_ATTACH_OUTPUT_HEIGHT: f32 = 360.0;
const POD_ATTACH_INPUT_WIDTH: f32 = 720.0;
const POD_DESCRIBE_DIALOG_WIDTH: f32 = 860.0;
const POD_DESCRIBE_DIALOG_HEIGHT: f32 = 580.0;
const POD_DESCRIBE_CONTENT_HEIGHT: f32 = 520.0;
const POD_DESCRIBE_CONTENT_INSET: f32 = 28.0;
const POD_DESCRIBE_FIELD_LABEL_WIDTH: f32 = 105.0;
const POD_DESCRIBE_FIELD_VALUE_WIDTH: f32 = 250.0;
const POD_DESCRIBE_LINE_WIDTH: f32 = 800.0;

fn status_color(ui: &egui::Ui, status: &str) -> egui::Color32 {
    match status {
        "Running" => egui::Color32::from_rgb(46, 160, 67),
        "Pending" => egui::Color32::from_rgb(191, 135, 0),
        "Succeeded" => ui.visuals().weak_text_color(),
        "Failed" | "CrashLoopBackOff" | "Error" => ui.visuals().error_fg_color,
        _ => ui.visuals().text_color(),
    }
}

fn evict_color() -> egui::Color32 {
    egui::Color32::from_rgb(217, 119, 6)
}

fn pod_attach_status_label(status: &PodAttachStatus) -> String {
    match status {
        PodAttachStatus::Disconnected => "Disconnected".to_owned(),
        PodAttachStatus::Connecting => "Connecting".to_owned(),
        PodAttachStatus::Attached => "Attached".to_owned(),
        PodAttachStatus::Error(error) => format!("Error: {error}"),
    }
}

#[cfg(test)]
fn filter_pod_rows<'a>(rows: &'a [PodRow], search_text: &str) -> Vec<&'a PodRow> {
    rows.iter()
        .filter(|row| row_matches_search(row, search_text))
        .collect()
}

fn row_matches_search(row: &PodRow, search_text: &str) -> bool {
    let needle = search_text.trim().to_lowercase();
    needle.is_empty() || row.name.to_lowercase().contains(&needle)
}

fn pod_rows_from_list(items: &[ResourceSummary]) -> Vec<PodRow> {
    let mut rows = items.iter().map(PodRow::from_summary).collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        left.namespace
            .cmp(&right.namespace)
            .then(left.name.cmp(&right.name))
    });
    rows
}

#[derive(Clone, Debug, PartialEq)]
struct PodRow {
    key: String,
    name: String,
    namespace: String,
    cpu: String,
    memory: String,
    containers: String,
    container_names: Vec<String>,
    restarts: String,
    controlled_by: String,
    node: String,
    qos: String,
    status: String,
    age: String,
    raw: serde_json::Value,
}

impl PodRow {
    fn from_summary(summary: &ResourceSummary) -> Self {
        let raw = &summary.raw;
        let name = value_str(raw, &["metadata", "name"]).unwrap_or(&summary.name);
        let namespace = value_str(raw, &["metadata", "namespace"])
            .or(summary.namespace.as_deref())
            .unwrap_or("N/A");
        let key = pod_key(namespace, name);
        let container_statuses = raw
            .pointer("/status/containerStatuses")
            .and_then(serde_json::Value::as_array);
        let total_containers = raw
            .pointer("/spec/containers")
            .and_then(serde_json::Value::as_array)
            .map_or(0, Vec::len);
        let container_names = container_names(raw);
        let ready_containers = container_statuses.map_or(0, |statuses| {
            statuses
                .iter()
                .filter(|status| value_bool(status, &["ready"]).unwrap_or(false))
                .count()
        });
        let restarts = container_statuses.map_or(0, |statuses| {
            statuses
                .iter()
                .filter_map(|status| value_u64(status, &["restartCount"]))
                .sum::<u64>()
        });
        let (cpu, memory) = resource_summaries(raw);

        Self {
            key,
            name: name.to_owned(),
            namespace: namespace.to_owned(),
            cpu,
            memory,
            containers: format!("{ready_containers}/{total_containers}"),
            container_names,
            restarts: restarts.to_string(),
            controlled_by: owner_reference(raw).unwrap_or_else(|| "N/A".to_owned()),
            node: value_str(raw, &["spec", "nodeName"])
                .unwrap_or("N/A")
                .to_owned(),
            qos: value_str(raw, &["status", "qosClass"])
                .unwrap_or("N/A")
                .to_owned(),
            status: pod_status(raw, summary.status.as_deref()),
            age: value_str(raw, &["metadata", "creationTimestamp"])
                .map(|timestamp| {
                    human_age_from_rfc3339(timestamp).unwrap_or_else(|| timestamp.to_owned())
                })
                .unwrap_or_else(|| "N/A".to_owned()),
            raw: summary.raw.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum PodTableAction {
    Logs { key: String },
    Attach { key: String },
    Evict { key: String },
    Describe { key: String },
    View { key: String },
    Edit { key: String },
    Delete { key: String },
}

#[derive(Clone, Debug, PartialEq)]
struct PodCreateDialog {
    yaml: String,
    parse_error: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
struct PodDescribeDialog {
    key: String,
    name: String,
    describe: PodDescribe,
}

#[derive(Clone, Debug, PartialEq)]
struct PodDescribe {
    summary: Vec<DescribeField>,
    metadata: Vec<DescribeField>,
    containers: Vec<ContainerDescribe>,
    conditions: Vec<ConditionDescribe>,
    volumes: Vec<String>,
    node_selectors: Vec<String>,
    tolerations: Vec<String>,
    raw_yaml: String,
}

#[derive(Clone, Debug, PartialEq)]
struct ContainerDescribe {
    name: String,
    image: String,
    ready: bool,
    restarts: String,
    state: String,
    state_detail: String,
    resources: Vec<DescribeField>,
    ports: String,
    env_count: String,
    volume_mounts: String,
    probes: Vec<DescribeField>,
}

#[derive(Clone, Debug, PartialEq)]
struct ConditionDescribe {
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

#[derive(Clone, Debug, PartialEq)]
struct PodViewDialog {
    key: String,
    name: String,
    yaml: String,
}

#[derive(Clone, Debug, PartialEq)]
struct PodEditDialog {
    key: String,
    namespace: Option<String>,
    name: String,
    yaml: String,
    parse_error: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
struct PodDeleteDialog {
    key: String,
    namespace: Option<String>,
    name: String,
}

#[derive(Clone, Debug, PartialEq)]
struct PodBatchDeleteDialog {
    targets: Vec<ResourceDeleteTarget>,
}

#[derive(Clone, Debug, PartialEq)]
struct PodEvictDialog {
    key: String,
    namespace: String,
    name: String,
}

#[derive(Clone, Debug, PartialEq)]
struct PodLogDialog {
    key: String,
    namespace: String,
    pod: String,
    containers: Vec<String>,
    selected_container: Option<String>,
    lines: Vec<LogLine>,
    status: LoadStatus,
    error: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
struct PodAttachDialog {
    key: String,
    namespace: String,
    pod: String,
    containers: Vec<String>,
    selected_container: Option<String>,
    input: String,
    output: String,
    request_id: Option<u64>,
    status: PodAttachStatus,
    error: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum PodAttachStatus {
    Disconnected,
    Connecting,
    Attached,
    Error(String),
}

fn show_pod_describe(ui: &mut egui::Ui, describe: &PodDescribe) {
    describe_section(ui, egui_phosphor::regular::CUBE, "Pod");
    describe_fields(ui, &describe.summary);

    ui.add_space(10.0);
    describe_section(ui, egui_phosphor::regular::BOX_ARROW_DOWN, "Containers");
    if describe.containers.is_empty() {
        ui.label("N/A");
    } else {
        for (index, container) in describe.containers.iter().enumerate() {
            if index > 0 {
                ui.separator();
            }
            show_container_describe(ui, container);
        }
    }

    ui.add_space(10.0);
    describe_section(ui, egui_phosphor::regular::CHECK_CIRCLE, "Conditions");
    if describe.conditions.is_empty() {
        ui.label("N/A");
    } else {
        egui::Grid::new("pod-describe-conditions")
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
                    wrapped_value(ui, &condition.condition_type, 140.0);
                    ui.colored_label(condition_color(ui, &condition.status), &condition.status);
                    wrapped_value(ui, &condition.reason, 170.0);
                    wrapped_value(ui, &condition.message, 330.0);
                    ui.end_row();
                }
            });
    }

    ui.add_space(10.0);
    describe_section(ui, egui_phosphor::regular::HARD_DRIVES, "Volumes");
    describe_lines(ui, &describe.volumes);

    ui.add_space(10.0);
    describe_section(ui, egui_phosphor::regular::TAG, "Metadata");
    describe_fields(ui, &describe.metadata);

    ui.add_space(10.0);
    describe_section(ui, egui_phosphor::regular::LIST_CHECKS, "Scheduling");
    ui.strong("Node selectors");
    describe_lines(ui, &describe.node_selectors);
    ui.add_space(4.0);
    ui.strong("Tolerations");
    describe_lines(ui, &describe.tolerations);

    ui.add_space(10.0);
    egui::CollapsingHeader::new(format!("{} Raw manifest", egui_phosphor::regular::CODE))
        .id_salt("pod-describe-raw-manifest")
        .show(ui, |ui| {
            egui::ScrollArea::both()
                .id_salt("pod-describe-raw-manifest-content")
                .max_height(180.0)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.add(
                        egui::Label::new(egui::RichText::new(&describe.raw_yaml).monospace())
                            .selectable(true),
                    );
                });
        });
}

fn show_container_describe(ui: &mut egui::Ui, container: &ContainerDescribe) {
    ui.horizontal(|ui| {
        ui.strong(format!(
            "{} {}",
            egui_phosphor::regular::PACKAGE,
            container.name
        ));
        wrapped_value(ui, &container.image, 660.0);
    });

    let ready_text = if container.ready {
        format!("{} Ready", egui_phosphor::regular::CHECK_CIRCLE)
    } else {
        format!("{} Not ready", egui_phosphor::regular::WARNING_CIRCLE)
    };
    ui.horizontal_wrapped(|ui| {
        ui.colored_label(
            condition_color(ui, if container.ready { "True" } else { "False" }),
            ready_text,
        );
        ui.label(format!("{} restarts", container.restarts));
        ui.label(format!("State: {}", container.state));
        if container.state_detail != "N/A" {
            wrapped_value(ui, &container.state_detail, 360.0);
        }
    });

    ui.add_space(4.0);
    describe_section(ui, egui_phosphor::regular::GAUGE, "Resources");
    describe_fields(ui, &container.resources);

    ui.add_space(4.0);
    describe_fields(
        ui,
        &[
            DescribeField::new("Ports", &container.ports),
            DescribeField::new("Env", &container.env_count),
            DescribeField::new("Mounts", &container.volume_mounts),
        ],
    );

    ui.add_space(4.0);
    describe_fields(ui, &container.probes);
}

fn describe_section(ui: &mut egui::Ui, icon: &str, title: &str) {
    ui.horizontal(|ui| {
        ui.label(icon);
        ui.strong(title);
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
                        [POD_DESCRIBE_FIELD_LABEL_WIDTH, 0.0],
                        egui::Label::new(egui::RichText::new(&field.label).weak()).wrap(),
                    );
                    wrapped_value(ui, &field.value, POD_DESCRIBE_FIELD_VALUE_WIDTH);
                }
                if chunk.len() == 1 {
                    ui.label("");
                    ui.label("");
                }
                ui.end_row();
            }
        });
}

fn describe_lines(ui: &mut egui::Ui, lines: &[String]) {
    if lines.is_empty() {
        ui.label("N/A");
        return;
    }

    for line in lines {
        wrapped_value(ui, line, POD_DESCRIBE_LINE_WIDTH);
    }
}

fn wrapped_value(ui: &mut egui::Ui, value: &str, width: f32) {
    ui.add_sized([width, 0.0], egui::Label::new(value).wrap());
}

fn condition_color(ui: &egui::Ui, status: &str) -> egui::Color32 {
    match status {
        "True" | "Ready" | "Running" => egui::Color32::from_rgb(46, 160, 67),
        "False" | "Not ready" | "Waiting" | "Pending" => egui::Color32::from_rgb(191, 135, 0),
        "Unknown" | "Terminated" | "Failed" | "Error" => ui.visuals().error_fg_color,
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

fn pod_describe_from_row(row: &PodRow) -> PodDescribe {
    let raw = &row.raw;
    PodDescribe {
        summary: vec![
            DescribeField::new("Name", row.name.clone()),
            DescribeField::new("Namespace", row.namespace.clone()),
            DescribeField::new("Status", row.status.clone()),
            DescribeField::new("Node", row.node.clone()),
            DescribeField::new("QoS", row.qos.clone()),
            DescribeField::new("Age", row.age.clone()),
            DescribeField::new("Owner", row.controlled_by.clone()),
            DescribeField::new("Restarts", row.restarts.clone()),
        ],
        metadata: pod_metadata_fields(raw),
        containers: pod_container_describes(raw),
        conditions: pod_condition_describes(raw),
        volumes: pod_volume_describes(raw),
        node_selectors: string_map_entries(raw.pointer("/spec/nodeSelector")),
        tolerations: pod_toleration_describes(raw),
        raw_yaml: full_manifest_yaml(raw),
    }
}

fn pod_metadata_fields(raw: &serde_json::Value) -> Vec<DescribeField> {
    vec![
        DescribeField::new(
            "Service account",
            value_str(raw, &["spec", "serviceAccountName"]).unwrap_or("N/A"),
        ),
        DescribeField::new(
            "Priority class",
            value_str(raw, &["spec", "priorityClassName"]).unwrap_or("N/A"),
        ),
        DescribeField::new(
            "Pod IP",
            value_str(raw, &["status", "podIP"]).unwrap_or("N/A"),
        ),
        DescribeField::new(
            "Host IP",
            value_str(raw, &["status", "hostIP"]).unwrap_or("N/A"),
        ),
        DescribeField::new(
            "Labels",
            string_map_summary(raw.pointer("/metadata/labels")),
        ),
        DescribeField::new(
            "Annotations",
            string_map_summary(raw.pointer("/metadata/annotations")),
        ),
    ]
}

fn pod_container_describes(raw: &serde_json::Value) -> Vec<ContainerDescribe> {
    raw.pointer("/spec/containers")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .map(|container| container_describe(raw, container))
        .collect()
}

fn container_describe(raw: &serde_json::Value, container: &serde_json::Value) -> ContainerDescribe {
    let name = value_str(container, &["name"]).unwrap_or("N/A").to_owned();
    let status = container_status_by_name(raw, &name);
    let ready = status
        .and_then(|status| value_bool(status, &["ready"]))
        .unwrap_or(false);
    let restarts = status
        .and_then(|status| value_u64(status, &["restartCount"]))
        .map_or_else(|| "N/A".to_owned(), |value| value.to_string());
    let (state, state_detail) = status
        .map(container_state)
        .unwrap_or_else(|| ("N/A".to_owned(), "N/A".to_owned()));

    ContainerDescribe {
        name,
        image: value_str(container, &["image"]).unwrap_or("N/A").to_owned(),
        ready,
        restarts,
        state,
        state_detail,
        resources: vec![
            DescribeField::new(
                "CPU request",
                value_str(container, &["resources", "requests", "cpu"]).unwrap_or("N/A"),
            ),
            DescribeField::new(
                "CPU limit",
                value_str(container, &["resources", "limits", "cpu"]).unwrap_or("N/A"),
            ),
            DescribeField::new(
                "Memory request",
                value_str(container, &["resources", "requests", "memory"]).unwrap_or("N/A"),
            ),
            DescribeField::new(
                "Memory limit",
                value_str(container, &["resources", "limits", "memory"]).unwrap_or("N/A"),
            ),
        ],
        ports: container_ports(container),
        env_count: value_array(container, &["env"]).map_or_else(
            || "0 vars".to_owned(),
            |items| format!("{} vars", items.len()),
        ),
        volume_mounts: container_volume_mounts(container),
        probes: vec![
            DescribeField::new("Liveness", probe_label(container, "livenessProbe")),
            DescribeField::new("Readiness", probe_label(container, "readinessProbe")),
            DescribeField::new("Startup", probe_label(container, "startupProbe")),
        ],
    }
}

fn container_status_by_name<'a>(
    raw: &'a serde_json::Value,
    name: &str,
) -> Option<&'a serde_json::Value> {
    raw.pointer("/status/containerStatuses")
        .and_then(serde_json::Value::as_array)?
        .iter()
        .find(|status| value_str(status, &["name"]) == Some(name))
}

fn container_state(status: &serde_json::Value) -> (String, String) {
    if let Some(waiting) = status.pointer("/state/waiting") {
        return (
            "Waiting".to_owned(),
            value_str(waiting, &["reason"]).unwrap_or("N/A").to_owned(),
        );
    }
    if let Some(terminated) = status.pointer("/state/terminated") {
        let reason = value_str(terminated, &["reason"]).unwrap_or("N/A");
        let exit_code = terminated
            .get("exitCode")
            .and_then(serde_json::Value::as_i64)
            .map_or_else(|| "N/A".to_owned(), |value| value.to_string());
        return (
            "Terminated".to_owned(),
            format!("{reason} exit {exit_code}"),
        );
    }
    if let Some(running) = status.pointer("/state/running") {
        return (
            "Running".to_owned(),
            value_str(running, &["startedAt"])
                .unwrap_or("N/A")
                .to_owned(),
        );
    }
    ("N/A".to_owned(), "N/A".to_owned())
}

fn container_ports(container: &serde_json::Value) -> String {
    let Some(ports) = value_array(container, &["ports"]) else {
        return "N/A".to_owned();
    };
    if ports.is_empty() {
        return "N/A".to_owned();
    }

    ports
        .iter()
        .map(|port| {
            let port_number = port
                .get("containerPort")
                .and_then(serde_json::Value::as_u64)
                .map_or_else(|| "N/A".to_owned(), |value| value.to_string());
            let protocol = value_str(port, &["protocol"]).unwrap_or("TCP");
            match value_str(port, &["name"]) {
                Some(name) => format!("{name}:{port_number}/{protocol}"),
                None => format!("{port_number}/{protocol}"),
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn container_volume_mounts(container: &serde_json::Value) -> String {
    let Some(mounts) = value_array(container, &["volumeMounts"]) else {
        return "N/A".to_owned();
    };
    if mounts.is_empty() {
        return "N/A".to_owned();
    }

    mounts
        .iter()
        .map(|mount| {
            let name = value_str(mount, &["name"]).unwrap_or("N/A");
            let path = value_str(mount, &["mountPath"]).unwrap_or("N/A");
            format!("{name} at {path}")
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn probe_label(container: &serde_json::Value, name: &str) -> String {
    if container.get(name).is_some() {
        format!("{} configured", egui_phosphor::regular::CHECK)
    } else {
        "N/A".to_owned()
    }
}

fn pod_condition_describes(raw: &serde_json::Value) -> Vec<ConditionDescribe> {
    raw.pointer("/status/conditions")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .map(|condition| ConditionDescribe {
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

fn pod_volume_describes(raw: &serde_json::Value) -> Vec<String> {
    raw.pointer("/spec/volumes")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .map(|volume| {
            let name = value_str(volume, &["name"]).unwrap_or("N/A");
            let kind = volume
                .as_object()
                .and_then(|object| object.keys().find(|key| key.as_str() != "name"))
                .map_or("N/A", String::as_str);
            format!("{name} ({kind})")
        })
        .collect()
}

fn pod_toleration_describes(raw: &serde_json::Value) -> Vec<String> {
    raw.pointer("/spec/tolerations")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .map(|toleration| {
            let key = value_str(toleration, &["key"]).unwrap_or("N/A");
            let operator = value_str(toleration, &["operator"]).unwrap_or("Equal");
            let effect = value_str(toleration, &["effect"]).unwrap_or("N/A");
            format!("{key} {operator} {effect}")
        })
        .collect()
}

fn string_map_summary(value: Option<&serde_json::Value>) -> String {
    let entries = string_map_entries(value);
    if entries.is_empty() {
        "N/A".to_owned()
    } else {
        entries.join(", ")
    }
}

fn string_map_entries(value: Option<&serde_json::Value>) -> Vec<String> {
    value
        .and_then(serde_json::Value::as_object)
        .into_iter()
        .flat_map(|object| {
            object.iter().map(|(key, value)| {
                let value = value
                    .as_str()
                    .map_or_else(|| value.to_string(), ToOwned::to_owned);
                format!("{key}={value}")
            })
        })
        .collect()
}

fn pod_key(namespace: &str, name: &str) -> String {
    format!("{namespace}/{name}")
}

fn default_pod_yaml(namespace: Option<&str>) -> String {
    let namespace = namespace.unwrap_or("default");
    format!(
        r#"apiVersion: v1
kind: Pod
metadata:
  name: example-pod
  namespace: {namespace}
spec:
  containers:
    - name: app
      image: nginx:latest
"#
    )
}

fn pod_apply_parts_from_yaml(
    yaml: &str,
) -> Result<(Option<String>, String, serde_json::Value), String> {
    let manifest =
        serde_yaml::from_str::<serde_json::Value>(yaml).map_err(|error| error.to_string())?;
    let name = value_str(&manifest, &["metadata", "name"])
        .filter(|name| !name.trim().is_empty())
        .ok_or_else(|| "metadata.name is required".to_owned())?
        .to_owned();
    let namespace = value_str(&manifest, &["metadata", "namespace"])
        .filter(|namespace| !namespace.trim().is_empty())
        .map(ToOwned::to_owned);

    Ok((namespace, name, manifest))
}

fn empty_to_none(value: String) -> Option<String> {
    if value.is_empty() || value == "N/A" {
        None
    } else {
        Some(value)
    }
}

fn editable_manifest(raw: &serde_json::Value) -> serde_json::Value {
    let mut manifest = raw.clone();
    if let Some(metadata) = manifest
        .get_mut("metadata")
        .and_then(serde_json::Value::as_object_mut)
    {
        for key in [
            "creationTimestamp",
            "generation",
            "managedFields",
            "resourceVersion",
            "selfLink",
            "uid",
        ] {
            metadata.remove(key);
        }
    }
    if let Some(object) = manifest.as_object_mut() {
        object.remove("status");
    }
    manifest
}

fn editable_manifest_yaml(raw: &serde_json::Value) -> String {
    serde_yaml::to_string(&editable_manifest(raw))
        .or_else(|_| serde_json::to_string_pretty(raw))
        .unwrap_or_default()
}

fn full_manifest_yaml(raw: &serde_json::Value) -> String {
    serde_yaml::to_string(raw)
        .or_else(|_| serde_json::to_string_pretty(raw))
        .unwrap_or_default()
}

fn container_names(raw: &serde_json::Value) -> Vec<String> {
    raw.pointer("/spec/containers")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|container| value_str(container, &["name"]))
        .map(ToOwned::to_owned)
        .collect()
}

fn resource_summaries(raw: &serde_json::Value) -> (String, String) {
    let Some(containers) = raw
        .pointer("/spec/containers")
        .and_then(serde_json::Value::as_array)
    else {
        return ("N/A".to_owned(), "N/A".to_owned());
    };

    let cpu = aggregate_named_resource(containers, "cpu");
    let memory = aggregate_named_resource(containers, "memory");
    (cpu, memory)
}

fn aggregate_named_resource(containers: &[serde_json::Value], name: &str) -> String {
    let requests = containers
        .iter()
        .filter_map(|container| value_str(container, &["resources", "requests", name]))
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    let limits = containers
        .iter()
        .filter_map(|container| value_str(container, &["resources", "limits", name]))
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();

    let request = total_resource_label(name, &requests);
    let limit = total_resource_label(name, &limits);

    match (request, limit) {
        (None, None) => "N/A".to_owned(),
        (Some(request), None) => request,
        (None, Some(limit)) => format!("N/A / {limit}"),
        (Some(request), Some(limit)) => format!("{request} / {limit}"),
    }
}

fn total_resource_label(name: &str, values: &[String]) -> Option<String> {
    if values.is_empty() {
        return None;
    }

    match name {
        "cpu" => values
            .iter()
            .map(|value| parse_cpu_millicores(value))
            .sum::<Option<u64>>()
            .map(format_cpu_millicores),
        "memory" => values
            .iter()
            .map(|value| parse_memory_bytes(value))
            .sum::<Option<u64>>()
            .map(format_memory_bytes),
        _ => Some(values.join(" + ")),
    }
}

fn parse_cpu_millicores(value: &str) -> Option<u64> {
    let value = value.trim();
    if let Some(milli) = value.strip_suffix('m') {
        return milli.parse().ok();
    }

    if let Some((whole, fraction)) = value.split_once('.') {
        let whole = whole.parse::<u64>().ok()?.saturating_mul(1000);
        let fraction_digits = fraction.chars().take(3).collect::<String>();
        let fraction = fraction_digits
            .chars()
            .chain(std::iter::repeat('0'))
            .take(3)
            .collect::<String>()
            .parse::<u64>()
            .ok()
            .unwrap_or(0);
        return Some(whole + fraction);
    }

    value
        .parse::<u64>()
        .ok()
        .map(|cores| cores.saturating_mul(1000))
}

fn format_cpu_millicores(value: u64) -> String {
    if value.is_multiple_of(1000) {
        (value / 1000).to_string()
    } else {
        format!("{value}m")
    }
}

fn parse_memory_bytes(value: &str) -> Option<u64> {
    let value = value.trim();
    for (suffix, multiplier) in [
        ("Ki", 1024_u64),
        ("Mi", 1024_u64.pow(2)),
        ("Gi", 1024_u64.pow(3)),
        ("Ti", 1024_u64.pow(4)),
        ("K", 1000_u64),
        ("M", 1000_u64.pow(2)),
        ("G", 1000_u64.pow(3)),
        ("T", 1000_u64.pow(4)),
    ] {
        if let Some(number) = value.strip_suffix(suffix) {
            return number
                .parse::<u64>()
                .ok()
                .map(|number| number.saturating_mul(multiplier));
        }
    }
    value.parse().ok()
}

fn format_memory_bytes(value: u64) -> String {
    for (suffix, multiplier) in [
        ("Ti", 1024_u64.pow(4)),
        ("Gi", 1024_u64.pow(3)),
        ("Mi", 1024_u64.pow(2)),
        ("Ki", 1024_u64),
    ] {
        if value >= multiplier && value.is_multiple_of(multiplier) {
            return format!("{}{}", value / multiplier, suffix);
        }
    }
    value.to_string()
}

fn owner_reference(raw: &serde_json::Value) -> Option<String> {
    let owner = raw
        .pointer("/metadata/ownerReferences")
        .and_then(serde_json::Value::as_array)?
        .first()?;
    let kind = value_str(owner, &["kind"])?;
    let name = value_str(owner, &["name"])?;
    Some(format!("{kind}/{name}"))
}

fn pod_status(raw: &serde_json::Value, summary_status: Option<&str>) -> String {
    if let Some(reason) = container_waiting_or_terminated_reason(raw) {
        return reason;
    }

    value_str(raw, &["status", "phase"])
        .or(summary_status)
        .unwrap_or("Unknown")
        .to_owned()
}

fn container_waiting_or_terminated_reason(raw: &serde_json::Value) -> Option<String> {
    raw.pointer("/status/containerStatuses")
        .and_then(serde_json::Value::as_array)?
        .iter()
        .find_map(|status| {
            value_str(status, &["state", "waiting", "reason"])
                .or_else(|| value_str(status, &["state", "terminated", "reason"]))
                .map(ToOwned::to_owned)
        })
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

fn value_array<'a>(
    value: &'a serde_json::Value,
    path: &[&str],
) -> Option<&'a Vec<serde_json::Value>> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    current.as_array()
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
    fn pod_row_extracts_table_fields_from_raw_summary() {
        let row = PodRow::from_summary(&pod_summary());

        assert_eq!(row.name, "api-75f");
        assert_eq!(row.namespace, "default");
        assert_eq!(row.cpu, "150m / 500m");
        assert_eq!(row.memory, "192Mi / 512Mi");
        assert_eq!(row.containers, "1/2");
        assert_eq!(row.container_names, vec!["api", "sidecar"]);
        assert_eq!(row.restarts, "3");
        assert_eq!(row.controlled_by, "ReplicaSet/api-75f");
        assert_eq!(row.node, "kind-worker");
        assert_eq!(row.qos, "Burstable");
        assert_eq!(row.status, "CrashLoopBackOff");
        assert!(row.age.ends_with(" ago"));
    }

    #[test]
    fn pod_rows_filter_by_name_case_insensitively() {
        let rows = vec![
            PodRow::from_summary(&pod_summary()),
            PodRow::from_summary(&ResourceSummary {
                name: "worker".to_owned(),
                namespace: Some("default".to_owned()),
                kind: "Pod".to_owned(),
                status: Some("Running".to_owned()),
                raw: serde_json::json!({"metadata": {"name": "worker", "namespace": "default"}}),
            }),
        ];

        let filtered = filter_pod_rows(&rows, "API");

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "api-75f");
    }

    #[test]
    fn pod_rows_are_sorted_by_namespace_and_name() {
        let rows = pod_rows_from_list(&[
            ResourceSummary {
                name: "worker".to_owned(),
                namespace: Some("zeta".to_owned()),
                kind: "Pod".to_owned(),
                status: None,
                raw: serde_json::json!({"metadata": {"namespace": "zeta", "name": "worker"}}),
            },
            ResourceSummary {
                name: "api".to_owned(),
                namespace: Some("default".to_owned()),
                kind: "Pod".to_owned(),
                status: None,
                raw: serde_json::json!({"metadata": {"namespace": "default", "name": "api"}}),
            },
            ResourceSummary {
                name: "scheduler".to_owned(),
                namespace: Some("default".to_owned()),
                kind: "Pod".to_owned(),
                status: None,
                raw: serde_json::json!({"metadata": {"namespace": "default", "name": "scheduler"}}),
            },
        ]);

        let keys = rows.into_iter().map(|row| row.key).collect::<Vec<_>>();
        assert_eq!(
            keys,
            vec!["default/api", "default/scheduler", "zeta/worker"]
        );
    }

    #[test]
    fn pod_create_yaml_extracts_apply_parts() {
        let (namespace, name, manifest) = pod_apply_parts_from_yaml(
            r#"
apiVersion: v1
kind: Pod
metadata:
  name: worker
  namespace: production
spec:
  containers: []
"#,
        )
        .unwrap();

        assert_eq!(namespace.as_deref(), Some("production"));
        assert_eq!(name, "worker");
        assert_eq!(
            manifest
                .pointer("/metadata/name")
                .and_then(serde_json::Value::as_str),
            Some("worker")
        );
    }

    #[test]
    fn selected_delete_targets_follow_checked_rows() {
        let mut panel = PodResourcePanel::default();
        let row = PodRow::from_summary(&pod_summary());
        panel.selected_rows.insert(row.key.clone());
        panel.rows.push(row);
        panel.rows.push(PodRow::from_summary(&ResourceSummary {
            name: "worker".to_owned(),
            namespace: Some("default".to_owned()),
            kind: "Pod".to_owned(),
            status: Some("Running".to_owned()),
            raw: serde_json::json!({"metadata": {"name": "worker", "namespace": "default"}}),
        }));

        let targets = panel.selected_delete_targets();

        assert_eq!(
            targets,
            vec![ResourceDeleteTarget {
                namespace: Some("default".to_owned()),
                name: "api-75f".to_owned(),
            }]
        );
    }

    #[test]
    fn pod_request_query_uses_selected_namespace() {
        let mut panel = PodResourcePanel {
            namespace_filter: Some("production".to_owned()),
            ..PodResourcePanel::default()
        };

        let request = panel.request_pods(ClusterId::new("local"));
        let query = request.query();

        assert_eq!(query.resource.plural, "pods");
        assert_eq!(query.namespace.as_deref(), Some("production"));
    }

    #[test]
    fn editable_manifest_removes_server_owned_fields() {
        let manifest = editable_manifest(&serde_json::json!({
            "metadata": {
                "name": "api",
                "namespace": "default",
                "managedFields": [],
                "resourceVersion": "42",
                "uid": "abc",
                "creationTimestamp": "2026-05-18T10:00:00Z"
            },
            "spec": {"containers": []},
            "status": {"phase": "Running"}
        }));

        assert!(manifest.pointer("/metadata/managedFields").is_none());
        assert!(manifest.pointer("/metadata/resourceVersion").is_none());
        assert!(manifest.pointer("/metadata/uid").is_none());
        assert!(manifest.pointer("/metadata/creationTimestamp").is_none());
        assert!(manifest.pointer("/status").is_none());
        assert_eq!(
            manifest.pointer("/spec/containers").unwrap(),
            &serde_json::json!([])
        );
    }

    #[test]
    fn pod_table_action_can_describe_row() {
        assert_eq!(
            PodTableAction::Describe {
                key: "default/api-75f".to_owned()
            },
            PodTableAction::Describe {
                key: "default/api-75f".to_owned()
            }
        );
    }

    #[test]
    fn pod_table_action_can_attach_row() {
        assert_eq!(
            PodTableAction::Attach {
                key: "default/api-75f".to_owned()
            },
            PodTableAction::Attach {
                key: "default/api-75f".to_owned()
            }
        );
    }

    #[test]
    fn pod_attach_status_labels_are_stable() {
        assert_eq!(
            pod_attach_status_label(&PodAttachStatus::Disconnected),
            "Disconnected"
        );
        assert_eq!(
            pod_attach_status_label(&PodAttachStatus::Connecting),
            "Connecting"
        );
        assert_eq!(
            pod_attach_status_label(&PodAttachStatus::Attached),
            "Attached"
        );
        assert_eq!(
            pod_attach_status_label(&PodAttachStatus::Error("denied".to_owned())),
            "Error: denied"
        );
    }

    #[test]
    fn pod_describe_extracts_container_details() {
        let row = PodRow::from_summary(&pod_summary());
        let describe = pod_describe_from_row(&row);

        assert_eq!(describe.containers.len(), 2);
        let api = &describe.containers[0];
        assert_eq!(api.name, "api");
        assert_eq!(api.image, "ghcr.io/example/api:1.0.0");
        assert!(api.ready);
        assert_eq!(api.restarts, "1");
        assert_eq!(api.state, "Running");
        assert_eq!(api.ports, "http:8080/TCP");
        assert_eq!(api.env_count, "1 vars");
        assert_eq!(api.volume_mounts, "config at /etc/config");
        assert_eq!(
            api.resources,
            vec![
                DescribeField::new("CPU request", "100m"),
                DescribeField::new("CPU limit", "500m"),
                DescribeField::new("Memory request", "128Mi"),
                DescribeField::new("Memory limit", "512Mi"),
            ]
        );
        assert_eq!(
            api.probes[1].value,
            format!("{} configured", egui_phosphor::regular::CHECK)
        );

        let sidecar = &describe.containers[1];
        assert_eq!(sidecar.state, "Waiting");
        assert_eq!(sidecar.state_detail, "CrashLoopBackOff");

        assert_eq!(describe.conditions[0].condition_type, "Ready");
        assert_eq!(describe.volumes, vec!["config (configMap)"]);
        assert_eq!(describe.node_selectors, vec!["disk=ssd"]);
        assert_eq!(describe.tolerations, vec!["dedicated Equal NoSchedule"]);
    }

    #[test]
    fn pod_describe_handles_missing_optional_fields() {
        let row = PodRow::from_summary(&ResourceSummary {
            name: "minimal".to_owned(),
            namespace: Some("default".to_owned()),
            kind: "Pod".to_owned(),
            status: Some("Pending".to_owned()),
            raw: serde_json::json!({
                "metadata": {"name": "minimal", "namespace": "default"},
                "spec": {"containers": [{"name": "app"}]},
                "status": {}
            }),
        });

        let describe = pod_describe_from_row(&row);

        assert_eq!(describe.containers.len(), 1);
        assert_eq!(describe.containers[0].image, "N/A");
        assert_eq!(describe.containers[0].restarts, "N/A");
        assert_eq!(describe.containers[0].state, "N/A");
        assert_eq!(describe.containers[0].ports, "N/A");
        assert_eq!(describe.containers[0].volume_mounts, "N/A");
        assert!(describe.conditions.is_empty());
        assert!(describe.volumes.is_empty());
    }

    #[test]
    fn namespaces_are_sorted_and_deduplicated() {
        let list = ResourceList {
            items: vec![
                namespace_summary("kube-system"),
                namespace_summary("default"),
                namespace_summary("default"),
            ],
            continue_token: None,
        };

        assert_eq!(
            namespaces_from_list(&list),
            vec!["default".to_owned(), "kube-system".to_owned()]
        );
    }

    #[test]
    fn stale_resource_events_do_not_replace_current_rows() {
        let mut panel = PodResourcePanel::default();
        let cluster_id = ClusterId::new("local");
        let first = panel.request_pods(cluster_id.clone());
        let second = panel.request_pods(cluster_id);

        panel.apply_event(ResourceUiEvent::ResourcesLoaded {
            request: first,
            result: Ok(ResourceList {
                items: vec![ResourceSummary {
                    name: "stale".to_owned(),
                    namespace: Some("default".to_owned()),
                    kind: "Pod".to_owned(),
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
                items: vec![pod_summary()],
                continue_token: None,
            }),
        });

        assert_eq!(panel.rows.len(), 1);
        assert_eq!(panel.rows[0].name, "api-75f");
    }

    #[test]
    fn stale_watch_events_do_not_replace_current_rows() {
        let mut panel = PodResourcePanel::default();
        let cluster_id = ClusterId::new("local");
        let first = panel.request_pod_watch(cluster_id.clone());
        let second = panel.request_pod_watch(cluster_id);

        panel.apply_event(ResourceUiEvent::ResourceWatchUpdated {
            request: first,
            result: Ok(miku_api::ResourceEvent::Snapshot(ResourceList {
                items: vec![ResourceSummary {
                    name: "stale".to_owned(),
                    namespace: Some("default".to_owned()),
                    kind: "Pod".to_owned(),
                    status: None,
                    raw: serde_json::json!({"metadata": {"name": "stale"}}),
                }],
                continue_token: None,
            })),
        });
        assert!(panel.rows.is_empty());

        panel.apply_event(ResourceUiEvent::ResourceWatchUpdated {
            request: second,
            result: Ok(miku_api::ResourceEvent::Snapshot(ResourceList {
                items: vec![pod_summary()],
                continue_token: None,
            })),
        });

        assert_eq!(panel.rows.len(), 1);
        assert_eq!(panel.rows[0].name, "api-75f");
    }

    #[test]
    fn watch_snapshot_replaces_rows_and_retains_existing_selection() {
        let mut panel = PodResourcePanel::default();
        let cluster_id = ClusterId::new("local");
        let request = panel.request_pod_watch(cluster_id);
        panel.rows = vec![
            PodRow::from_summary(&pod_summary()),
            PodRow::from_summary(&ResourceSummary {
                name: "worker".to_owned(),
                namespace: Some("default".to_owned()),
                kind: "Pod".to_owned(),
                status: Some("Running".to_owned()),
                raw: serde_json::json!({"metadata": {"name": "worker", "namespace": "default"}}),
            }),
        ];
        panel.selected_rows.insert("default/api-75f".to_owned());
        panel.selected_rows.insert("default/worker".to_owned());

        panel.apply_event(ResourceUiEvent::ResourceWatchUpdated {
            request,
            result: Ok(miku_api::ResourceEvent::Snapshot(ResourceList {
                items: vec![pod_summary()],
                continue_token: None,
            })),
        });

        assert_eq!(panel.rows.len(), 1);
        assert!(panel.selected_rows.contains("default/api-75f"));
        assert!(!panel.selected_rows.contains("default/worker"));
    }

    fn pod_summary() -> ResourceSummary {
        ResourceSummary {
            name: "api-75f".to_owned(),
            namespace: Some("default".to_owned()),
            kind: "Pod".to_owned(),
            status: Some("Running".to_owned()),
            raw: serde_json::json!({
                "metadata": {
                    "name": "api-75f",
                    "namespace": "default",
                    "creationTimestamp": "2026-05-18T10:00:00Z",
                    "labels": {"app": "api"},
                    "annotations": {"checksum/config": "abc123"},
                    "ownerReferences": [
                        {"kind": "ReplicaSet", "name": "api-75f"}
                    ]
                },
                "spec": {
                    "nodeName": "kind-worker",
                    "serviceAccountName": "api",
                    "nodeSelector": {"disk": "ssd"},
                    "tolerations": [
                        {"key": "dedicated", "operator": "Equal", "effect": "NoSchedule"}
                    ],
                    "volumes": [
                        {"name": "config", "configMap": {"name": "api-config"}}
                    ],
                    "containers": [
                        {
                            "name": "api",
                            "image": "ghcr.io/example/api:1.0.0",
                            "ports": [
                                {"name": "http", "containerPort": 8080, "protocol": "TCP"}
                            ],
                            "env": [
                                {"name": "RUST_LOG", "value": "info"}
                            ],
                            "volumeMounts": [
                                {"name": "config", "mountPath": "/etc/config"}
                            ],
                            "readinessProbe": {"httpGet": {"path": "/health", "port": 8080}},
                            "resources": {
                                "requests": {"cpu": "100m", "memory": "128Mi"},
                                "limits": {"cpu": "500m", "memory": "512Mi"}
                            }
                        },
                        {
                            "name": "sidecar",
                            "image": "ghcr.io/example/sidecar:1.0.0",
                            "resources": {
                                "requests": {"cpu": "50m", "memory": "64Mi"}
                            }
                        }
                    ]
                },
                "status": {
                    "phase": "Running",
                    "qosClass": "Burstable",
                    "podIP": "10.244.0.42",
                    "hostIP": "172.18.0.2",
                    "conditions": [
                        {"type": "Ready", "status": "False", "reason": "ContainersNotReady", "message": "sidecar is waiting"}
                    ],
                    "containerStatuses": [
                        {
                            "name": "api",
                            "ready": true,
                            "restartCount": 1,
                            "state": {"running": {"startedAt": "2026-05-18T10:00:30Z"}}
                        },
                        {
                            "name": "sidecar",
                            "ready": false,
                            "restartCount": 2,
                            "state": {"waiting": {"reason": "CrashLoopBackOff"}}
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
            status: None,
            raw: serde_json::json!({"metadata": {"name": name}}),
        }
    }
}
