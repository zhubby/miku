use std::collections::BTreeSet;

use eframe::egui;
use egui_extras::{Column, TableBuilder};
use miku_api::ResourceSummary;
use miku_core::{ClusterId, ResourceRef};

#[cfg(test)]
use super::ResourceLoadRequest;
use super::components::ResourceYamlViewDialog;
use super::components::{
    DescribeField, GenericBatchDeleteDialog, GenericCreateDialog, GenericDeleteDialog,
    GenericEditDialog, ResourceBatchDeleteDialogInput, ResourceCreateDialogInput,
    ResourceCreateDialogResponse, ResourceDeleteDialogInput, ResourceDeleteDialogResponse,
    ResourceEditDialogInput, ResourceEditDialogResponse, ResourceMapEntry, ResourceMetadata,
    ResourceRowTarget, ResourceToolbar, SELECT_COLUMN_WIDTH, apply_resource_request,
    batch_delete_resource_request, default_resource_yaml, delete_resource_request, describe_fields,
    describe_group, describe_metadata_maps, describe_raw_manifest, edit_resource_request,
    editable_resource_yaml, resource_map_entries, selected_delete_targets,
    show_resource_batch_delete_dialog, show_resource_create_dialog, show_resource_delete_dialog,
    show_resource_describe_window, show_resource_edit_dialog, show_row_selection_checkbox,
    visible_keys,
};
use super::{
    LoadStatus, ResourceActionKind, ResourceActionOutcome, ResourceLoadKind, ResourcePanelRequests,
    ResourceUiEvent, ResourceWatchRequest, namespaces_from_list,
};
use crate::time::human_age_from_rfc3339;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StorageResourceKind {
    PersistentVolumeClaim,
    PersistentVolume,
    StorageClass,
}

#[derive(Clone, Debug)]
pub(crate) struct StorageResourcePanel {
    kind: StorageResourceKind,
    namespace_filter: Option<String>,
    search_text: String,
    namespaces: Vec<String>,
    namespace_status: LoadStatus,
    row_status: LoadStatus,
    rows: Vec<StorageRow>,
    selected_rows: BTreeSet<String>,
    next_request_id: u64,
    namespace_request_id: Option<u64>,
    row_request_id: Option<u64>,
    namespace_watch_request_id: Option<u64>,
    row_watch_request_id: Option<u64>,
    last_cluster_id: Option<ClusterId>,
    describe_dialog: Option<StorageDescribeDialog>,
    view_dialog: Option<StorageViewDialog>,
    edit_dialog: Option<GenericEditDialog>,
    create_dialog: Option<GenericCreateDialog>,
    batch_delete_dialog: Option<GenericBatchDeleteDialog>,
    delete_dialog: Option<GenericDeleteDialog>,
    action_request_id: Option<u64>,
    action_error: Option<String>,
}

impl StorageResourcePanel {
    pub(crate) fn new(kind: StorageResourceKind) -> Self {
        Self {
            kind,
            namespace_filter: None,
            search_text: String::new(),
            namespaces: Vec::new(),
            namespace_status: LoadStatus::Idle,
            row_status: LoadStatus::Idle,
            rows: Vec::new(),
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
                            let key = storage_key(namespace.as_deref().unwrap_or(""), &name);
                            self.rows.retain(|row| row.key != key);
                            self.selected_rows.remove(&key);
                        }
                        self.delete_dialog = None;
                        self.action_error = None;
                    }
                    Ok(ResourceActionOutcome::BatchDeleted(targets)) => {
                        for target in targets {
                            let key = storage_key(
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
                    show_storage_table(ui, self.kind, &self.rows, indices, &mut self.selected_rows);
                self.apply_table_action(action);
            }
        }
    }

