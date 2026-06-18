use std::collections::BTreeSet;

use eframe::egui::{self, TextWrapMode};
use egui_extras::{Column, TableBuilder};
use miku_api::ResourceSummary;
use miku_core::{ClusterId, ResourceRef};

#[cfg(test)]
use super::ResourceLoadRequest;
use super::components::ResourceYamlViewDialog;
use super::components::{
    GenericBatchDeleteDialog, GenericCreateDialog, GenericDeleteDialog, GenericEditDialog,
    ResourceBatchDeleteDialogInput, ResourceCreateDialogInput, ResourceCreateDialogResponse,
    ResourceDeleteDialogInput, ResourceDeleteDialogResponse, ResourceEditDialogInput,
    ResourceEditDialogResponse, ResourceMetadata, ResourceRowTarget, ResourceToolbar,
    SELECT_COLUMN_WIDTH, apply_resource_request, batch_delete_resource_request,
    default_resource_yaml, delete_resource_request, edit_resource_request, editable_resource_yaml,
    selected_delete_targets, show_resource_batch_delete_dialog, show_resource_create_dialog,
    show_resource_delete_dialog, show_resource_edit_dialog, show_row_selection_checkbox,
    visible_keys,
};
use super::{
    LoadStatus, ResourceActionKind, ResourceActionOutcome, ResourceLoadKind, ResourcePanelRequests,
    ResourceUiEvent, ResourceWatchRequest, namespaces_from_list,
};
use crate::time::human_age_from_rfc3339;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ConfigResourceKind {
    HorizontalPodAutoscaler,
    PodDisruptionBudget,
    PriorityClass,
    RuntimeClass,
    Lease,
    MutatingWebhookConfiguration,
    ValidatingWebhookConfiguration,
}

#[derive(Clone, Debug)]
pub(crate) struct ConfigResourcePanel {
    kind: ConfigResourceKind,
    namespace_filter: Option<String>,
    search_text: String,
    namespaces: Vec<String>,
    namespace_status: LoadStatus,
    row_status: LoadStatus,
    rows: Vec<ConfigRow>,
    rows_version: u64,
    filter_cache: ConfigFilterCache,
    selected_rows: BTreeSet<String>,
    next_request_id: u64,
    namespace_request_id: Option<u64>,
    row_request_id: Option<u64>,
    namespace_watch_request_id: Option<u64>,
    row_watch_request_id: Option<u64>,
    last_cluster_id: Option<ClusterId>,
    describe_dialog: Option<ConfigDescribeDialog>,
    view_dialog: Option<ConfigViewDialog>,
    edit_dialog: Option<GenericEditDialog>,
    create_dialog: Option<GenericCreateDialog>,
    batch_delete_dialog: Option<GenericBatchDeleteDialog>,
    delete_dialog: Option<GenericDeleteDialog>,
    action_request_id: Option<u64>,
    action_error: Option<String>,
}

impl ConfigResourcePanel {
    pub(crate) fn new(kind: ConfigResourceKind) -> Self {
        Self {
            kind,
            namespace_filter: None,
            search_text: String::new(),
            namespaces: Vec::new(),
            namespace_status: LoadStatus::Idle,
            row_status: LoadStatus::Idle,
            rows: Vec::new(),
            rows_version: 0,
            filter_cache: ConfigFilterCache::default(),
            selected_rows: BTreeSet::new(),
            next_request_id: 0,
            namespace_request_id: None,
            row_request_id: None,
            namespace_watch_request_id: None,
            row_watch_request_id: None,
            last_cluster_id: None,
            describe_dialog: None,
            view_dialog: None,
            edit_dialog: None,
            create_dialog: None,
            batch_delete_dialog: None,
            delete_dialog: None,
            action_request_id: None,
            action_error: None,
        }
    }

