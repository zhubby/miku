use std::collections::BTreeSet;

use eframe::egui;
use egui_extras::{Column, TableBuilder};
use miku_api::ResourceSummary;
use miku_core::{ClusterId, ResourceRef};

use super::{
    LoadStatus, ResourceActionKind, ResourceActionOutcome, ResourceDeleteTarget, ResourceLoadKind,
    ResourceLoadRequest, ResourcePanelRequests, ResourceUiEvent,
    components::{
        GenericBatchDeleteDialog, GenericCreateDialog, ResourceBatchDeleteDialogInput,
        ResourceCreateDialogInput, ResourceCreateDialogResponse, ResourceDeleteDialogResponse,
        ResourceMetadata, ResourceRowTarget, ResourceToolbar, ResourceYamlViewDialog,
        SELECT_COLUMN_WIDTH, apply_resource_request, batch_delete_resource_request,
        default_resource_yaml, selected_delete_targets, show_resource_batch_delete_dialog,
        show_resource_create_dialog, show_row_selection_checkbox,
    },
};
use crate::time::human_age_from_rfc3339;

const CUSTOM_RESOURCE_COLUMNS: &[&str] = &[
    "Name", "Group", "Kind", "Plural", "Scope", "Versions", "Age",
];
const CUSTOM_RESOURCE_COLUMN_WIDTHS: &[f32] = &[260.0, 180.0, 160.0, 160.0, 110.0, 220.0, 120.0];
const CUSTOM_RESOURCE_INSTANCE_COLUMNS: &[&str] = &["Name", "Namespace", "Kind", "Status", "Age"];
const CUSTOM_RESOURCE_INSTANCE_COLUMN_WIDTHS: &[f32] = &[260.0, 180.0, 180.0, 160.0, 120.0];
const EXPAND_DIALOG_SIZE: egui::Vec2 = egui::vec2(900.0, 520.0);
const EXPAND_TABLE_HEIGHT: f32 = 390.0;

#[derive(Clone, Debug, Default)]
pub(crate) struct CustomResourcesPanel {
    search_text: String,
    status: LoadStatus,
    rows: Vec<CustomResourceRow>,
    selected_rows: BTreeSet<String>,
    expand_dialog: Option<CustomResourceExpandDialog>,
    view_dialog: Option<CustomResourceViewDialog>,
    create_dialog: Option<GenericCreateDialog>,
    batch_delete_dialog: Option<GenericBatchDeleteDialog>,
    action_request_id: Option<u64>,
    action_error: Option<String>,
    next_request_id: u64,
    row_request_id: Option<u64>,
    last_cluster_id: Option<ClusterId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CustomResourceRow {
    name: String,
    group: String,
    kind: String,
    plural: String,
    scope: String,
    versions: String,
    storage_version: Option<String>,
    age: String,
}

#[derive(Clone, Debug)]
struct CustomResourceExpandDialog {
    crd_key: String,
    title: String,
    resource: Option<ResourceRef>,
    namespaced: bool,
    status: LoadStatus,
    rows: Vec<CustomResourceInstanceRow>,
    selected_rows: BTreeSet<String>,
    create_dialog: Option<GenericCreateDialog>,
    batch_delete_dialog: Option<GenericBatchDeleteDialog>,
    action_request_id: Option<u64>,
    action_error: Option<String>,
    request_id: Option<u64>,
}

#[derive(Clone, Debug, PartialEq)]
struct CustomResourceInstanceRow {
    key: String,
    name: String,
    namespace: String,
    kind: String,
    status: String,
    age: String,
    raw: serde_json::Value,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CustomResourceViewDialog {
    key: String,
    name: String,
    yaml: String,
}

impl CustomResourcesPanel {
    pub(crate) fn show(
        &mut self,
        ui: &mut egui::Ui,
        cluster_id: Option<&ClusterId>,
    ) -> ResourcePanelRequests {
        let mut requests = ResourcePanelRequests::default();
        let Some(cluster_id) = cluster_id else {
            ui.centered_and_justified(|ui| {
                ui.label("Select a cluster to load custom resources.");
            });
            return requests;
        };

        self.reset_for_cluster_change(cluster_id);
        if matches!(self.status, LoadStatus::Idle) {
            requests
                .loads
                .push(self.request_crd_load(cluster_id.clone()));
        }

        self.show_toolbar(ui, cluster_id, &mut requests);
        ui.separator();
        self.show_body(ui, cluster_id, &mut requests);
        self.show_expand_dialog(ui.ctx(), cluster_id, &mut requests);
        self.show_create_dialog(ui.ctx(), cluster_id, &mut requests.actions);
        self.show_batch_delete_dialog(ui.ctx(), cluster_id, &mut requests.actions);
        self.show_view_dialog(ui.ctx());
        requests
    }