    fn apply_table_action(&mut self, action: Option<StorageTableAction>) {
        match action {
            Some(StorageTableAction::Describe { key }) => {
                let Some((name, describe)) = self
                    .row_by_key(&key)
                    .map(|row| (row.name.clone(), describe_from_row(self.kind, row)))
                else {
                    return;
                };
                self.describe_dialog = Some(StorageDescribeDialog {
                    key,
                    name,
                    describe,
                });
            }
            Some(StorageTableAction::View { key }) => {
                let Some((name, yaml)) = self
                    .row_by_key(&key)
                    .map(|row| (row.name.clone(), full_manifest_yaml(&row.raw)))
                else {
                    return;
                };
                self.view_dialog = Some(StorageViewDialog { key, name, yaml });
            }
            Some(StorageTableAction::Edit { key }) => {
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
            Some(StorageTableAction::Delete { key }) => {
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
            egui::Id::new((self.kind.id(), "describe", &dialog.key)),
            format!("Describe {}", dialog.name),
            &mut open,
            |ui| {
                show_storage_describe(ui, &dialog.describe);
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

    fn row_by_key(&self, key: &str) -> Option<&StorageRow> {
        self.rows.iter().find(|row| row.key == key)
    }

    fn replace_rows(&mut self, rows: Vec<StorageRow>) {
        let targets = rows.iter().map(StorageRow::target).collect::<Vec<_>>();
        let visible_keys = visible_keys(&targets);
        self.selected_rows.retain(|key| visible_keys.contains(key));
        self.rows = rows;
    }

    fn upsert_row(&mut self, row: StorageRow) {
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
    }

    fn prune_selection_to_visible(&mut self) {
        let targets = self
            .filtered_row_indices()
            .into_iter()
            .filter_map(|index| self.rows.get(index))
            .map(StorageRow::target)
            .collect::<Vec<_>>();
        let visible_keys = visible_keys(&targets);
        self.selected_rows.retain(|key| visible_keys.contains(key));
    }

    fn selected_delete_targets(&self) -> Vec<super::ResourceDeleteTarget> {
        let targets = self.rows.iter().map(StorageRow::target).collect::<Vec<_>>();
        selected_delete_targets(&targets, &self.selected_rows)
    }
}

impl StorageResourceKind {
    fn id(self) -> &'static str {
        match self {
            Self::PersistentVolumeClaim => "persistent_volume_claim",
            Self::PersistentVolume => "persistent_volume",
            Self::StorageClass => "storage_class",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::PersistentVolumeClaim => "PersistentVolumeClaims",
            Self::PersistentVolume => "PersistentVolumes",
            Self::StorageClass => "StorageClasses",
        }
    }

    fn is_namespaced(self) -> bool {
        matches!(self, Self::PersistentVolumeClaim)
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
            Self::PersistentVolumeClaim => "PersistentVolumeClaim",
            Self::PersistentVolume => "PersistentVolume",
            Self::StorageClass => "StorageClass",
        }
    }

    fn api_version(self) -> &'static str {
        match self {
            Self::PersistentVolumeClaim | Self::PersistentVolume => "v1",
            Self::StorageClass => "storage.k8s.io/v1",
        }
    }

    fn resource_ref(self) -> ResourceRef {
        match self {
            Self::PersistentVolumeClaim => ResourceRef::core("v1", "persistentvolumeclaims"),
            Self::PersistentVolume => ResourceRef::core("v1", "persistentvolumes").cluster_scoped(),
            Self::StorageClass => {
                ResourceRef::grouped("storage.k8s.io", "v1", "storageclasses").cluster_scoped()
            }
        }
    }

    fn columns(self) -> &'static [&'static str] {
        match self {
            Self::PersistentVolumeClaim => &[
                "Name",
                "Namespace",
                "Status",
                "Volume",
                "Class",
                "Capacity",
                "Access Modes",
                "Age",
            ],
            Self::PersistentVolume => &[
                "Name",
                "Status",
                "Claim",
                "Class",
                "Capacity",
                "Access Modes",
                "Reclaim Policy",
                "Age",
            ],
            Self::StorageClass => &[
                "Name",
                "Provisioner",
                "Reclaim Policy",
                "Binding Mode",
                "Expansion",
                "Default",
                "Age",
            ],
        }
    }

    fn widths(self) -> &'static [f32] {
        match self {
            Self::PersistentVolumeClaim => &[240.0, 160.0, 120.0, 220.0, 180.0, 120.0, 180.0, 90.0],
            Self::PersistentVolume => &[260.0, 120.0, 240.0, 180.0, 120.0, 180.0, 140.0, 90.0],
            Self::StorageClass => &[240.0, 300.0, 140.0, 160.0, 110.0, 90.0, 90.0],
        }
    }

    fn load_kind(self, namespace: Option<String>) -> ResourceLoadKind {
        match self {
            Self::PersistentVolumeClaim => ResourceLoadKind::PersistentVolumeClaims { namespace },
            Self::PersistentVolume => ResourceLoadKind::PersistentVolumes,
            Self::StorageClass => ResourceLoadKind::StorageClasses,
        }
    }