    pub(crate) fn show(
        &mut self,
        ui: &mut egui::Ui,
        cluster_id: Option<&ClusterId>,
    ) -> ResourcePanelRequests {
        let mut requests = ResourcePanelRequests::default();
        let Some(cluster_id) = cluster_id else {
            ui.centered_and_justified(|ui| {
                ui.label(format!("Select a cluster to load {}.", self.kind.title()));
            });
            return requests;
        };

        self.reset_for_cluster_change(cluster_id);
        if self.kind.is_namespaced() && matches!(self.namespace_status, LoadStatus::Idle) {
            requests
                .watches
                .push(self.request_namespace_watch(cluster_id.clone()));
        }
        if matches!(self.row_status, LoadStatus::Idle) {
            requests
                .watches
                .push(self.request_resource_watch(cluster_id.clone()));
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
            ResourceUiEvent::ResourcesLoaded { request, result } => {
                if matches!(request.kind, ResourceLoadKind::Namespaces) {
                    self.apply_namespaces_load(request.request_id, result);
                } else if self.kind.matches_load_kind(&request.kind) {
                    self.apply_rows_load(request.request_id, result);
                }
            }
            ResourceUiEvent::ResourceWatchUpdated { request, result } => {
                if matches!(request.kind, ResourceLoadKind::Namespaces) {
                    self.apply_namespaces_watch(request.request_id, result);
                } else if self.kind.matches_load_kind(&request.kind) {
                    self.apply_rows_watch(request.request_id, result);
                }
            }
            ResourceUiEvent::ResourceActionCompleted { request, result } => {
                if self.action_request_id != Some(request.request_id) {
                    return;
                }
                self.action_request_id = None;
                match result {
                    Ok(ResourceActionOutcome::Applied(summary)) => {
                        self.upsert_row(row_from_summary(self.kind, &summary));
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
                            && resource == self.kind.metadata().resource
                        {
                            let key = config_key(namespace.as_deref().unwrap_or(""), &name);
                            self.rows.retain(|row| row.key != key);
                            self.invalidate_rows();
                            self.selected_rows.remove(&key);
                        }
                        self.delete_dialog = None;
                        self.action_error = None;
                    }
                    Ok(ResourceActionOutcome::BatchDeleted(targets)) => {
                        for target in targets {
                            let key =
                                config_key(target.namespace.as_deref().unwrap_or(""), &target.name);
                            self.rows.retain(|row| row.key != key);
                            self.invalidate_rows();
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

    fn apply_namespaces_load(
        &mut self,
        request_id: u64,
        result: Result<miku_api::ResourceList, String>,
    ) {
        if self.namespace_request_id == Some(request_id) {
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

    fn apply_rows_load(&mut self, request_id: u64, result: Result<miku_api::ResourceList, String>) {
        if self.row_request_id != Some(request_id) {
            return;
        }
        self.row_request_id = None;
        match result {
            Ok(list) => {
                self.replace_rows(self.kind.rows_from_list(&list.items));
                self.row_status = LoadStatus::Loaded;
            }
            Err(error) => self.row_status = LoadStatus::Error(error),
        }
    }

    fn apply_namespaces_watch(
        &mut self,
        request_id: u64,
        result: Result<miku_api::ResourceEvent, String>,
    ) {
        if self.namespace_watch_request_id == Some(request_id) {
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

    fn apply_rows_watch(
        &mut self,
        request_id: u64,
        result: Result<miku_api::ResourceEvent, String>,
    ) {
        if self.row_watch_request_id != Some(request_id) {
            return;
        }
        match result {
            Ok(miku_api::ResourceEvent::Snapshot(list)) => {
                self.replace_rows(self.kind.rows_from_list(&list.items));
                self.row_status = LoadStatus::Loaded;
            }
            Ok(_) => {}
            Err(error) => self.row_status = LoadStatus::Error(error),
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
        self.invalidate_rows();
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
            id_salt: self.kind.id(),
            namespaces: if self.kind.is_namespaced() {
                &self.namespaces
            } else {
                &[]
            },
            namespace_filter: &mut self.namespace_filter,
            search_text: &mut self.search_text,
            search_hint: "Search resources...",
            item_count,
            selected_count: self.selected_rows.len(),
            loading: matches!(self.row_status, LoadStatus::Loading),
        }
        .show(ui);

        if response.namespace_changed && self.kind.is_namespaced() {
            requests
                .watches
                .push(self.request_resource_watch(cluster_id.clone()));
        }
        if response.search_changed {
            self.prune_selection_to_visible();
        }
        if response.refresh_clicked {
            if self.kind.is_namespaced() {
                requests
                    .watches
                    .push(self.request_namespace_watch(cluster_id.clone()));
            }
            requests
                .watches
                .push(self.request_resource_watch(cluster_id.clone()));
        }
        if response.create_clicked {
            let metadata = self.kind.metadata();
            self.create_dialog = Some(GenericCreateDialog {
                yaml: default_resource_yaml(metadata, self.namespace_filter.as_deref()),
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
        if matches!(self.namespace_status, LoadStatus::Error(_)) && self.kind.is_namespaced() {
            ui.colored_label(ui.visuals().error_fg_color, "Namespaces unavailable");
        }
    }

    fn show_body(&mut self, ui: &mut egui::Ui) {
        match &self.row_status {
            LoadStatus::Idle | LoadStatus::Loading if self.rows.is_empty() => {
                ui.centered_and_justified(|ui| {
                    ui.label(format!("Loading {}...", self.kind.title()));
                });
            }
            LoadStatus::Error(error) => {
                ui.centered_and_justified(|ui| {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                });
            }
            _ => {
                let indices = self.filtered_row_indices();
                if indices.is_empty() {
                    ui.centered_and_justified(|ui| {
                        ui.label(format!(
                            "No {} match the current filters.",
                            self.kind.title()
                        ));
                    });
                    return;
                }
                let action =
                    show_config_table(ui, self.kind, &self.rows, indices, &mut self.selected_rows);
                self.apply_table_action(action);
            }
        }
    }

    fn apply_table_action(&mut self, action: Option<ConfigTableAction>) {
        match action {
            Some(ConfigTableAction::Describe { key }) => {
                let Some((name, describe)) = self
                    .row_by_key(&key)
                    .map(|row| (row.name.clone(), describe_from_row(self.kind, row)))
                else {
                    return;
                };
                self.describe_dialog = Some(ConfigDescribeDialog {
                    key,
                    name,
                    describe,
                });
            }
            Some(ConfigTableAction::View { key }) => {
                let Some((name, yaml)) = self
                    .row_by_key(&key)
                    .map(|row| (row.name.clone(), full_manifest_yaml(&row.raw)))
                else {
                    return;
                };
                self.view_dialog = Some(ConfigViewDialog { key, name, yaml });
            }
            Some(ConfigTableAction::Edit { key }) => {
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
            Some(ConfigTableAction::Delete { key }) => {
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
        egui::Window::new(format!("Describe {}", dialog.name))
            .id(egui::Id::new((self.kind.id(), "describe", &dialog.key)))
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .open(&mut open)
            .collapsible(false)
            .fixed_size([860.0, 580.0])
            .show(ctx, |ui| {
                egui::ScrollArea::both()
                    .id_salt((self.kind.id(), "describe_content", &dialog.key))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_min_width(1120.0);
                        ui.style_mut().wrap_mode = Some(TextWrapMode::Extend);
                        show_config_describe(ui, &dialog.describe);
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
            id: egui::Id::new((self.kind.id(), "view", &dialog.key)),
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
                metadata: self.kind.metadata(),
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
                    self.kind.metadata(),
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
                metadata: self.kind.metadata(),
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
                    self.kind.metadata(),
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
                metadata: self.kind.metadata(),
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
                    self.kind.metadata(),
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
                metadata: self.kind.metadata(),
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
                    self.kind.metadata(),
                    dialog.target,
                );
                self.action_request_id = Some(request.request_id);
                requests.push(request);
            }
        }
    }

    #[cfg(test)]
    fn request_resources(&mut self, cluster_id: ClusterId) -> ResourceLoadRequest {
        let request = ResourceLoadRequest {
            request_id: self.allocate_request_id(),
            cluster_id,
            kind: self.kind.load_kind(self.namespace_filter.clone()),
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

    fn request_resource_watch(&mut self, cluster_id: ClusterId) -> ResourceWatchRequest {
        let request = ResourceWatchRequest {
            request_id: self.allocate_request_id(),
            cluster_id,
            kind: self.kind.load_kind(self.namespace_filter.clone()),
        };
        self.row_watch_request_id = Some(request.request_id);
        self.row_status = LoadStatus::Loading;
        request
    }

    fn allocate_request_id(&mut self) -> u64 {
        self.next_request_id += 1;
        self.next_request_id
    }

    fn filtered_row_count(&mut self) -> usize {
        self.filtered_row_indices().len()
    }

    fn filtered_row_indices(&mut self) -> Vec<usize> {
        if self.filter_cache.rows_version != self.rows_version
            || self.filter_cache.search_text != self.search_text
        {
            self.filter_cache.rows_version = self.rows_version;
            self.filter_cache.search_text.clone_from(&self.search_text);
            self.filter_cache.indices = self
                .rows
                .iter()
                .enumerate()
                .filter_map(|(index, row)| {
                    row_matches_search(row, &self.search_text).then_some(index)
                })
                .collect();
        }
        self.filter_cache.indices.clone()
    }

    fn row_by_key(&self, key: &str) -> Option<&ConfigRow> {
        self.rows.iter().find(|row| row.key == key)
    }

    fn replace_rows(&mut self, rows: Vec<ConfigRow>) {
        let targets = rows.iter().map(ConfigRow::target).collect::<Vec<_>>();
        let visible_keys = visible_keys(&targets);
        self.selected_rows.retain(|key| visible_keys.contains(key));
        self.rows = rows;
        self.invalidate_rows();
    }

    fn upsert_row(&mut self, row: ConfigRow) {
        if let Some(existing) = self
            .rows
            .iter_mut()
            .find(|existing| existing.key == row.key)
        {
            *existing = row;
        } else {
            self.rows.push(row);
        }
        if self.kind.is_namespaced() {
            self.rows.sort_by(|left, right| {
                left.namespace
                    .cmp(&right.namespace)
                    .then(left.name.cmp(&right.name))
            });
        } else {
            self.rows.sort_by(|left, right| left.name.cmp(&right.name));
        }
        self.invalidate_rows();
    }

    fn invalidate_rows(&mut self) {
        self.rows_version = self.rows_version.wrapping_add(1);
        self.filter_cache.indices.clear();
    }

    fn prune_selection_to_visible(&mut self) {
        let targets = self
            .filtered_row_indices()
            .into_iter()
            .filter_map(|index| self.rows.get(index))
            .map(ConfigRow::target)
            .collect::<Vec<_>>();
        let visible_keys = visible_keys(&targets);
        self.selected_rows.retain(|key| visible_keys.contains(key));
    }

    fn selected_delete_targets(&self) -> Vec<super::ResourceDeleteTarget> {
        let targets = self.rows.iter().map(ConfigRow::target).collect::<Vec<_>>();
        selected_delete_targets(&targets, &self.selected_rows)
    }
}

impl ConfigResourceKind {
    fn id(self) -> &'static str {
        match self {
            Self::HorizontalPodAutoscaler => "horizontal_pod_autoscaler",
            Self::PodDisruptionBudget => "pod_disruption_budget",
            Self::PriorityClass => "priority_class",
            Self::RuntimeClass => "runtime_class",
            Self::Lease => "lease",
            Self::MutatingWebhookConfiguration => "mutating_webhook_configuration",
            Self::ValidatingWebhookConfiguration => "validating_webhook_configuration",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::HorizontalPodAutoscaler => "HorizontalPodAutoscalers",
            Self::PodDisruptionBudget => "PodDisruptionBudgets",
            Self::PriorityClass => "PriorityClasses",
            Self::RuntimeClass => "RuntimeClasses",
            Self::Lease => "Leases",
            Self::MutatingWebhookConfiguration => "MutatingWebhookConfigurations",
            Self::ValidatingWebhookConfiguration => "ValidatingWebhookConfigurations",
        }
    }

    fn is_namespaced(self) -> bool {
        matches!(
            self,
            Self::HorizontalPodAutoscaler | Self::PodDisruptionBudget | Self::Lease
        )
    }

    fn metadata(self) -> ResourceMetadata {
        ResourceMetadata {
            id: self.id().to_owned(),
            title: self.title().to_owned(),
            api_version: self.api_version().to_owned(),
            kind: self.singular_kind().to_owned(),
            resource: self.resource_ref(),
            namespaced: self.is_namespaced(),
        }
    }

    fn singular_kind(self) -> &'static str {
        match self {
            Self::HorizontalPodAutoscaler => "HorizontalPodAutoscaler",
            Self::PodDisruptionBudget => "PodDisruptionBudget",
            Self::PriorityClass => "PriorityClass",
            Self::RuntimeClass => "RuntimeClass",
            Self::Lease => "Lease",
            Self::MutatingWebhookConfiguration => "MutatingWebhookConfiguration",
            Self::ValidatingWebhookConfiguration => "ValidatingWebhookConfiguration",
        }
    }

    fn api_version(self) -> &'static str {
        match self {
            Self::HorizontalPodAutoscaler => "autoscaling/v2",
            Self::PodDisruptionBudget => "policy/v1",
            Self::PriorityClass => "scheduling.k8s.io/v1",
            Self::RuntimeClass => "node.k8s.io/v1",
            Self::Lease => "coordination.k8s.io/v1",
            Self::MutatingWebhookConfiguration | Self::ValidatingWebhookConfiguration => {
                "admissionregistration.k8s.io/v1"
            }
        }
    }

    fn resource_ref(self) -> ResourceRef {
        match self {
            Self::HorizontalPodAutoscaler => {
                ResourceRef::grouped("autoscaling", "v2", "horizontalpodautoscalers")
            }
            Self::PodDisruptionBudget => {
                ResourceRef::grouped("policy", "v1", "poddisruptionbudgets")
            }
            Self::PriorityClass => {
                ResourceRef::grouped("scheduling.k8s.io", "v1", "priorityclasses").cluster_scoped()
            }
            Self::RuntimeClass => {
                ResourceRef::grouped("node.k8s.io", "v1", "runtimeclasses").cluster_scoped()
            }
            Self::Lease => ResourceRef::grouped("coordination.k8s.io", "v1", "leases"),
            Self::MutatingWebhookConfiguration => ResourceRef::grouped(
                "admissionregistration.k8s.io",
                "v1",
                "mutatingwebhookconfigurations",
            )
            .cluster_scoped(),
            Self::ValidatingWebhookConfiguration => ResourceRef::grouped(
                "admissionregistration.k8s.io",
                "v1",
                "validatingwebhookconfigurations",
            )
            .cluster_scoped(),
        }
    }

    fn columns(self) -> &'static [&'static str] {
        match self {
            Self::HorizontalPodAutoscaler => &[
                "Name",
                "Namespace",
                "Reference",
                "Targets",
                "Min",
                "Max",
                "Replicas",
                "Age",
            ],
            Self::PodDisruptionBudget => &[
                "Name",
                "Namespace",
                "Min Available",
                "Max Unavailable",
                "Allowed",
                "Current Healthy",
                "Desired Healthy",
                "Age",
            ],
            Self::PriorityClass => &[
                "Name",
                "Value",
                "Global Default",
                "Preemption Policy",
                "Description",
                "Age",
            ],
            Self::RuntimeClass => &["Name", "Handler", "Overhead", "Scheduling", "Age"],
            Self::Lease => &[
                "Name",
                "Namespace",
                "Holder",
                "Acquire Time",
                "Renew Time",
                "Duration",
                "Transitions",
                "Age",
            ],
            Self::MutatingWebhookConfiguration | Self::ValidatingWebhookConfiguration => &[
                "Name",
                "Webhooks",
                "Failure Policy",
                "Side Effects",
                "Admission Review Versions",
                "Age",
            ],
        }
    }

    fn widths(self) -> &'static [f32] {
        match self {
            Self::HorizontalPodAutoscaler => &[240.0, 160.0, 220.0, 240.0, 80.0, 80.0, 120.0, 90.0],
            Self::PodDisruptionBudget => &[240.0, 160.0, 130.0, 150.0, 90.0, 130.0, 130.0, 90.0],
            Self::PriorityClass => &[260.0, 110.0, 130.0, 170.0, 360.0, 90.0],
            Self::RuntimeClass => &[260.0, 220.0, 180.0, 260.0, 90.0],
            Self::Lease => &[240.0, 160.0, 220.0, 220.0, 220.0, 100.0, 110.0, 90.0],
            Self::MutatingWebhookConfiguration | Self::ValidatingWebhookConfiguration => {
                &[300.0, 100.0, 180.0, 180.0, 260.0, 90.0]
            }
        }
    }

    fn load_kind(self, namespace: Option<String>) -> ResourceLoadKind {
        match self {
            Self::HorizontalPodAutoscaler => {
                ResourceLoadKind::HorizontalPodAutoscalers { namespace }
            }
            Self::PodDisruptionBudget => ResourceLoadKind::PodDisruptionBudgets { namespace },
            Self::PriorityClass => ResourceLoadKind::PriorityClasses,
            Self::RuntimeClass => ResourceLoadKind::RuntimeClasses,
            Self::Lease => ResourceLoadKind::Leases { namespace },
            Self::MutatingWebhookConfiguration => ResourceLoadKind::MutatingWebhookConfigurations,
            Self::ValidatingWebhookConfiguration => {
                ResourceLoadKind::ValidatingWebhookConfigurations
            }
        }
    }

    fn matches_load_kind(self, kind: &ResourceLoadKind) -> bool {
        matches!(
            (self, kind),
            (
                Self::HorizontalPodAutoscaler,
                ResourceLoadKind::HorizontalPodAutoscalers { .. }
            ) | (
                Self::PodDisruptionBudget,
                ResourceLoadKind::PodDisruptionBudgets { .. }
            ) | (Self::PriorityClass, ResourceLoadKind::PriorityClasses)
                | (Self::RuntimeClass, ResourceLoadKind::RuntimeClasses)
                | (Self::Lease, ResourceLoadKind::Leases { .. })
                | (
                    Self::MutatingWebhookConfiguration,
                    ResourceLoadKind::MutatingWebhookConfigurations
                )
                | (
                    Self::ValidatingWebhookConfiguration,
                    ResourceLoadKind::ValidatingWebhookConfigurations
                )
        )
    }

    fn rows_from_list(self, items: &[ResourceSummary]) -> Vec<ConfigRow> {
        let mut rows = items
            .iter()
            .map(|summary| row_from_summary(self, summary))
            .collect::<Vec<_>>();
        if self.is_namespaced() {
            rows.sort_by(|left, right| {
                left.namespace
                    .cmp(&right.namespace)
                    .then(left.name.cmp(&right.name))
            });
        } else {
            rows.sort_by(|left, right| left.name.cmp(&right.name));
        }
        rows
    }
}

impl ConfigRow {
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

fn show_config_table(
    ui: &mut egui::Ui,
    kind: ConfigResourceKind,
    rows: &[ConfigRow],
    row_indices: Vec<usize>,
    selected_rows: &mut BTreeSet<String>,
) -> Option<ConfigTableAction> {
    let row_height = ui.spacing().interact_size.y;
    let widths = kind.widths();
    let column_count = widths.len() + 1;
    let table_width = SELECT_COLUMN_WIDTH
        + widths.iter().sum::<f32>()
        + ui.spacing().item_spacing.x * column_count.saturating_sub(1) as f32;
    let mut action = None;
    egui::ScrollArea::horizontal()
        .id_salt((kind.id(), "table_horizontal"))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.set_min_width(table_width);
            let mut table = TableBuilder::new(ui)
                .id_salt((kind.id(), "table"))
                .striped(true)
                .resizable(false)
                .sense(egui::Sense::click())
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .min_scrolled_height(0.0);
            table = table.column(Column::exact(SELECT_COLUMN_WIDTH));
            for width in widths {
                table = table.column(Column::exact(*width));
            }
            table
                .header(row_height, |mut header| {
                    header.col(|_| {});
                    for label in kind.columns() {
                        header.col(|ui| {
                            ui.strong(*label);
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
                        for cell in &row.cells {
                            table_row.col(|ui| {
                                ui.label(cell);
                            });
                        }
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
                                action = Some(ConfigTableAction::Describe {
                                    key: row.key.clone(),
                                });
                                ui.close();
                            }
                            if ui
                                .button(format!("{} View", egui_phosphor::regular::EYE))
                                .clicked()
                            {
                                action = Some(ConfigTableAction::View {
                                    key: row.key.clone(),
                                });
                                ui.close();
                            }
                            if ui
                                .button(format!("{} Edit", egui_phosphor::regular::PENCIL_SIMPLE))
                                .clicked()
                            {
                                action = Some(ConfigTableAction::Edit {
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
                                action = Some(ConfigTableAction::Delete {
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

fn row_matches_search(row: &ConfigRow, search_text: &str) -> bool {
    let needle = search_text.trim().to_lowercase();
    needle.is_empty() || row.search_text.contains(&needle)
}

fn config_key(namespace: &str, name: &str) -> String {
    if namespace.is_empty() {
        name.to_owned()
    } else {
        format!("{namespace}/{name}")
    }
}

fn namespace_value(namespace: &str) -> Option<String> {
    if namespace.is_empty() || namespace == "N/A" {
        None
    } else {
        Some(namespace.to_owned())
    }
}

#[derive(Clone, Debug, PartialEq)]
struct ConfigRow {
    key: String,
    name: String,
    namespace: String,
    cells: Vec<String>,
    details: Vec<(String, String)>,
    raw: serde_json::Value,
    search_text: String,
}

#[derive(Clone, Debug, PartialEq)]
struct ConfigFilterCache {
    rows_version: u64,
    search_text: String,
    indices: Vec<usize>,
}

impl Default for ConfigFilterCache {
    fn default() -> Self {
        Self {
            rows_version: u64::MAX,
            search_text: String::new(),
            indices: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ConfigTableAction {
    Describe { key: String },
    View { key: String },
    Edit { key: String },
    Delete { key: String },
}

#[derive(Clone, Debug, PartialEq)]
struct ConfigDescribeDialog {
    key: String,
    name: String,
    describe: ConfigDescribe,
}

#[derive(Clone, Debug, PartialEq)]
struct ConfigViewDialog {
    key: String,
    name: String,
    yaml: String,
}

#[derive(Clone, Debug, PartialEq)]
struct ConfigDescribe {
    title: &'static str,
    summary: Vec<(String, String)>,
    labels: String,
    annotations: String,
    raw_yaml: String,
}

fn row_from_summary(kind: ConfigResourceKind, summary: &ResourceSummary) -> ConfigRow {
    let raw = &summary.raw;
    let name = value_str(raw, &["metadata", "name"])
        .unwrap_or(&summary.name)
        .to_owned();
    let namespace = value_str(raw, &["metadata", "namespace"])
        .or(summary.namespace.as_deref())
        .unwrap_or("N/A")
        .to_owned();
    let age = value_str(raw, &["metadata", "creationTimestamp"])
        .map(|timestamp| human_age_from_rfc3339(timestamp).unwrap_or_else(|| timestamp.to_owned()))
        .unwrap_or_else(|| "N/A".to_owned());
    let cells = match kind {
        ConfigResourceKind::HorizontalPodAutoscaler => hpa_cells(raw, &name, &namespace, &age),
        ConfigResourceKind::PodDisruptionBudget => pdb_cells(raw, &name, &namespace, &age),
        ConfigResourceKind::PriorityClass => priority_class_cells(raw, &name, &age),
        ConfigResourceKind::RuntimeClass => runtime_class_cells(raw, &name, &age),
        ConfigResourceKind::Lease => lease_cells(raw, &name, &namespace, &age),
        ConfigResourceKind::MutatingWebhookConfiguration
        | ConfigResourceKind::ValidatingWebhookConfiguration => webhook_cells(raw, &name, &age),
    };
    let details = kind
        .columns()
        .iter()
        .zip(cells.iter())
        .map(|(label, value)| ((*label).to_owned(), value.clone()))
        .collect::<Vec<_>>();
    let key = if kind.is_namespaced() {
        format!("{namespace}/{name}")
    } else {
        name.clone()
    };
    let search_text = cells.join(" ").to_lowercase();
    ConfigRow {
        key,
        name,
        namespace,
        cells,
        details,
        raw: summary.raw.clone(),
        search_text,
    }
}

fn hpa_cells(raw: &serde_json::Value, name: &str, namespace: &str, age: &str) -> Vec<String> {
    vec![
        name.to_owned(),
        namespace.to_owned(),
        scale_target_ref(raw),
        hpa_targets(raw),
        value_i64(raw, &["spec", "minReplicas"])
            .map_or_else(|| "N/A".to_owned(), |value| value.to_string()),
        value_i64(raw, &["spec", "maxReplicas"])
            .map_or_else(|| "N/A".to_owned(), |value| value.to_string()),
        hpa_replicas(raw),
        age.to_owned(),
    ]
}

fn pdb_cells(raw: &serde_json::Value, name: &str, namespace: &str, age: &str) -> Vec<String> {
    vec![
        name.to_owned(),
        namespace.to_owned(),
        int_or_string(raw.pointer("/spec/minAvailable")),
        int_or_string(raw.pointer("/spec/maxUnavailable")),
        value_i64(raw, &["status", "disruptionsAllowed"])
            .unwrap_or(0)
            .to_string(),
        value_i64(raw, &["status", "currentHealthy"])
            .unwrap_or(0)
            .to_string(),
        value_i64(raw, &["status", "desiredHealthy"])
            .unwrap_or(0)
            .to_string(),
        age.to_owned(),
    ]
}

fn priority_class_cells(raw: &serde_json::Value, name: &str, age: &str) -> Vec<String> {
    vec![
        name.to_owned(),
        value_i64(raw, &["value"]).map_or_else(|| "N/A".to_owned(), |value| value.to_string()),
        value_bool(raw, &["globalDefault"])
            .map_or_else(|| "N/A".to_owned(), |value| value.to_string()),
        value_str(raw, &["preemptionPolicy"])
            .unwrap_or("N/A")
            .to_owned(),
        value_str(raw, &["description"]).unwrap_or("N/A").to_owned(),
        age.to_owned(),
    ]
}

fn runtime_class_cells(raw: &serde_json::Value, name: &str, age: &str) -> Vec<String> {
    vec![
        name.to_owned(),
        value_str(raw, &["handler"]).unwrap_or("N/A").to_owned(),
        runtime_overhead(raw),
        runtime_scheduling(raw),
        age.to_owned(),
    ]
}

fn lease_cells(raw: &serde_json::Value, name: &str, namespace: &str, age: &str) -> Vec<String> {
    vec![
        name.to_owned(),
        namespace.to_owned(),
        value_str(raw, &["spec", "holderIdentity"])
            .unwrap_or("N/A")
            .to_owned(),
        value_str(raw, &["spec", "acquireTime"])
            .unwrap_or("N/A")
            .to_owned(),
        value_str(raw, &["spec", "renewTime"])
            .unwrap_or("N/A")
            .to_owned(),
        value_i64(raw, &["spec", "leaseDurationSeconds"])
            .map_or_else(|| "N/A".to_owned(), |value| format!("{value}s")),
        value_i64(raw, &["spec", "leaseTransitions"])
            .unwrap_or(0)
            .to_string(),
        age.to_owned(),
    ]
}

fn webhook_cells(raw: &serde_json::Value, name: &str, age: &str) -> Vec<String> {
    vec![
        name.to_owned(),
        array_len(raw.pointer("/webhooks")).to_string(),
        webhook_values(raw, "failurePolicy"),
        webhook_values(raw, "sideEffects"),
        webhook_admission_versions(raw),
        age.to_owned(),
    ]
}

fn scale_target_ref(raw: &serde_json::Value) -> String {
    let kind = value_str(raw, &["spec", "scaleTargetRef", "kind"]);
    let name = value_str(raw, &["spec", "scaleTargetRef", "name"]);
    match (kind, name) {
        (Some(kind), Some(name)) => format!("{kind}/{name}"),
        (_, Some(name)) => name.to_owned(),
        _ => "N/A".to_owned(),
    }
}

fn hpa_targets(raw: &serde_json::Value) -> String {
    let Some(metrics) = raw
        .pointer("/status/currentMetrics")
        .and_then(serde_json::Value::as_array)
    else {
        return "N/A".to_owned();
    };
    let values = metrics
        .iter()
        .map(|metric| {
            let metric_type = value_str(metric, &["type"]).unwrap_or("Metric");
            let current = metric
                .pointer("/resource/current/averageUtilization")
                .or_else(|| metric.pointer("/resource/current/averageValue"))
                .or_else(|| metric.pointer("/pods/current/averageValue"))
                .or_else(|| metric.pointer("/object/current/value"))
                .or_else(|| metric.pointer("/external/current/value"))
                .map(value_to_string)
                .unwrap_or_else(|| "N/A".to_owned());
            format!("{metric_type}: {current}")
        })
        .collect::<Vec<_>>();
    if values.is_empty() {
        "N/A".to_owned()
    } else {
        values.join(", ")
    }
}

fn hpa_replicas(raw: &serde_json::Value) -> String {
    let current = value_i64(raw, &["status", "currentReplicas"]).unwrap_or(0);
    let desired = value_i64(raw, &["status", "desiredReplicas"]).unwrap_or(0);
    format!("{current}/{desired}")
}

fn int_or_string(value: Option<&serde_json::Value>) -> String {
    value.map_or_else(|| "N/A".to_owned(), value_to_string)
}

fn runtime_overhead(raw: &serde_json::Value) -> String {
    resource_map(raw.pointer("/overhead/podFixed")).unwrap_or_else(|| "N/A".to_owned())
}

fn runtime_scheduling(raw: &serde_json::Value) -> String {
    let selector = resource_map(raw.pointer("/scheduling/nodeSelector"));
    let tolerations = array_len(raw.pointer("/scheduling/tolerations"));
    match (selector, tolerations) {
        (Some(selector), 0) => selector,
        (Some(selector), count) => format!("{selector}; tolerations={count}"),
        (None, count) if count > 0 => format!("tolerations={count}"),
        _ => "N/A".to_owned(),
    }
}

fn webhook_values(raw: &serde_json::Value, field: &str) -> String {
    let Some(webhooks) = raw
        .pointer("/webhooks")
        .and_then(serde_json::Value::as_array)
    else {
        return "N/A".to_owned();
    };
    let mut values = webhooks
        .iter()
        .filter_map(|webhook| webhook.get(field))
        .filter_map(serde_json::Value::as_str)
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    values.sort();
    values.dedup();
    if values.is_empty() {
        "N/A".to_owned()
    } else {
        values.join(", ")
    }
}

fn webhook_admission_versions(raw: &serde_json::Value) -> String {
    let Some(webhooks) = raw
        .pointer("/webhooks")
        .and_then(serde_json::Value::as_array)
    else {
        return "N/A".to_owned();
    };
    let mut values = webhooks
        .iter()
        .filter_map(|webhook| webhook.get("admissionReviewVersions"))
        .filter_map(serde_json::Value::as_array)
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    values.sort();
    values.dedup();
    if values.is_empty() {
        "N/A".to_owned()
    } else {
        values.join(", ")
    }
}

fn describe_from_row(kind: ConfigResourceKind, row: &ConfigRow) -> ConfigDescribe {
    ConfigDescribe {
        title: kind.title(),
        summary: row.details.clone(),
        labels: resource_map(row.raw.pointer("/metadata/labels"))
            .unwrap_or_else(|| "N/A".to_owned()),
        annotations: resource_map(row.raw.pointer("/metadata/annotations"))
            .unwrap_or_else(|| "N/A".to_owned()),
        raw_yaml: full_manifest_yaml(&row.raw),
    }
}

fn show_config_describe(ui: &mut egui::Ui, describe: &ConfigDescribe) {
    ui.heading(describe.title);
    ui.separator();
    egui::Grid::new("config_describe_summary")
        .num_columns(2)
        .spacing([16.0, 4.0])
        .show(ui, |ui| {
            for (label, value) in &describe.summary {
                ui.weak(label);
                ui.label(value);
                ui.end_row();
            }
        });
    ui.add_space(10.0);
    describe_block(ui, "Labels", &describe.labels);
    describe_block(ui, "Annotations", &describe.annotations);
    describe_block(ui, "Raw manifest", &describe.raw_yaml);
}

fn describe_block(ui: &mut egui::Ui, title: &str, value: &str) {
    ui.strong(title);
    ui.add(
        egui::Label::new(egui::RichText::new(value).monospace())
            .wrap_mode(TextWrapMode::Extend)
            .selectable(true),
    );
    ui.add_space(8.0);
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

fn array_len(value: Option<&serde_json::Value>) -> usize {
    value
        .and_then(serde_json::Value::as_array)
        .map_or(0, Vec::len)
}

fn value_to_string(value: &serde_json::Value) -> String {
    value
        .as_str()
        .map_or_else(|| value.to_string(), ToOwned::to_owned)
}

fn value_str<'a>(value: &'a serde_json::Value, path: &[&str]) -> Option<&'a str> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    current.as_str()
}

fn value_i64(value: &serde_json::Value, path: &[&str]) -> Option<i64> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    current.as_i64()
}

fn value_bool(value: &serde_json::Value, path: &[&str]) -> Option<bool> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    current.as_bool()
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
    fn namespaced_config_requests_use_selected_namespace() {
        for kind in [
            ConfigResourceKind::HorizontalPodAutoscaler,
            ConfigResourceKind::PodDisruptionBudget,
            ConfigResourceKind::Lease,
        ] {
            let mut panel = ConfigResourcePanel::new(kind);
            panel.namespace_filter = Some("production".to_owned());
            let query = panel.request_resources(ClusterId::new("local")).query();
            assert_eq!(query.namespace.as_deref(), Some("production"));
        }
    }

    #[test]
    fn cluster_config_requests_are_cluster_scoped() {
        for kind in [
            ConfigResourceKind::PriorityClass,
            ConfigResourceKind::RuntimeClass,
            ConfigResourceKind::MutatingWebhookConfiguration,
            ConfigResourceKind::ValidatingWebhookConfiguration,
        ] {
            let mut panel = ConfigResourcePanel::new(kind);
            let query = panel.request_resources(ClusterId::new("local")).query();
            assert_eq!(query.namespace, None);
            assert!(matches!(
                query.resource.scope,
                miku_core::ResourceScope::Cluster
            ));
        }
    }

    #[test]
    fn config_rows_extract_fields() {
        let hpa = row_from_summary(ConfigResourceKind::HorizontalPodAutoscaler, &hpa_summary());
        assert_eq!(hpa.cells[2], "Deployment/api");
        assert_eq!(hpa.cells[3], "Resource: 70");
        assert_eq!(hpa.cells[4], "2");
        assert_eq!(hpa.cells[5], "10");
        assert_eq!(hpa.cells[6], "3/5");

        let pdb = row_from_summary(ConfigResourceKind::PodDisruptionBudget, &pdb_summary());
        assert_eq!(pdb.cells[2], "50%");
        assert_eq!(pdb.cells[3], "1");
        assert_eq!(pdb.cells[4], "2");
        assert_eq!(pdb.cells[5], "4");
        assert_eq!(pdb.cells[6], "3");

        let priority = row_from_summary(ConfigResourceKind::PriorityClass, &priority_summary());
        assert_eq!(priority.cells[1], "100000");
        assert_eq!(priority.cells[2], "false");
        assert_eq!(priority.cells[3], "PreemptLowerPriority");

        let runtime = row_from_summary(ConfigResourceKind::RuntimeClass, &runtime_summary());
        assert_eq!(runtime.cells[1], "runc");
        assert_eq!(runtime.cells[2], "cpu=100m");
        assert_eq!(runtime.cells[3], "disk=ssd; tolerations=1");

        let lease = row_from_summary(ConfigResourceKind::Lease, &lease_summary());
        assert_eq!(lease.cells[2], "controller");
        assert_eq!(lease.cells[5], "15s");
        assert_eq!(lease.cells[6], "4");

        let webhook = row_from_summary(
            ConfigResourceKind::MutatingWebhookConfiguration,
            &webhook_summary("MutatingWebhookConfiguration"),
        );
        assert_eq!(webhook.cells[1], "1");
        assert_eq!(webhook.cells[2], "Fail");
        assert_eq!(webhook.cells[3], "None");
        assert_eq!(webhook.cells[4], "v1");
    }

    #[test]
    fn config_rows_handle_missing_fields() {
        let hpa = row_from_summary(
            ConfigResourceKind::HorizontalPodAutoscaler,
            &minimal_summary("HorizontalPodAutoscaler"),
        );
        assert_eq!(hpa.cells[2], "N/A");
        assert_eq!(hpa.cells[3], "N/A");
        assert_eq!(hpa.cells[4], "N/A");
        assert_eq!(hpa.cells[5], "N/A");
        assert_eq!(hpa.cells[6], "0/0");

        let pdb = row_from_summary(
            ConfigResourceKind::PodDisruptionBudget,
            &minimal_summary("PodDisruptionBudget"),
        );
        assert_eq!(pdb.cells[2], "N/A");
        assert_eq!(pdb.cells[4], "0");

        let webhook = row_from_summary(
            ConfigResourceKind::ValidatingWebhookConfiguration,
            &minimal_summary("ValidatingWebhookConfiguration"),
        );
        assert_eq!(webhook.cells[1], "0");
        assert_eq!(webhook.cells[2], "N/A");
    }

    #[test]
    fn config_rows_sort_and_filter_case_insensitively() {
        let rows = ConfigResourceKind::Lease.rows_from_list(&[
            lease_summary_with_name("zeta", "worker"),
            lease_summary_with_name("default", "api-b"),
            lease_summary_with_name("default", "api-a"),
        ]);
        let keys = rows.iter().map(|row| row.key.as_str()).collect::<Vec<_>>();
        assert_eq!(keys, vec!["default/api-a", "default/api-b", "zeta/worker"]);
        assert_eq!(
            rows.iter()
                .filter(|row| row_matches_search(row, "CONTROLLER"))
                .count(),
            3
        );
        assert_eq!(
            rows.iter()
                .filter(|row| row_matches_search(row, "ZETA"))
                .count(),
            1
        );
    }

    #[test]
    fn stale_watch_events_do_not_replace_current_rows() {
        let mut panel = ConfigResourcePanel::new(ConfigResourceKind::Lease);
        let cluster_id = ClusterId::new("local");
        let first = panel.request_resource_watch(cluster_id.clone());
        let second = panel.request_resource_watch(cluster_id);
        panel.apply_event(ResourceUiEvent::ResourceWatchUpdated {
            request: first,
            result: Ok(miku_api::ResourceEvent::Snapshot(ResourceList {
                items: vec![lease_summary_with_name("default", "stale")],
                continue_token: None,
            })),
        });
        assert!(panel.rows.is_empty());
        panel.apply_event(ResourceUiEvent::ResourceWatchUpdated {
            request: second,
            result: Ok(miku_api::ResourceEvent::Snapshot(ResourceList {
                items: vec![lease_summary()],
                continue_token: None,
            })),
        });
        assert_eq!(panel.rows[0].name, "kube-scheduler");
    }

    #[test]
    fn namespace_watch_events_update_namespaced_selectors() {
        let mut panel = ConfigResourcePanel::new(ConfigResourceKind::HorizontalPodAutoscaler);
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
        let mut panel = ConfigResourcePanel::new(ConfigResourceKind::Lease);
        let row = row_from_summary(
            ConfigResourceKind::Lease,
            &with_server_fields(lease_summary()),
        );
        let key = row.key.clone();
        panel.rows = vec![row];

        panel.apply_table_action(Some(ConfigTableAction::Edit { key }));

        let dialog = panel.edit_dialog.as_ref().unwrap();
        assert_eq!(dialog.target.namespace.as_deref(), Some("kube-system"));
        assert_eq!(dialog.target.name, "kube-scheduler");
        let manifest = serde_yaml::from_str::<serde_json::Value>(&dialog.yaml).unwrap();
        assert!(manifest.pointer("/metadata/creationTimestamp").is_none());
        assert!(manifest.pointer("/metadata/resourceVersion").is_none());
        assert!(manifest.pointer("/metadata/managedFields").is_none());
        assert!(manifest.pointer("/status").is_none());
    }

    #[test]
    fn delete_action_opens_delete_dialog_for_namespaced_and_cluster_resources() {
        let mut namespaced_panel = ConfigResourcePanel::new(ConfigResourceKind::Lease);
        let namespaced_row = row_from_summary(ConfigResourceKind::Lease, &lease_summary());
        let namespaced_key = namespaced_row.key.clone();
        namespaced_panel.rows = vec![namespaced_row];

        namespaced_panel.apply_table_action(Some(ConfigTableAction::Delete {
            key: namespaced_key,
        }));

        let dialog = namespaced_panel.delete_dialog.as_ref().unwrap();
        assert_eq!(dialog.target.namespace.as_deref(), Some("kube-system"));
        assert_eq!(dialog.target.name, "kube-scheduler");

        let mut cluster_panel = ConfigResourcePanel::new(ConfigResourceKind::PriorityClass);
        let cluster_row = row_from_summary(ConfigResourceKind::PriorityClass, &priority_summary());
        let cluster_key = cluster_row.key.clone();
        cluster_panel.rows = vec![cluster_row];

        cluster_panel.apply_table_action(Some(ConfigTableAction::Delete { key: cluster_key }));

        let dialog = cluster_panel.delete_dialog.as_ref().unwrap();
        assert_eq!(dialog.target.namespace, None);
        assert_eq!(dialog.target.name, "high");
    }

    #[test]
    fn apply_completion_closes_edit_dialog_and_upserts_sorted_row() {
        let mut panel = ConfigResourcePanel::new(ConfigResourceKind::Lease);
        let row = row_from_summary(ConfigResourceKind::Lease, &lease_summary());
        panel.rows = ConfigResourceKind::Lease.rows_from_list(&[
            lease_summary_with_name("zeta", "worker"),
            lease_summary_with_name("default", "api-b"),
        ]);
        panel.edit_dialog = Some(GenericEditDialog {
            target: row.target(),
            yaml: "kind: Lease".to_owned(),
            parse_error: None,
        });
        panel.action_request_id = Some(7);
        panel.action_error = Some("old error".to_owned());

        panel.apply_event(ResourceUiEvent::ResourceActionCompleted {
            request: super::super::ResourceActionRequest {
                request_id: 7,
                cluster_id: ClusterId::new("local"),
                kind: ResourceActionKind::ApplyResource {
                    resource: ConfigResourceKind::Lease.metadata().resource,
                    namespace: Some("default".to_owned()),
                    name: "api-a".to_owned(),
                    manifest: serde_json::json!({}),
                },
            },
            result: Ok(ResourceActionOutcome::Applied(lease_summary_with_name(
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
        let mut panel = ConfigResourcePanel::new(ConfigResourceKind::Lease);
        let row = row_from_summary(ConfigResourceKind::Lease, &lease_summary());
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
                    resource: ConfigResourceKind::Lease.metadata().resource,
                    namespace: Some("kube-system".to_owned()),
                    name: "kube-scheduler".to_owned(),
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
        let row = row_from_summary(ConfigResourceKind::Lease, &lease_summary());
        let mut edit_panel = ConfigResourcePanel::new(ConfigResourceKind::Lease);
        edit_panel.edit_dialog = Some(GenericEditDialog {
            target: row.target(),
            yaml: "kind: Lease".to_owned(),
            parse_error: None,
        });
        edit_panel.action_request_id = Some(7);

        edit_panel.apply_event(ResourceUiEvent::ResourceActionCompleted {
            request: super::super::ResourceActionRequest {
                request_id: 7,
                cluster_id: ClusterId::new("local"),
                kind: ResourceActionKind::ApplyResource {
                    resource: ConfigResourceKind::Lease.metadata().resource,
                    namespace: Some("kube-system".to_owned()),
                    name: "kube-scheduler".to_owned(),
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

        let mut delete_panel = ConfigResourcePanel::new(ConfigResourceKind::Lease);
        delete_panel.delete_dialog = Some(GenericDeleteDialog {
            target: row.delete_target(),
        });
        delete_panel.action_request_id = Some(9);

        delete_panel.apply_event(ResourceUiEvent::ResourceActionCompleted {
            request: super::super::ResourceActionRequest {
                request_id: 9,
                cluster_id: ClusterId::new("local"),
                kind: ResourceActionKind::DeleteResource {
                    resource: ConfigResourceKind::Lease.metadata().resource,
                    namespace: Some("kube-system".to_owned()),
                    name: "kube-scheduler".to_owned(),
                },
            },
            result: Err("delete denied".to_owned()),
        });

        assert!(delete_panel.delete_dialog.is_some());
        assert_eq!(delete_panel.action_error.as_deref(), Some("delete denied"));
    }

    #[test]
    fn cluster_change_clears_edit_delete_batch_and_pending_action() {
        let row = row_from_summary(ConfigResourceKind::Lease, &lease_summary());
        let mut panel = ConfigResourcePanel::new(ConfigResourceKind::Lease);
        panel.last_cluster_id = Some(ClusterId::new("old"));
        panel.edit_dialog = Some(GenericEditDialog {
            target: row.target(),
            yaml: "kind: Lease".to_owned(),
            parse_error: None,
        });
        panel.delete_dialog = Some(GenericDeleteDialog {
            target: row.delete_target(),
        });
        panel.batch_delete_dialog = Some(GenericBatchDeleteDialog {
            targets: vec![row.delete_target()],
        });
        panel.action_request_id = Some(7);
        panel.action_error = Some("old error".to_owned());

        panel.reset_for_cluster_change(&ClusterId::new("new"));

        assert!(panel.edit_dialog.is_none());
        assert!(panel.delete_dialog.is_none());
        assert!(panel.batch_delete_dialog.is_none());
        assert_eq!(panel.action_request_id, None);
        assert_eq!(panel.action_error, None);
    }

    fn hpa_summary() -> ResourceSummary {
        ResourceSummary {
            name: "api".to_owned(),
            namespace: Some("default".to_owned()),
            kind: "HorizontalPodAutoscaler".to_owned(),
            status: None,
            raw: serde_json::json!({
                "metadata": {"name": "api", "namespace": "default", "creationTimestamp": "2026-05-18T10:00:00Z"},
                "spec": {"scaleTargetRef": {"kind": "Deployment", "name": "api"}, "minReplicas": 2, "maxReplicas": 10},
                "status": {
                    "currentReplicas": 3,
                    "desiredReplicas": 5,
                    "currentMetrics": [{"type": "Resource", "resource": {"current": {"averageUtilization": 70}}}]
                }
            }),
        }
    }

    fn pdb_summary() -> ResourceSummary {
        ResourceSummary {
            name: "api".to_owned(),
            namespace: Some("default".to_owned()),
            kind: "PodDisruptionBudget".to_owned(),
            status: None,
            raw: serde_json::json!({
                "metadata": {"name": "api", "namespace": "default", "creationTimestamp": "2026-05-18T10:00:00Z"},
                "spec": {"minAvailable": "50%", "maxUnavailable": 1},
                "status": {"disruptionsAllowed": 2, "currentHealthy": 4, "desiredHealthy": 3}
            }),
        }
    }

    fn priority_summary() -> ResourceSummary {
        ResourceSummary {
            name: "high".to_owned(),
            namespace: None,
            kind: "PriorityClass".to_owned(),
            status: None,
            raw: serde_json::json!({
                "metadata": {"name": "high", "creationTimestamp": "2026-05-18T10:00:00Z"},
                "value": 100000,
                "globalDefault": false,
                "preemptionPolicy": "PreemptLowerPriority",
                "description": "high priority"
            }),
        }
    }

    fn runtime_summary() -> ResourceSummary {
        ResourceSummary {
            name: "runc".to_owned(),
            namespace: None,
            kind: "RuntimeClass".to_owned(),
            status: None,
            raw: serde_json::json!({
                "metadata": {"name": "runc", "creationTimestamp": "2026-05-18T10:00:00Z"},
                "handler": "runc",
                "overhead": {"podFixed": {"cpu": "100m"}},
                "scheduling": {"nodeSelector": {"disk": "ssd"}, "tolerations": [{"key": "runtime"}]}
            }),
        }
    }

    fn lease_summary() -> ResourceSummary {
        lease_summary_with_name("kube-system", "kube-scheduler")
    }

    fn lease_summary_with_name(namespace: &str, name: &str) -> ResourceSummary {
        ResourceSummary {
            name: name.to_owned(),
            namespace: Some(namespace.to_owned()),
            kind: "Lease".to_owned(),
            status: None,
            raw: serde_json::json!({
                "metadata": {"name": name, "namespace": namespace, "creationTimestamp": "2026-05-18T10:00:00Z"},
                "spec": {
                    "holderIdentity": "controller",
                    "acquireTime": "2026-05-18T10:01:00Z",
                    "renewTime": "2026-05-18T10:02:00Z",
                    "leaseDurationSeconds": 15,
                    "leaseTransitions": 4
                }
            }),
        }
    }

    fn webhook_summary(kind: &str) -> ResourceSummary {
        ResourceSummary {
            name: "policy".to_owned(),
            namespace: None,
            kind: kind.to_owned(),
            status: None,
            raw: serde_json::json!({
                "metadata": {"name": "policy", "creationTimestamp": "2026-05-18T10:00:00Z"},
                "webhooks": [{"name": "policy.example.com", "failurePolicy": "Fail", "sideEffects": "None", "admissionReviewVersions": ["v1"]}]
            }),
        }
    }

    fn minimal_summary(kind: &str) -> ResourceSummary {
        ResourceSummary {
            name: "minimal".to_owned(),
            namespace: Some("default".to_owned()),
            kind: kind.to_owned(),
            status: None,
            raw: serde_json::json!({"metadata": {"name": "minimal", "namespace": "default"}}),
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