    pub(crate) fn apply_event(&mut self, event: ResourceUiEvent) {
        match event {
            ResourceUiEvent::ResourcesLoaded { request, result } => match &request.kind {
                ResourceLoadKind::CustomResourceDefinitions => {
                    if self.row_request_id != Some(request.request_id) {
                        return;
                    }

                    self.row_request_id = None;
                    match result {
                        Ok(list) => {
                            self.replace_rows(custom_resource_rows_from_items(&list.items));
                            self.status = LoadStatus::Loaded;
                        }
                        Err(error) => self.status = LoadStatus::Error(error),
                    }
                }
                ResourceLoadKind::CustomResources { .. } => {
                    let Some(dialog) = self.expand_dialog.as_mut() else {
                        return;
                    };
                    if dialog.request_id != Some(request.request_id) {
                        return;
                    }

                    dialog.request_id = None;
                    match result {
                        Ok(list) => {
                            dialog.replace_rows(custom_resource_instance_rows_from_items(
                                &list.items,
                            ));
                            dialog.status = LoadStatus::Loaded;
                        }
                        Err(error) => dialog.status = LoadStatus::Error(error),
                    }
                }
                _ => {}
            },
            ResourceUiEvent::ResourceActionCompleted { request, result } => {
                self.apply_action_result(request, result);
            }
            ResourceUiEvent::ResourceWatchUpdated { .. }
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
        self.search_text.clear();
        self.rows.clear();
        self.selected_rows.clear();
        self.expand_dialog = None;
        self.view_dialog = None;
        self.create_dialog = None;
        self.batch_delete_dialog = None;
        self.action_request_id = None;
        self.action_error = None;
        self.status = LoadStatus::Idle;
        self.row_request_id = None;
    }

    fn show_toolbar(
        &mut self,
        ui: &mut egui::Ui,
        cluster_id: &ClusterId,
        requests: &mut ResourcePanelRequests,
    ) {
        let item_count = self.filtered_row_indices().len();
        let response = ResourceToolbar {
            id_salt: "custom_resources_toolbar",
            namespaces: &[],
            namespace_filter: &mut None,
            search_text: &mut self.search_text,
            search_hint: "Search Custom Resources...",
            item_count,
            selected_count: self.selected_rows.len(),
            loading: matches!(self.status, LoadStatus::Loading),
        }
        .show(ui);

        if response.search_changed {
            self.prune_selection_to_visible();
        }
        if response.refresh_clicked {
            requests
                .loads
                .push(self.request_crd_load(cluster_id.clone()));
        }
        if response.create_clicked {
            self.create_dialog = Some(GenericCreateDialog {
                yaml: default_resource_yaml(crd_metadata(), None),
                parse_error: None,
            });
            self.action_error = None;
        }
        if response.batch_delete_clicked {
            let targets = self.selected_crd_delete_targets();
            if !targets.is_empty() {
                self.batch_delete_dialog = Some(GenericBatchDeleteDialog { targets });
                self.action_error = None;
            }
        }
    }

    fn show_body(
        &mut self,
        ui: &mut egui::Ui,
        cluster_id: &ClusterId,
        requests: &mut ResourcePanelRequests,
    ) {
        match &self.status {
            LoadStatus::Idle | LoadStatus::Loading if self.rows.is_empty() => {
                ui.centered_and_justified(|ui| {
                    ui.label("Loading custom resources...");
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
                        ui.label("No custom resources match the current filters.");
                    });
                    return;
                }

                let action = show_custom_resource_table(
                    ui,
                    &self.rows,
                    row_indices,
                    &mut self.selected_rows,
                );
                self.apply_table_action(action, cluster_id, requests);
            }
        }
    }

    fn apply_table_action(
        &mut self,
        action: Option<CustomResourceTableAction>,
        cluster_id: &ClusterId,
        requests: &mut ResourcePanelRequests,
    ) {
        let Some(CustomResourceTableAction::Expand { key }) = action else {
            return;
        };
        let Some(row) = self.rows.iter().find(|row| row.name == key).cloned() else {
            return;
        };

        let resource = row.resource_ref();
        let mut dialog = CustomResourceExpandDialog {
            crd_key: row.name.clone(),
            title: format!("{} ({})", row.kind, row.name),
            resource: resource.clone(),
            namespaced: row.scope == "Namespaced",
            status: LoadStatus::Loading,
            rows: Vec::new(),
            selected_rows: BTreeSet::new(),
            create_dialog: None,
            batch_delete_dialog: None,
            action_request_id: None,
            action_error: None,
            request_id: None,
        };

        if let Some(resource) = resource {
            let request = self.request_custom_resource_load(cluster_id.clone(), resource);
            dialog.request_id = Some(request.request_id);
            requests.loads.push(request);
        } else {
            dialog.status = LoadStatus::Error(
                "custom resource definition is missing group, plural, or served version".to_owned(),
            );
        }

        self.expand_dialog = Some(dialog);
    }

