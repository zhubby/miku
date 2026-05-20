use std::collections::BTreeSet;

use eframe::egui;
use egui_extras::{Column, TableBuilder};
use miku_api::{LogLine, ResourceSummary};
use miku_core::ClusterId;

#[cfg(test)]
use super::ResourceLoadRequest;
use super::components::{ResourceToolbar, ResourceYamlEditDialog, ResourceYamlViewDialog};
use super::{
    LoadStatus, PodLogRequest, ResourceActionKind, ResourceActionOutcome, ResourceActionRequest,
    ResourceDeleteTarget, ResourceLoadKind, ResourcePanelRequests, ResourceUiEvent,
    ResourceWatchRequest, namespaces_from_list,
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
    view_dialog: Option<PodViewDialog>,
    edit_dialog: Option<PodEditDialog>,
    delete_dialog: Option<PodDeleteDialog>,
    batch_delete_dialog: Option<PodBatchDeleteDialog>,
    evict_dialog: Option<PodEvictDialog>,
    log_dialog: Option<PodLogDialog>,
    action_error: Option<String>,
    refresh_after_action: bool,
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
        if self.refresh_after_action && !matches!(self.row_status, LoadStatus::Loading) {
            self.refresh_after_action = false;
            requests
                .watches
                .push(self.request_pod_watch(cluster_id.clone()));
        }

        self.show_toolbar(ui, cluster_id, &mut requests);
        ui.separator();
        self.show_body(ui);
        self.show_create_dialog(ui.ctx(), cluster_id, &mut requests.actions);
        self.show_view_dialog(ui.ctx());
        self.show_edit_dialog(ui.ctx(), cluster_id, &mut requests.actions);
        self.show_delete_dialog(ui.ctx(), cluster_id, &mut requests.actions);
        self.show_batch_delete_dialog(ui.ctx(), cluster_id, &mut requests.actions);
        self.show_evict_dialog(ui.ctx(), cluster_id, &mut requests.actions);
        self.show_log_dialog(ui.ctx(), cluster_id, &mut requests.logs);

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
                        self.refresh_after_action = true;
                    }
                    Ok(ResourceActionOutcome::Deleted) => {
                        if let ResourceActionKind::DeletePod { namespace, name } = request.kind {
                            let key = pod_key(namespace.as_deref().unwrap_or(""), &name);
                            self.rows.retain(|row| row.key != key);
                            self.selected_rows.remove(&key);
                        }
                        self.delete_dialog = None;
                        self.action_error = None;
                        self.refresh_after_action = true;
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
                        self.refresh_after_action = true;
                    }
                    Ok(ResourceActionOutcome::Evicted) => {
                        self.evict_dialog = None;
                        self.action_error = None;
                        self.refresh_after_action = true;
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
        self.view_dialog = None;
        self.edit_dialog = None;
        self.delete_dialog = None;
        self.batch_delete_dialog = None;
        self.evict_dialog = None;
        self.log_dialog = None;
        self.action_error = None;
        self.refresh_after_action = false;
    }

    fn show_toolbar(
        &mut self,
        ui: &mut egui::Ui,
        cluster_id: &ClusterId,
        requests: &mut ResourcePanelRequests,
    ) {
        let filtered_rows = self.filtered_rows();
        let response = ResourceToolbar {
            id_salt: "pod_resource_toolbar",
            namespaces: &self.namespaces,
            namespace_filter: &mut self.namespace_filter,
            search_text: &mut self.search_text,
            search_hint: "Search Pods...",
            item_count: filtered_rows.len(),
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
                .filtered_rows()
                .into_iter()
                .map(|row| row.key)
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
                let rows = self.filtered_rows();
                if rows.is_empty() {
                    ui.centered_and_justified(|ui| {
                        ui.label("No pods match the current filters.");
                    });
                    return;
                }

                let action = show_pod_table(ui, rows, &mut self.selected_rows);
                self.apply_table_action(action);
            }
        }
    }

    fn apply_table_action(&mut self, action: Option<PodTableAction>) {
        match action {
            Some(PodTableAction::Logs(row)) => {
                let containers = row.container_names.clone();
                let selected_container = containers.first().cloned();
                self.log_dialog = Some(PodLogDialog {
                    key: row.key,
                    namespace: empty_to_none(row.namespace).unwrap_or_else(|| "default".to_owned()),
                    pod: row.name,
                    containers,
                    selected_container,
                    lines: Vec::new(),
                    status: LoadStatus::Idle,
                    error: None,
                });
            }
            Some(PodTableAction::Evict(row)) => {
                self.evict_dialog = Some(PodEvictDialog {
                    key: row.key,
                    namespace: empty_to_none(row.namespace).unwrap_or_else(|| "default".to_owned()),
                    name: row.name,
                });
                self.action_error = None;
            }
            Some(PodTableAction::View(row)) => {
                self.view_dialog = Some(PodViewDialog {
                    key: row.key,
                    name: row.name,
                    yaml: row.full_yaml,
                });
            }
            Some(PodTableAction::Edit(row)) => {
                self.edit_dialog = Some(PodEditDialog {
                    key: row.key,
                    namespace: empty_to_none(row.namespace),
                    name: row.name,
                    yaml: row.edit_yaml,
                    parse_error: None,
                });
                self.action_error = None;
            }
            Some(PodTableAction::Delete(row)) => {
                self.delete_dialog = Some(PodDeleteDialog {
                    key: row.key,
                    namespace: empty_to_none(row.namespace),
                    name: row.name,
                });
                self.action_error = None;
            }
            None => {}
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

    fn filtered_rows(&self) -> Vec<PodRow> {
        filter_pod_rows(&self.rows, &self.search_text)
    }

    fn replace_rows(&mut self, rows: Vec<PodRow>) {
        let visible_keys = rows
            .iter()
            .map(|row| row.key.clone())
            .collect::<BTreeSet<_>>();
        self.selected_rows.retain(|key| visible_keys.contains(key));
        self.rows = rows;
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
    rows: Vec<PodRow>,
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
                    body.rows(row_height, rows.len(), |mut table_row| {
                        let row_index = table_row.index();
                        let Some(row) = rows.get(row_index) else {
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
                                action = Some(PodTableAction::Logs(row.clone()));
                                ui.close();
                            }
                            ui.add_enabled(
                                false,
                                egui::Button::new(format!(
                                    "{} Shell",
                                    egui_phosphor::regular::TERMINAL
                                )),
                            )
                            .on_disabled_hover_text("Shell is not implemented yet.");
                            let evict_text = egui::RichText::new(format!(
                                "{} Evict",
                                egui_phosphor::regular::WARNING
                            ))
                            .color(evict_color());
                            if ui.button(evict_text).clicked() {
                                action = Some(PodTableAction::Evict(row.clone()));
                                ui.close();
                            }
                            ui.separator();
                            if ui
                                .button(format!("{} View", egui_phosphor::regular::EYE))
                                .clicked()
                            {
                                action = Some(PodTableAction::View(row.clone()));
                                ui.close();
                            }
                            if ui
                                .button(format!("{} Edit", egui_phosphor::regular::PENCIL_SIMPLE))
                                .clicked()
                            {
                                action = Some(PodTableAction::Edit(row.clone()));
                                ui.close();
                            }
                            let delete_text = egui::RichText::new(format!(
                                "{} Delete",
                                egui_phosphor::regular::TRASH
                            ))
                            .color(ui.visuals().error_fg_color);
                            if ui.button(delete_text).clicked() {
                                action = Some(PodTableAction::Delete(row.clone()));
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

fn filter_pod_rows(rows: &[PodRow], search_text: &str) -> Vec<PodRow> {
    let needle = search_text.trim().to_lowercase();
    rows.iter()
        .filter(|row| needle.is_empty() || row.name.to_lowercase().contains(&needle))
        .cloned()
        .collect()
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
    edit_yaml: String,
    full_yaml: String,
}

impl PodRow {
    fn from_summary(summary: &ResourceSummary) -> Self {
        let raw = &summary.raw;
        let name = value_str(raw, &["metadata", "name"]).unwrap_or(&summary.name);
        let namespace = value_str(raw, &["metadata", "namespace"])
            .or(summary.namespace.as_deref())
            .unwrap_or("N/A");
        let key = pod_key(namespace, name);
        let full_yaml = full_manifest_yaml(&summary.raw);
        let editable_manifest = editable_manifest(&summary.raw);
        let edit_yaml = serde_yaml::to_string(&editable_manifest)
            .or_else(|_| serde_json::to_string_pretty(&summary.raw))
            .unwrap_or_default();
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
            edit_yaml,
            full_yaml,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum PodTableAction {
    Logs(PodRow),
    Evict(PodRow),
    View(PodRow),
    Edit(PodRow),
    Delete(PodRow),
}

#[derive(Clone, Debug, PartialEq)]
struct PodCreateDialog {
    yaml: String,
    parse_error: Option<String>,
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
                    "ownerReferences": [
                        {"kind": "ReplicaSet", "name": "api-75f"}
                    ]
                },
                "spec": {
                    "nodeName": "kind-worker",
                    "containers": [
                        {
                            "name": "api",
                            "resources": {
                                "requests": {"cpu": "100m", "memory": "128Mi"},
                                "limits": {"cpu": "500m", "memory": "512Mi"}
                            }
                        },
                        {
                            "name": "sidecar",
                            "resources": {
                                "requests": {"cpu": "50m", "memory": "64Mi"}
                            }
                        }
                    ]
                },
                "status": {
                    "phase": "Running",
                    "qosClass": "Burstable",
                    "containerStatuses": [
                        {"ready": true, "restartCount": 1, "state": {"running": {}}},
                        {
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