    fn matches_load_kind(self, kind: &ResourceLoadKind) -> bool {
        matches!(
            (self, kind),
            (
                Self::PersistentVolumeClaim,
                ResourceLoadKind::PersistentVolumeClaims { .. }
            ) | (Self::PersistentVolume, ResourceLoadKind::PersistentVolumes)
                | (Self::StorageClass, ResourceLoadKind::StorageClasses)
        )
    }

    fn rows_from_list(self, items: &[ResourceSummary]) -> Vec<StorageRow> {
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

fn show_storage_table(
    ui: &mut egui::Ui,
    kind: StorageResourceKind,
    rows: &[StorageRow],
    row_indices: Vec<usize>,
    selected_rows: &mut BTreeSet<String>,
) -> Option<StorageTableAction> {
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
                            crate::clipboard::copy_name_menu_item(ui, &row.name);
                            ui.separator();
                            if ui
                                .button(format!("{} Describe", egui_phosphor::regular::INFO))
                                .clicked()
                            {
                                action = Some(StorageTableAction::Describe {
                                    key: row.key.clone(),
                                });
                                ui.close();
                            }
                            if ui
                                .button(format!("{} View", egui_phosphor::regular::EYE))
                                .clicked()
                            {
                                action = Some(StorageTableAction::View {
                                    key: row.key.clone(),
                                });
                                ui.close();
                            }
                            if ui
                                .button(format!("{} Edit", egui_phosphor::regular::PENCIL_SIMPLE))
                                .clicked()
                            {
                                action = Some(StorageTableAction::Edit {
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
                                action = Some(StorageTableAction::Delete {
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

fn row_matches_search(row: &StorageRow, search_text: &str) -> bool {
    let needle = search_text.trim().to_lowercase();
    needle.is_empty() || row.search_text.contains(&needle)
}

fn storage_key(namespace: &str, name: &str) -> String {
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
struct StorageRow {
    key: String,
    name: String,
    namespace: String,
    cells: Vec<String>,
    details: Vec<(String, String)>,
    search_text: String,
    raw: serde_json::Value,
}

impl StorageRow {
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
enum StorageTableAction {
    Describe { key: String },
    View { key: String },
    Edit { key: String },
    Delete { key: String },
}

#[derive(Clone, Debug, PartialEq)]
struct StorageDescribeDialog {
    key: String,
    name: String,
    describe: StorageDescribe,
}

#[derive(Clone, Debug, PartialEq)]
struct StorageViewDialog {
    key: String,
    name: String,
    yaml: String,
}

#[derive(Clone, Debug, PartialEq)]
struct StorageDescribe {
    title: &'static str,
    summary: Vec<DescribeField>,
    labels: Vec<ResourceMapEntry>,
    annotations: Vec<ResourceMapEntry>,
    raw_yaml: String,
}

fn row_from_summary(kind: StorageResourceKind, summary: &ResourceSummary) -> StorageRow {
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
        StorageResourceKind::PersistentVolumeClaim => pvc_cells(raw, &name, &namespace, &age),
        StorageResourceKind::PersistentVolume => pv_cells(raw, &name, &age),
        StorageResourceKind::StorageClass => storage_class_cells(raw, &name, &age),
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
    StorageRow {
        key,
        name,
        namespace,
        cells,
        details,
        search_text,
        raw: summary.raw.clone(),
    }
}

fn pvc_cells(raw: &serde_json::Value, name: &str, namespace: &str, age: &str) -> Vec<String> {
    vec![
        name.to_owned(),
        namespace.to_owned(),
        value_str(raw, &["status", "phase"])
            .unwrap_or("N/A")
            .to_owned(),
        value_str(raw, &["spec", "volumeName"])
            .unwrap_or("N/A")
            .to_owned(),
        value_str(raw, &["spec", "storageClassName"])
            .unwrap_or("N/A")
            .to_owned(),
        value_str(raw, &["status", "capacity", "storage"])
            .or_else(|| value_str(raw, &["spec", "resources", "requests", "storage"]))
            .unwrap_or("N/A")
            .to_owned(),
        string_array(raw.pointer("/spec/accessModes")),
        age.to_owned(),
    ]
}

fn pv_cells(raw: &serde_json::Value, name: &str, age: &str) -> Vec<String> {
    vec![
        name.to_owned(),
        value_str(raw, &["status", "phase"])
            .unwrap_or("N/A")
            .to_owned(),
        claim_ref(raw),
        value_str(raw, &["spec", "storageClassName"])
            .unwrap_or("N/A")
            .to_owned(),
        value_str(raw, &["spec", "capacity", "storage"])
            .unwrap_or("N/A")
            .to_owned(),
        string_array(raw.pointer("/spec/accessModes")),
        value_str(raw, &["spec", "persistentVolumeReclaimPolicy"])
            .unwrap_or("N/A")
            .to_owned(),
        age.to_owned(),
    ]
}

fn storage_class_cells(raw: &serde_json::Value, name: &str, age: &str) -> Vec<String> {
    vec![
        name.to_owned(),
        value_str(raw, &["provisioner"]).unwrap_or("N/A").to_owned(),
        value_str(raw, &["reclaimPolicy"])
            .unwrap_or("N/A")
            .to_owned(),
        value_str(raw, &["volumeBindingMode"])
            .unwrap_or("N/A")
            .to_owned(),
        value_bool(raw, &["allowVolumeExpansion"])
            .map_or_else(|| "N/A".to_owned(), |value| value.to_string()),
        value_str(
            raw,
            &[
                "metadata",
                "annotations",
                "storageclass.kubernetes.io/is-default-class",
            ],
        )
        .or_else(|| {
            value_str(
                raw,
                &[
                    "metadata",
                    "annotations",
                    "storageclass.beta.kubernetes.io/is-default-class",
                ],
            )
        })
        .unwrap_or("false")
        .to_owned(),
        age.to_owned(),
    ]
}

fn claim_ref(raw: &serde_json::Value) -> String {
    let namespace = value_str(raw, &["spec", "claimRef", "namespace"]);
    let name = value_str(raw, &["spec", "claimRef", "name"]);
    match (namespace, name) {
        (Some(namespace), Some(name)) => format!("{namespace}/{name}"),
        (_, Some(name)) => name.to_owned(),
        _ => "N/A".to_owned(),
    }
}

fn describe_from_row(kind: StorageResourceKind, row: &StorageRow) -> StorageDescribe {
    StorageDescribe {
        title: kind.title(),
        summary: row
            .details
            .iter()
            .map(|(label, value)| DescribeField::new(label.clone(), value.clone()))
            .collect(),
        labels: resource_map_entries(row.raw.pointer("/metadata/labels")),
        annotations: resource_map_entries(row.raw.pointer("/metadata/annotations")),
        raw_yaml: full_manifest_yaml(&row.raw),
    }
}

fn show_storage_describe(ui: &mut egui::Ui, describe: &StorageDescribe) {
    describe_group(
        ui,
        egui_phosphor::regular::HARD_DRIVES,
        describe.title,
        |ui| {
            describe_fields(ui, &describe.summary);
        },
    );

    ui.add_space(10.0);
    describe_group(ui, egui_phosphor::regular::TAG, "Metadata", |ui| {
        describe_metadata_maps(
            ui,
            "storage-describe-metadata",
            &describe.labels,
            &describe.annotations,
        );
    });

    ui.add_space(10.0);
    describe_group(ui, egui_phosphor::regular::CODE, "Raw manifest", |ui| {
        describe_raw_manifest(ui, "storage-describe-raw-manifest", &describe.raw_yaml);
    });
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
    fn pvc_request_query_uses_selected_namespace() {
        let mut panel = StorageResourcePanel::new(StorageResourceKind::PersistentVolumeClaim);
        panel.namespace_filter = Some("production".to_owned());
        let query = panel.request_resources(ClusterId::new("local")).query();
        assert_eq!(query.resource.plural, "persistentvolumeclaims");
        assert_eq!(query.namespace.as_deref(), Some("production"));
    }

    #[test]
    fn pv_and_storage_class_queries_are_cluster_scoped() {
        let mut pv_panel = StorageResourcePanel::new(StorageResourceKind::PersistentVolume);
        let pv_query = pv_panel.request_resources(ClusterId::new("local")).query();
        assert_eq!(pv_query.resource.plural, "persistentvolumes");
        assert_eq!(pv_query.namespace, None);
        assert!(matches!(
            pv_query.resource.scope,
            miku_core::ResourceScope::Cluster
        ));

        let mut class_panel = StorageResourcePanel::new(StorageResourceKind::StorageClass);
        let class_query = class_panel
            .request_resources(ClusterId::new("local"))
            .query();
        assert_eq!(class_query.resource.plural, "storageclasses");
        assert_eq!(class_query.namespace, None);
        assert!(matches!(
            class_query.resource.scope,
            miku_core::ResourceScope::Cluster
        ));
    }

    #[test]
    fn storage_rows_extract_fields() {
        let pvc = row_from_summary(StorageResourceKind::PersistentVolumeClaim, &pvc_summary());
        assert_eq!(pvc.cells[2], "Bound");
        assert_eq!(pvc.cells[3], "pv-api");
        assert_eq!(pvc.cells[4], "fast");
        assert_eq!(pvc.cells[5], "10Gi");
        assert_eq!(pvc.cells[6], "ReadWriteOnce");

        let pv = row_from_summary(StorageResourceKind::PersistentVolume, &pv_summary());
        assert_eq!(pv.cells[1], "Bound");
        assert_eq!(pv.cells[2], "default/data");
        assert_eq!(pv.cells[3], "fast");
        assert_eq!(pv.cells[4], "10Gi");
        assert_eq!(pv.cells[6], "Delete");

        let class = row_from_summary(StorageResourceKind::StorageClass, &storage_class_summary());
        assert_eq!(class.cells[1], "kubernetes.io/no-provisioner");
        assert_eq!(class.cells[2], "Retain");
        assert_eq!(class.cells[3], "WaitForFirstConsumer");
        assert_eq!(class.cells[4], "true");
        assert_eq!(class.cells[5], "true");
    }

    #[test]
    fn storage_rows_handle_missing_fields() {
        let pvc = row_from_summary(
            StorageResourceKind::PersistentVolumeClaim,
            &minimal_summary("PersistentVolumeClaim"),
        );
        assert_eq!(pvc.cells[2], "N/A");
        assert_eq!(pvc.cells[3], "N/A");
        assert_eq!(pvc.cells[4], "N/A");
        assert_eq!(pvc.cells[5], "N/A");
        assert_eq!(pvc.cells[6], "N/A");

        let class = row_from_summary(
            StorageResourceKind::StorageClass,
            &minimal_summary("StorageClass"),
        );
        assert_eq!(class.cells[1], "N/A");
        assert_eq!(class.cells[5], "false");
    }

    #[test]
    fn storage_rows_sort_and_filter_case_insensitively() {
        let rows = StorageResourceKind::PersistentVolumeClaim.rows_from_list(&[
            pvc_summary_with_name("zeta", "worker"),
            pvc_summary_with_name("default", "api-b"),
            pvc_summary_with_name("default", "api-a"),
        ]);
        let keys = rows.iter().map(|row| row.key.as_str()).collect::<Vec<_>>();
        assert_eq!(keys, vec!["default/api-a", "default/api-b", "zeta/worker"]);
        assert_eq!(
            rows.iter()
                .filter(|row| row_matches_search(row, "READWRITEONCE"))
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
        let mut panel = StorageResourcePanel::new(StorageResourceKind::PersistentVolumeClaim);
        let cluster_id = ClusterId::new("local");
        let first = panel.request_resource_watch(cluster_id.clone());
        let second = panel.request_resource_watch(cluster_id);
        panel.apply_event(ResourceUiEvent::ResourceWatchUpdated {
            request: first,
            result: Ok(miku_api::ResourceEvent::Snapshot(ResourceList {
                items: vec![pvc_summary_with_name("default", "stale")],
                continue_token: None,
            })),
        });
        assert!(panel.rows.is_empty());
        panel.apply_event(ResourceUiEvent::ResourceWatchUpdated {
            request: second,
            result: Ok(miku_api::ResourceEvent::Snapshot(ResourceList {
                items: vec![pvc_summary()],
                continue_token: None,
            })),
        });
        assert_eq!(panel.rows[0].name, "data");
    }

    #[test]
    fn namespace_watch_events_update_pvc_selector() {
        let mut panel = StorageResourcePanel::new(StorageResourceKind::PersistentVolumeClaim);
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
        let mut panel = StorageResourcePanel::new(StorageResourceKind::PersistentVolumeClaim);
        let row = row_from_summary(
            StorageResourceKind::PersistentVolumeClaim,
            &with_server_fields(pvc_summary()),
        );
        let key = row.key.clone();
        panel.rows = vec![row];

        panel.apply_table_action(Some(StorageTableAction::Edit { key }));

        let dialog = panel.edit_dialog.as_ref().unwrap();
        assert_eq!(dialog.target.namespace.as_deref(), Some("default"));
        assert_eq!(dialog.target.name, "data");
        let manifest = serde_yaml::from_str::<serde_json::Value>(&dialog.yaml).unwrap();
        assert!(manifest.pointer("/metadata/creationTimestamp").is_none());
        assert!(manifest.pointer("/metadata/resourceVersion").is_none());
        assert!(manifest.pointer("/metadata/managedFields").is_none());
        assert!(manifest.pointer("/status").is_none());
    }

    #[test]
    fn delete_action_opens_delete_dialog_for_namespaced_and_cluster_resources() {
        let mut namespaced_panel =
            StorageResourcePanel::new(StorageResourceKind::PersistentVolumeClaim);
        let namespaced_row =
            row_from_summary(StorageResourceKind::PersistentVolumeClaim, &pvc_summary());
        let namespaced_key = namespaced_row.key.clone();
        namespaced_panel.rows = vec![namespaced_row];

        namespaced_panel.apply_table_action(Some(StorageTableAction::Delete {
            key: namespaced_key,
        }));

        let dialog = namespaced_panel.delete_dialog.as_ref().unwrap();
        assert_eq!(dialog.target.namespace.as_deref(), Some("default"));
        assert_eq!(dialog.target.name, "data");

        let mut cluster_panel = StorageResourcePanel::new(StorageResourceKind::PersistentVolume);
        let cluster_row = row_from_summary(StorageResourceKind::PersistentVolume, &pv_summary());
        let cluster_key = cluster_row.key.clone();
        cluster_panel.rows = vec![cluster_row];

        cluster_panel.apply_table_action(Some(StorageTableAction::Delete { key: cluster_key }));

        let dialog = cluster_panel.delete_dialog.as_ref().unwrap();
        assert_eq!(dialog.target.namespace, None);
        assert_eq!(dialog.target.name, "pv-api");
    }

    #[test]
    fn apply_completion_closes_edit_dialog_and_upserts_sorted_row() {
        let mut panel = StorageResourcePanel::new(StorageResourceKind::PersistentVolumeClaim);
        let row = row_from_summary(StorageResourceKind::PersistentVolumeClaim, &pvc_summary());
        panel.rows = StorageResourceKind::PersistentVolumeClaim.rows_from_list(&[
            pvc_summary_with_name("zeta", "worker"),
            pvc_summary_with_name("default", "api-b"),
        ]);
        panel.edit_dialog = Some(GenericEditDialog {
            target: row.target(),
            yaml: "kind: PersistentVolumeClaim".to_owned(),
            parse_error: None,
        });
        panel.action_request_id = Some(7);
        panel.action_error = Some("old error".to_owned());

        panel.apply_event(ResourceUiEvent::ResourceActionCompleted {
            request: super::super::ResourceActionRequest {
                request_id: 7,
                cluster_id: ClusterId::new("local"),
                kind: ResourceActionKind::ApplyResource {
                    resource: StorageResourceKind::PersistentVolumeClaim
                        .metadata()
                        .resource,
                    namespace: Some("default".to_owned()),
                    name: "api-a".to_owned(),
                    manifest: serde_json::json!({}),
                },
            },
            result: Ok(ResourceActionOutcome::Applied(pvc_summary_with_name(
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
        let mut panel = StorageResourcePanel::new(StorageResourceKind::PersistentVolumeClaim);
        let row = row_from_summary(StorageResourceKind::PersistentVolumeClaim, &pvc_summary());
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
                    resource: StorageResourceKind::PersistentVolumeClaim
                        .metadata()
                        .resource,
                    namespace: Some("default".to_owned()),
                    name: "data".to_owned(),
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
        let row = row_from_summary(StorageResourceKind::PersistentVolumeClaim, &pvc_summary());
        let mut edit_panel = StorageResourcePanel::new(StorageResourceKind::PersistentVolumeClaim);
        edit_panel.edit_dialog = Some(GenericEditDialog {
            target: row.target(),
            yaml: "kind: PersistentVolumeClaim".to_owned(),
            parse_error: None,
        });
        edit_panel.action_request_id = Some(7);

        edit_panel.apply_event(ResourceUiEvent::ResourceActionCompleted {
            request: super::super::ResourceActionRequest {
                request_id: 7,
                cluster_id: ClusterId::new("local"),
                kind: ResourceActionKind::ApplyResource {
                    resource: StorageResourceKind::PersistentVolumeClaim
                        .metadata()
                        .resource,
                    namespace: Some("default".to_owned()),
                    name: "data".to_owned(),
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

        let mut delete_panel =
            StorageResourcePanel::new(StorageResourceKind::PersistentVolumeClaim);
        delete_panel.delete_dialog = Some(GenericDeleteDialog {
            target: row.delete_target(),
        });
        delete_panel.action_request_id = Some(9);

        delete_panel.apply_event(ResourceUiEvent::ResourceActionCompleted {
            request: super::super::ResourceActionRequest {
                request_id: 9,
                cluster_id: ClusterId::new("local"),
                kind: ResourceActionKind::DeleteResource {
                    resource: StorageResourceKind::PersistentVolumeClaim
                        .metadata()
                        .resource,
                    namespace: Some("default".to_owned()),
                    name: "data".to_owned(),
                },
            },
            result: Err("delete denied".to_owned()),
        });

        assert!(delete_panel.delete_dialog.is_some());
        assert_eq!(delete_panel.action_error.as_deref(), Some("delete denied"));
    }

    #[test]
    fn cluster_change_clears_edit_delete_batch_and_pending_action() {
        let row = row_from_summary(StorageResourceKind::PersistentVolumeClaim, &pvc_summary());
        let mut panel = StorageResourcePanel::new(StorageResourceKind::PersistentVolumeClaim);
        panel.last_cluster_id = Some(ClusterId::new("old"));
        panel.edit_dialog = Some(GenericEditDialog {
            target: row.target(),
            yaml: "kind: PersistentVolumeClaim".to_owned(),
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

    fn pvc_summary() -> ResourceSummary {
        pvc_summary_with_name("default", "data")
    }

    fn pvc_summary_with_name(namespace: &str, name: &str) -> ResourceSummary {
        ResourceSummary {
            name: name.to_owned(),
            namespace: Some(namespace.to_owned()),
            kind: "PersistentVolumeClaim".to_owned(),
            status: None,
            raw: serde_json::json!({
                "metadata": {"name": name, "namespace": namespace, "creationTimestamp": "2026-05-18T10:00:00Z"},
                "spec": {
                    "volumeName": "pv-api",
                    "storageClassName": "fast",
                    "accessModes": ["ReadWriteOnce"],
                    "resources": {"requests": {"storage": "10Gi"}}
                },
                "status": {
                    "phase": "Bound",
                    "capacity": {"storage": "10Gi"}
                }
            }),
        }
    }

    fn pv_summary() -> ResourceSummary {
        ResourceSummary {
            name: "pv-api".to_owned(),
            namespace: None,
            kind: "PersistentVolume".to_owned(),
            status: None,
            raw: serde_json::json!({
                "metadata": {"name": "pv-api", "creationTimestamp": "2026-05-18T10:00:00Z"},
                "spec": {
                    "storageClassName": "fast",
                    "capacity": {"storage": "10Gi"},
                    "accessModes": ["ReadWriteOnce"],
                    "persistentVolumeReclaimPolicy": "Delete",
                    "claimRef": {"namespace": "default", "name": "data"}
                },
                "status": {"phase": "Bound"}
            }),
        }
    }

    fn storage_class_summary() -> ResourceSummary {
        ResourceSummary {
            name: "fast".to_owned(),
            namespace: None,
            kind: "StorageClass".to_owned(),
            status: None,
            raw: serde_json::json!({
                "metadata": {
                    "name": "fast",
                    "creationTimestamp": "2026-05-18T10:00:00Z",
                    "annotations": {"storageclass.kubernetes.io/is-default-class": "true"}
                },
                "provisioner": "kubernetes.io/no-provisioner",
                "reclaimPolicy": "Retain",
                "volumeBindingMode": "WaitForFirstConsumer",
                "allowVolumeExpansion": true
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