    fn show_expand_dialog(
        &mut self,
        ctx: &egui::Context,
        cluster_id: &ClusterId,
        requests: &mut ResourcePanelRequests,
    ) {
        let Some(dialog) = self.expand_dialog.as_mut() else {
            return;
        };

        let mut open = true;
        let mut refresh_clicked = false;
        let mut create_clicked = false;
        let mut batch_delete_clicked = false;
        let mut view_key = None;
        egui::Window::new(format!("Expand {}", dialog.title))
            .id(egui::Id::new(("custom_resource_expand", &dialog.crd_key)))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .fixed_size(EXPAND_DIALOG_SIZE)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            dialog.resource.is_some()
                                && !matches!(dialog.status, LoadStatus::Loading),
                            egui::Button::new(egui_phosphor::regular::ARROWS_CLOCKWISE),
                        )
                        .on_hover_text("Refresh")
                        .clicked()
                    {
                        refresh_clicked = true;
                    }
                    ui.label(format!("{} items", dialog.rows.len()));
                    if matches!(dialog.status, LoadStatus::Loading) {
                        ui.label("Loading...");
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        create_clicked = ui
                            .button(egui_phosphor::regular::PLUS)
                            .on_hover_text("Create")
                            .clicked();
                        let delete_text = egui::RichText::new(egui_phosphor::regular::TRASH)
                            .color(ui.visuals().error_fg_color);
                        batch_delete_clicked = ui
                            .add_enabled(
                                !dialog.selected_rows.is_empty(),
                                egui::Button::new(delete_text),
                            )
                            .on_hover_text("Delete selected")
                            .clicked();
                    });
                });
                ui.separator();

                match &dialog.status {
                    LoadStatus::Idle | LoadStatus::Loading if dialog.rows.is_empty() => {
                        ui.allocate_ui([ui.available_width(), EXPAND_TABLE_HEIGHT].into(), |ui| {
                            ui.centered_and_justified(|ui| {
                                ui.label("Loading custom resource objects...");
                            });
                        });
                    }
                    LoadStatus::Error(error) => {
                        ui.allocate_ui([ui.available_width(), EXPAND_TABLE_HEIGHT].into(), |ui| {
                            ui.centered_and_justified(|ui| {
                                ui.colored_label(ui.visuals().error_fg_color, error);
                            });
                        });
                    }
                    _ if dialog.rows.is_empty() => {
                        ui.allocate_ui([ui.available_width(), EXPAND_TABLE_HEIGHT].into(), |ui| {
                            ui.centered_and_justified(|ui| {
                                ui.label("No custom resource objects found.");
                            });
                        });
                    }
                    _ => {
                        view_key = show_custom_resource_instance_table(
                            ui,
                            &dialog.rows,
                            &mut dialog.selected_rows,
                        )
                        .map(|action| match action {
                            CustomResourceInstanceTableAction::View { key } => key,
                        });
                    }
                }
            });

        if refresh_clicked && let Some(resource) = dialog.resource.clone() {
            let request = self.request_custom_resource_load(cluster_id.clone(), resource);
            if let Some(dialog) = self.expand_dialog.as_mut() {
                dialog.request_id = Some(request.request_id);
                dialog.status = LoadStatus::Loading;
                requests.loads.push(request);
            }
        }

        if create_clicked && let Some(dialog) = self.expand_dialog.as_mut() {
            if let Some(metadata) = dialog.resource_metadata() {
                dialog.create_dialog = Some(GenericCreateDialog {
                    yaml: default_resource_yaml(metadata, None),
                    parse_error: None,
                });
                dialog.action_error = None;
            }
        }

        if batch_delete_clicked && let Some(dialog) = self.expand_dialog.as_mut() {
            let targets = dialog.selected_delete_targets();
            if !targets.is_empty() {
                dialog.batch_delete_dialog = Some(GenericBatchDeleteDialog { targets });
                dialog.action_error = None;
            }
        }

        self.show_instance_create_dialog(ctx, cluster_id, requests);
        self.show_instance_batch_delete_dialog(ctx, cluster_id, requests);

        if let Some(key) = view_key {
            let Some(dialog) = self.expand_dialog.as_ref() else {
                return;
            };
            let Some(row) = dialog.rows.iter().find(|row| row.key == key) else {
                return;
            };
            self.view_dialog = Some(CustomResourceViewDialog {
                key: row.key.clone(),
                name: row.name.clone(),
                yaml: full_manifest_yaml(&row.raw),
            });
        }

        if !open {
            self.expand_dialog = None;
        }
    }

    fn show_view_dialog(&mut self, ctx: &egui::Context) {
        let Some(dialog) = self.view_dialog.as_mut() else {
            return;
        };

        let mut open = true;
        let response = ResourceYamlViewDialog {
            id: egui::Id::new(("custom_resource_view", &dialog.key)),
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
                metadata: crd_metadata(),
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
                    crd_metadata(),
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
                metadata: crd_metadata(),
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
                    crd_metadata(),
                    dialog.targets,
                );
                self.action_request_id = Some(request.request_id);
                requests.push(request);
            }
        }
    }

    fn show_instance_create_dialog(
        &mut self,
        ctx: &egui::Context,
        cluster_id: &ClusterId,
        requests: &mut ResourcePanelRequests,
    ) {
        let Some(metadata) = self
            .expand_dialog
            .as_ref()
            .and_then(CustomResourceExpandDialog::resource_metadata)
        else {
            return;
        };
        let Some(dialog) = self.expand_dialog.as_mut() else {
            return;
        };
        let Some(create_dialog) = dialog.create_dialog.as_mut() else {
            return;
        };
        match show_resource_create_dialog(
            ctx,
            ResourceCreateDialogInput {
                metadata: metadata.clone(),
                dialog: create_dialog,
                action_error: dialog.action_error.as_deref(),
                action_in_flight: dialog.action_request_id.is_some(),
                namespace_default: None,
            },
        ) {
            ResourceCreateDialogResponse::None => {}
            ResourceCreateDialogResponse::Cancel => {
                dialog.create_dialog = None;
                dialog.action_error = None;
            }
            ResourceCreateDialogResponse::Apply(parsed) => {
                let request = apply_resource_request(
                    self.allocate_request_id(),
                    cluster_id.clone(),
                    metadata,
                    parsed,
                );
                if let Some(dialog) = self.expand_dialog.as_mut() {
                    dialog.action_request_id = Some(request.request_id);
                }
                requests.actions.push(request);
            }
        }
    }

    fn show_instance_batch_delete_dialog(
        &mut self,
        ctx: &egui::Context,
        cluster_id: &ClusterId,
        requests: &mut ResourcePanelRequests,
    ) {
        let Some(dialog) = self.expand_dialog.as_ref() else {
            return;
        };
        let Some(delete_dialog) = dialog.batch_delete_dialog.clone() else {
            return;
        };
        let Some(metadata) = dialog.resource_metadata() else {
            return;
        };
        match show_resource_batch_delete_dialog(
            ctx,
            ResourceBatchDeleteDialogInput {
                metadata: metadata.clone(),
                targets: &delete_dialog.targets,
                action_error: dialog.action_error.as_deref(),
                action_in_flight: dialog.action_request_id.is_some(),
            },
        ) {
            ResourceDeleteDialogResponse::None => {}
            ResourceDeleteDialogResponse::Cancel => {
                if let Some(dialog) = self.expand_dialog.as_mut() {
                    dialog.batch_delete_dialog = None;
                    dialog.action_error = None;
                }
            }
            ResourceDeleteDialogResponse::Delete => {
                let request = batch_delete_resource_request(
                    self.allocate_request_id(),
                    cluster_id.clone(),
                    metadata,
                    delete_dialog.targets,
                );
                if let Some(dialog) = self.expand_dialog.as_mut() {
                    dialog.action_request_id = Some(request.request_id);
                }
                requests.actions.push(request);
            }
        }
    }

    fn filtered_row_indices(&self) -> Vec<usize> {
        let search_text = self.search_text.trim().to_lowercase();
        self.rows
            .iter()
            .enumerate()
            .filter_map(|(index, row)| {
                (search_text.is_empty()
                    || row.name.to_lowercase().contains(&search_text)
                    || row.group.to_lowercase().contains(&search_text)
                    || row.kind.to_lowercase().contains(&search_text)
                    || row.plural.to_lowercase().contains(&search_text))
                .then_some(index)
            })
            .collect()
    }

    fn replace_rows(&mut self, rows: Vec<CustomResourceRow>) {
        let visible_keys = rows
            .iter()
            .map(|row| row.name.clone())
            .collect::<BTreeSet<_>>();
        self.selected_rows.retain(|key| visible_keys.contains(key));
        self.rows = rows;
    }

    fn prune_selection_to_visible(&mut self) {
        let visible_keys = self
            .filtered_row_indices()
            .into_iter()
            .filter_map(|index| self.rows.get(index))
            .map(|row| row.name.clone())
            .collect::<BTreeSet<_>>();
        self.selected_rows.retain(|key| visible_keys.contains(key));
    }

    fn selected_crd_delete_targets(&self) -> Vec<ResourceDeleteTarget> {
        self.rows
            .iter()
            .filter(|row| self.selected_rows.contains(&row.name))
            .map(|row| ResourceDeleteTarget {
                namespace: None,
                name: row.name.clone(),
            })
            .collect()
    }

    fn apply_action_result(
        &mut self,
        request: super::ResourceActionRequest,
        result: Result<ResourceActionOutcome, String>,
    ) {
        if self.action_request_id == Some(request.request_id) {
            self.action_request_id = None;
            match result {
                Ok(ResourceActionOutcome::Applied(_)) => {
                    self.create_dialog = None;
                    self.action_error = None;
                }
                Ok(ResourceActionOutcome::Deleted) => {
                    if let ResourceActionKind::DeleteResource { name, .. } = request.kind {
                        self.rows.retain(|row| row.name != name);
                        self.selected_rows.remove(&name);
                    }
                    self.action_error = None;
                }
                Ok(ResourceActionOutcome::BatchDeleted(targets)) => {
                    for target in targets {
                        self.rows.retain(|row| row.name != target.name);
                        self.selected_rows.remove(&target.name);
                    }
                    self.batch_delete_dialog = None;
                    self.action_error = None;
                }
                Ok(ResourceActionOutcome::Evicted) => {}
                Err(error) => self.action_error = Some(error),
            }
            return;
        }

        let Some(dialog) = self.expand_dialog.as_mut() else {
            return;
        };
        if dialog.action_request_id != Some(request.request_id) {
            return;
        }

        dialog.action_request_id = None;
        match result {
            Ok(ResourceActionOutcome::Applied(_)) => {
                dialog.create_dialog = None;
                dialog.action_error = None;
            }
            Ok(ResourceActionOutcome::Deleted) => {
                if let ResourceActionKind::DeleteResource {
                    namespace, name, ..
                } = request.kind
                {
                    let key = custom_resource_instance_key(namespace.as_deref(), &name);
                    dialog.rows.retain(|row| row.key != key);
                    dialog.selected_rows.remove(&key);
                }
                dialog.action_error = None;
            }
            Ok(ResourceActionOutcome::BatchDeleted(targets)) => {
                for target in targets {
                    let key =
                        custom_resource_instance_key(target.namespace.as_deref(), &target.name);
                    dialog.rows.retain(|row| row.key != key);
                    dialog.selected_rows.remove(&key);
                }
                dialog.batch_delete_dialog = None;
                dialog.action_error = None;
            }
            Ok(ResourceActionOutcome::Evicted) => {}
            Err(error) => dialog.action_error = Some(error),
        }
    }

    fn request_crd_load(&mut self, cluster_id: ClusterId) -> ResourceLoadRequest {
        let request_id = self.allocate_request_id();
        self.row_request_id = Some(request_id);
        self.status = LoadStatus::Loading;
        ResourceLoadRequest {
            request_id,
            cluster_id,
            kind: ResourceLoadKind::CustomResourceDefinitions,
        }
    }

    fn request_custom_resource_load(
        &mut self,
        cluster_id: ClusterId,
        resource: ResourceRef,
    ) -> ResourceLoadRequest {
        ResourceLoadRequest {
            request_id: self.allocate_request_id(),
            cluster_id,
            kind: ResourceLoadKind::CustomResources { resource },
        }
    }

    fn allocate_request_id(&mut self) -> u64 {
        self.next_request_id += 1;
        self.next_request_id
    }
}

impl CustomResourceRow {
    fn resource_ref(&self) -> Option<ResourceRef> {
        let version = self.storage_version.as_ref()?;
        if self.group == "N/A" || self.plural == "N/A" {
            return None;
        }

        let resource = ResourceRef::grouped(&self.group, version, &self.plural);
        if self.scope == "Namespaced" {
            Some(resource)
        } else {
            Some(resource.cluster_scoped())
        }
    }
}

impl CustomResourceExpandDialog {
    fn resource_metadata(&self) -> Option<ResourceMetadata> {
        let resource = self.resource.clone()?;
        Some(ResourceMetadata {
            id: format!("custom_resource_instance_{}", self.crd_key),
            title: self.title.clone(),
            api_version: custom_resource_api_version(&resource),
            kind: custom_resource_kind_from_title(&self.title),
            namespaced: self.namespaced,
            resource,
        })
    }

    fn replace_rows(&mut self, rows: Vec<CustomResourceInstanceRow>) {
        let visible_keys = rows
            .iter()
            .map(|row| row.key.clone())
            .collect::<BTreeSet<_>>();
        self.selected_rows.retain(|key| visible_keys.contains(key));
        self.rows = rows;
    }

    fn selected_delete_targets(&self) -> Vec<ResourceDeleteTarget> {
        let targets = self.rows.iter().map(CustomResourceInstanceRow::target);
        selected_delete_targets(&targets.collect::<Vec<_>>(), &self.selected_rows)
    }
}

impl CustomResourceInstanceRow {
    fn target(&self) -> ResourceRowTarget {
        ResourceRowTarget {
            key: self.key.clone(),
            namespace: custom_resource_namespace_target(&self.namespace),
            name: self.name.clone(),
        }
    }
}

fn crd_metadata() -> ResourceMetadata {
    ResourceMetadata {
        id: "custom_resource_definition".to_owned(),
        title: "CustomResourceDefinitions".to_owned(),
        api_version: "apiextensions.k8s.io/v1".to_owned(),
        kind: "CustomResourceDefinition".to_owned(),
        resource: ResourceRef::grouped("apiextensions.k8s.io", "v1", "customresourcedefinitions")
            .cluster_scoped(),
        namespaced: false,
    }
}

fn custom_resource_api_version(resource: &ResourceRef) -> String {
    match resource.group.as_deref() {
        Some(group) => format!("{group}/{}", resource.version),
        None => resource.version.clone(),
    }
}

fn custom_resource_kind_from_title(title: &str) -> String {
    title
        .split_once(" (")
        .map(|(kind, _)| kind)
        .unwrap_or(title)
        .to_owned()
}

fn custom_resource_namespace_target(namespace: &str) -> Option<String> {
    (!namespace.is_empty() && namespace != "N/A").then(|| namespace.to_owned())
}

fn custom_resource_instance_key(namespace: Option<&str>, name: &str) -> String {
    format!("{}/{}", namespace.unwrap_or("N/A"), name)
}

fn show_custom_resource_table(
    ui: &mut egui::Ui,
    rows: &[CustomResourceRow],
    row_indices: Vec<usize>,
    selected_rows: &mut BTreeSet<String>,
) -> Option<CustomResourceTableAction> {
    let row_height = ui.spacing().interact_size.y;
    let table_width: f32 = SELECT_COLUMN_WIDTH
        + CUSTOM_RESOURCE_COLUMN_WIDTHS.iter().sum::<f32>()
        + ui.spacing().item_spacing.x * CUSTOM_RESOURCE_COLUMN_WIDTHS.len() as f32;
    let mut action = None;

    egui::ScrollArea::horizontal()
        .id_salt("custom_resource_table_horizontal")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.set_min_width(table_width);

            let mut table = TableBuilder::new(ui)
                .id_salt("custom_resource_table")
                .striped(true)
                .resizable(false)
                .sense(egui::Sense::click())
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .min_scrolled_height(0.0);
            table = table.column(Column::exact(SELECT_COLUMN_WIDTH));

            for width in CUSTOM_RESOURCE_COLUMN_WIDTHS {
                table = table.column(Column::exact(*width));
            }

            table
                .header(row_height, |mut header| {
                    header.col(|_| {});
                    for label in CUSTOM_RESOURCE_COLUMNS {
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
                        table_row.set_selected(selected_rows.contains(&row.name));
                        let mut checkbox_changed = false;

                        table_row.col(|ui| {
                            checkbox_changed =
                                show_row_selection_checkbox(ui, selected_rows, &row.name);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.name);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.group);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.kind);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.plural);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.scope);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.versions);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.age);
                        });

                        let response = table_row.response();
                        if response.clicked() && !checkbox_changed {
                            selected_rows.clear();
                            selected_rows.insert(row.name.clone());
                        }
                        response.context_menu(|ui| {
                            if ui
                                .button(format!("{} Expand", egui_phosphor::regular::ARROWS_OUT))
                                .clicked()
                            {
                                action = Some(CustomResourceTableAction::Expand {
                                    key: row.name.clone(),
                                });
                                ui.close();
                            }
                        });
                    });
                });
        });

    action
}

fn show_custom_resource_instance_table(
    ui: &mut egui::Ui,
    rows: &[CustomResourceInstanceRow],
    selected_rows: &mut BTreeSet<String>,
) -> Option<CustomResourceInstanceTableAction> {
    let row_height = ui.spacing().interact_size.y;
    let table_width: f32 = SELECT_COLUMN_WIDTH
        + CUSTOM_RESOURCE_INSTANCE_COLUMN_WIDTHS.iter().sum::<f32>()
        + ui.spacing().item_spacing.x * CUSTOM_RESOURCE_INSTANCE_COLUMN_WIDTHS.len() as f32;
    let mut action = None;

    ui.allocate_ui([ui.available_width(), EXPAND_TABLE_HEIGHT].into(), |ui| {
        egui::ScrollArea::both()
            .id_salt("custom_resource_instance_table_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_min_width(table_width);

                let mut table = TableBuilder::new(ui)
                    .id_salt("custom_resource_instance_table")
                    .striped(true)
                    .resizable(false)
                    .sense(egui::Sense::click())
                    .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                    .min_scrolled_height(0.0);
                table = table.column(Column::exact(SELECT_COLUMN_WIDTH));

                for width in CUSTOM_RESOURCE_INSTANCE_COLUMN_WIDTHS {
                    table = table.column(Column::exact(*width));
                }

                table
                    .header(row_height, |mut header| {
                        header.col(|_| {});
                        for label in CUSTOM_RESOURCE_INSTANCE_COLUMNS {
                            header.col(|ui| {
                                ui.strong(*label);
                            });
                        }
                    })
                    .body(|body| {
                        body.rows(row_height, rows.len(), |mut table_row| {
                            let Some(row) = rows.get(table_row.index()) else {
                                return;
                            };
                            table_row.set_selected(selected_rows.contains(&row.key));
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
                                ui.label(&row.kind);
                            });
                            table_row.col(|ui| {
                                ui.label(&row.status);
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
                                    .button(format!("{} View", egui_phosphor::regular::EYE))
                                    .clicked()
                                {
                                    action = Some(CustomResourceInstanceTableAction::View {
                                        key: row.key.clone(),
                                    });
                                    ui.close();
                                }
                            });
                        });
                    });
            });
    });

    action
}

fn custom_resource_rows_from_items(items: &[ResourceSummary]) -> Vec<CustomResourceRow> {
    let mut rows = items
        .iter()
        .map(custom_resource_row_from_summary)
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| left.name.cmp(&right.name));
    rows
}

fn custom_resource_row_from_summary(summary: &ResourceSummary) -> CustomResourceRow {
    let spec = summary.raw.get("spec").unwrap_or(&serde_json::Value::Null);
    let names = spec.get("names").unwrap_or(&serde_json::Value::Null);

    CustomResourceRow {
        name: summary.name.clone(),
        group: value_str(spec, "group").unwrap_or("N/A").to_owned(),
        kind: value_str(names, "kind").unwrap_or("N/A").to_owned(),
        plural: value_str(names, "plural").unwrap_or("N/A").to_owned(),
        scope: value_str(spec, "scope").unwrap_or("N/A").to_owned(),
        versions: versions_label(spec),
        storage_version: selected_crd_version(spec).map(ToOwned::to_owned),
        age: summary
            .raw
            .pointer("/metadata/creationTimestamp")
            .and_then(serde_json::Value::as_str)
            .and_then(human_age_from_rfc3339)
            .unwrap_or_else(|| "N/A".to_owned()),
    }
}

fn custom_resource_instance_rows_from_items(
    items: &[ResourceSummary],
) -> Vec<CustomResourceInstanceRow> {
    let mut rows = items
        .iter()
        .map(custom_resource_instance_row_from_summary)
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        left.namespace
            .cmp(&right.namespace)
            .then_with(|| left.name.cmp(&right.name))
    });
    rows
}

fn custom_resource_instance_row_from_summary(
    summary: &ResourceSummary,
) -> CustomResourceInstanceRow {
    let namespace = summary.namespace.as_deref().unwrap_or("N/A").to_owned();
    let key = format!("{namespace}/{}", summary.name);

    CustomResourceInstanceRow {
        key,
        name: summary.name.clone(),
        namespace,
        kind: summary.kind.clone(),
        status: summary.status.as_deref().unwrap_or("N/A").to_owned(),
        age: summary
            .raw
            .pointer("/metadata/creationTimestamp")
            .and_then(serde_json::Value::as_str)
            .and_then(human_age_from_rfc3339)
            .unwrap_or_else(|| "N/A".to_owned()),
        raw: summary.raw.clone(),
    }
}

fn versions_label(spec: &serde_json::Value) -> String {
    let versions = spec
        .get("versions")
        .and_then(serde_json::Value::as_array)
        .map(|versions| {
            versions
                .iter()
                .filter(|version| {
                    version
                        .get("served")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false)
                })
                .filter_map(|version| {
                    let name = version.get("name").and_then(serde_json::Value::as_str)?;
                    let label = if version
                        .get("storage")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false)
                    {
                        format!("{name} (storage)")
                    } else {
                        name.to_owned()
                    };
                    Some(label)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if versions.is_empty() {
        "N/A".to_owned()
    } else {
        versions.join(", ")
    }
}

fn selected_crd_version(spec: &serde_json::Value) -> Option<&str> {
    let versions = spec.get("versions").and_then(serde_json::Value::as_array)?;
    versions
        .iter()
        .find(|version| {
            version
                .get("served")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
                && version
                    .get("storage")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false)
        })
        .or_else(|| {
            versions.iter().find(|version| {
                version
                    .get("served")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false)
            })
        })
        .and_then(|version| version.get("name"))
        .and_then(serde_json::Value::as_str)
}

fn value_str<'a>(value: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(serde_json::Value::as_str)
}

fn full_manifest_yaml(raw: &serde_json::Value) -> String {
    serde_yaml::to_string(raw).unwrap_or_else(|error| format!("failed to render yaml: {error}"))
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum CustomResourceTableAction {
    Expand { key: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum CustomResourceInstanceTableAction {
    View { key: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_resource_rows_include_crd_table_fields() {
        let items = vec![ResourceSummary {
            name: "widgets.example.com".to_owned(),
            namespace: None,
            kind: "CustomResourceDefinition".to_owned(),
            status: None,
            raw: serde_json::json!({
                "metadata": {
                    "name": "widgets.example.com",
                    "creationTimestamp": "2026-05-20T10:00:00Z"
                },
                "spec": {
                    "group": "example.com",
                    "scope": "Namespaced",
                    "names": {
                        "kind": "Widget",
                        "plural": "widgets"
                    },
                    "versions": [
                        { "name": "v1beta1", "served": true, "storage": false },
                        { "name": "v1", "served": true, "storage": true },
                        { "name": "v1alpha1", "served": false, "storage": false }
                    ]
                }
            }),
        }];

        let rows = custom_resource_rows_from_items(&items);

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "widgets.example.com");
        assert_eq!(rows[0].group, "example.com");
        assert_eq!(rows[0].kind, "Widget");
        assert_eq!(rows[0].plural, "widgets");
        assert_eq!(rows[0].scope, "Namespaced");
        assert_eq!(rows[0].versions, "v1beta1, v1 (storage)");
        assert_eq!(rows[0].storage_version.as_deref(), Some("v1"));
    }

    #[test]
    fn custom_resource_row_uses_first_served_version_when_storage_is_missing() {
        let row = custom_resource_row_from_summary(&ResourceSummary {
            name: "gadgets.example.com".to_owned(),
            namespace: None,
            kind: "CustomResourceDefinition".to_owned(),
            status: None,
            raw: serde_json::json!({
                "metadata": { "name": "gadgets.example.com" },
                "spec": {
                    "group": "example.com",
                    "scope": "Cluster",
                    "names": {
                        "kind": "Gadget",
                        "plural": "gadgets"
                    },
                    "versions": [
                        { "name": "v1alpha1", "served": false },
                        { "name": "v1beta1", "served": true },
                        { "name": "v1", "served": true }
                    ]
                }
            }),
        });

        assert_eq!(row.storage_version.as_deref(), Some("v1beta1"));
        assert_eq!(
            row.resource_ref(),
            Some(ResourceRef::grouped("example.com", "v1beta1", "gadgets").cluster_scoped())
        );
    }

    #[test]
    fn custom_resource_instance_row_extracts_table_fields_and_keeps_raw() {
        let raw = serde_json::json!({
            "metadata": {
                "name": "demo",
                "namespace": "default",
                "creationTimestamp": "2026-05-20T10:00:00Z"
            },
            "spec": { "size": "small" }
        });
        let row = custom_resource_instance_row_from_summary(&ResourceSummary {
            name: "demo".to_owned(),
            namespace: Some("default".to_owned()),
            kind: "Widget".to_owned(),
            status: Some("Ready".to_owned()),
            raw: raw.clone(),
        });

        assert_eq!(row.key, "default/demo");
        assert_eq!(row.name, "demo");
        assert_eq!(row.namespace, "default");
        assert_eq!(row.kind, "Widget");
        assert_eq!(row.status, "Ready");
        assert_eq!(row.raw, raw);
    }

    #[test]
    fn crd_metadata_uses_generic_crd_resource() {
        let metadata = crd_metadata();

        assert_eq!(metadata.id, "custom_resource_definition");
        assert_eq!(metadata.api_version, "apiextensions.k8s.io/v1");
        assert_eq!(metadata.kind, "CustomResourceDefinition");
        assert!(!metadata.namespaced);
        assert_eq!(
            metadata.resource,
            ResourceRef::grouped("apiextensions.k8s.io", "v1", "customresourcedefinitions")
                .cluster_scoped()
        );
    }

    #[test]
    fn expanded_custom_resource_metadata_uses_dynamic_kind_and_scope() {
        let dialog = CustomResourceExpandDialog {
            crd_key: "widgets.example.com".to_owned(),
            title: "Widget (widgets.example.com)".to_owned(),
            resource: Some(ResourceRef::grouped("example.com", "v1", "widgets")),
            namespaced: true,
            status: LoadStatus::Loaded,
            rows: Vec::new(),
            selected_rows: BTreeSet::new(),
            create_dialog: None,
            batch_delete_dialog: None,
            action_request_id: None,
            action_error: None,
            request_id: None,
        };

        let metadata = dialog.resource_metadata().unwrap();

        assert_eq!(metadata.api_version, "example.com/v1");
        assert_eq!(metadata.kind, "Widget");
        assert!(metadata.namespaced);
        assert!(default_resource_yaml(metadata, Some("demo-ns")).contains("namespace: demo-ns"));
    }

    #[test]
    fn selected_custom_resource_instance_targets_preserve_namespaces() {
        let mut dialog = CustomResourceExpandDialog {
            crd_key: "widgets.example.com".to_owned(),
            title: "Widget (widgets.example.com)".to_owned(),
            resource: Some(ResourceRef::grouped("example.com", "v1", "widgets")),
            namespaced: true,
            status: LoadStatus::Loaded,
            rows: vec![
                CustomResourceInstanceRow {
                    key: custom_resource_instance_key(Some("default"), "demo"),
                    name: "demo".to_owned(),
                    namespace: "default".to_owned(),
                    kind: "Widget".to_owned(),
                    status: "Ready".to_owned(),
                    age: "1d".to_owned(),
                    raw: serde_json::json!({}),
                },
                CustomResourceInstanceRow {
                    key: custom_resource_instance_key(None, "cluster-demo"),
                    name: "cluster-demo".to_owned(),
                    namespace: "N/A".to_owned(),
                    kind: "Widget".to_owned(),
                    status: "Ready".to_owned(),
                    age: "1d".to_owned(),
                    raw: serde_json::json!({}),
                },
            ],
            selected_rows: BTreeSet::from([
                custom_resource_instance_key(Some("default"), "demo"),
                custom_resource_instance_key(None, "cluster-demo"),
            ]),
            create_dialog: None,
            batch_delete_dialog: None,
            action_request_id: None,
            action_error: None,
            request_id: None,
        };

        let targets = dialog.selected_delete_targets();

        assert_eq!(
            targets,
            vec![
                ResourceDeleteTarget {
                    namespace: Some("default".to_owned()),
                    name: "demo".to_owned()
                },
                ResourceDeleteTarget {
                    namespace: None,
                    name: "cluster-demo".to_owned()
                }
            ]
        );

        dialog.replace_rows(vec![]);
        assert!(dialog.selected_rows.is_empty());
    }
}
