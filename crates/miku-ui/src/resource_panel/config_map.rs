use std::collections::BTreeSet;

use eframe::egui::{self, TextWrapMode};
use egui_extras::{Column, TableBuilder};
use miku_api::ResourceSummary;
use miku_core::ClusterId;

#[cfg(test)]
use super::ResourceLoadRequest;
use super::components::{ResourceMapEntry, ResourceMapView, ResourceYamlViewDialog};
use super::{
    LoadStatus, ResourceLoadKind, ResourcePanelRequests, ResourceUiEvent, ResourceWatchRequest,
    namespaces_from_list,
};
use crate::time::human_age_from_rfc3339;

#[derive(Clone, Debug, Default)]
pub(crate) struct ConfigMapResourcePanel {
    namespace_filter: Option<String>,
    search_text: String,
    namespaces: Vec<String>,
    namespace_status: LoadStatus,
    row_status: LoadStatus,
    rows: Vec<ConfigMapRow>,
    next_request_id: u64,
    namespace_request_id: Option<u64>,
    row_request_id: Option<u64>,
    namespace_watch_request_id: Option<u64>,
    row_watch_request_id: Option<u64>,
    last_cluster_id: Option<ClusterId>,
    describe_dialog: Option<ConfigMapDescribeDialog>,
    view_dialog: Option<ConfigMapViewDialog>,
}

impl ConfigMapResourcePanel {
    pub(crate) fn show(
        &mut self,
        ui: &mut egui::Ui,
        cluster_id: Option<&ClusterId>,
    ) -> ResourcePanelRequests {
        let mut requests = ResourcePanelRequests::default();
        let Some(cluster_id) = cluster_id else {
            ui.centered_and_justified(|ui| {
                ui.label("Select a cluster to load configmaps.");
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
                .push(self.request_config_map_watch(cluster_id.clone()));
        }

        self.show_toolbar(ui, cluster_id, &mut requests);
        ui.separator();
        self.show_body(ui);
        self.show_describe_dialog(ui.ctx());
        self.show_view_dialog(ui.ctx());

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
                ResourceLoadKind::ConfigMaps { .. } => {
                    if self.row_request_id != Some(request.request_id) {
                        return;
                    }
                    self.row_request_id = None;
                    match result {
                        Ok(list) => {
                            self.rows = config_map_rows_from_list(&list.items);
                            self.row_status = LoadStatus::Loaded;
                        }
                        Err(error) => self.row_status = LoadStatus::Error(error),
                    }
                }
                ResourceLoadKind::Nodes
                | ResourceLoadKind::CronJobs { .. }
                | ResourceLoadKind::DaemonSets { .. }
                | ResourceLoadKind::Deployments { .. }
                | ResourceLoadKind::Events { .. }
                | ResourceLoadKind::Jobs { .. }
                | ResourceLoadKind::ReplicaSets { .. }
                | ResourceLoadKind::Secrets { .. }
                | ResourceLoadKind::StatefulSets { .. }
                | ResourceLoadKind::Pods { .. }
                | ResourceLoadKind::CustomResourceDefinitions => {}
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
                ResourceLoadKind::ConfigMaps { .. } => {
                    if self.row_watch_request_id != Some(request.request_id) {
                        return;
                    }
                    match result {
                        Ok(miku_api::ResourceEvent::Snapshot(list)) => {
                            self.rows = config_map_rows_from_list(&list.items);
                            self.row_status = LoadStatus::Loaded;
                        }
                        Ok(_) => {}
                        Err(error) => self.row_status = LoadStatus::Error(error),
                    }
                }
                ResourceLoadKind::Nodes
                | ResourceLoadKind::CronJobs { .. }
                | ResourceLoadKind::DaemonSets { .. }
                | ResourceLoadKind::Deployments { .. }
                | ResourceLoadKind::Events { .. }
                | ResourceLoadKind::Jobs { .. }
                | ResourceLoadKind::ReplicaSets { .. }
                | ResourceLoadKind::Secrets { .. }
                | ResourceLoadKind::StatefulSets { .. }
                | ResourceLoadKind::Pods { .. }
                | ResourceLoadKind::CustomResourceDefinitions => {}
            },
            ResourceUiEvent::ResourceActionCompleted { .. }
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
        self.namespace_filter = None;
        self.search_text.clear();
        self.namespaces.clear();
        self.rows.clear();
        self.namespace_status = LoadStatus::Idle;
        self.row_status = LoadStatus::Idle;
        self.namespace_request_id = None;
        self.row_request_id = None;
        self.namespace_watch_request_id = None;
        self.row_watch_request_id = None;
        self.describe_dialog = None;
        self.view_dialog = None;
    }

    fn show_toolbar(
        &mut self,
        ui: &mut egui::Ui,
        cluster_id: &ClusterId,
        requests: &mut ResourcePanelRequests,
    ) {
        ui.horizontal(|ui| {
            let selected_label = self
                .namespace_filter
                .as_deref()
                .unwrap_or("All namespaces")
                .to_owned();
            let mut namespace_changed = false;

            egui::ComboBox::from_id_salt("config_map_resource_namespace_filter")
                .selected_text(selected_label)
                .width(220.0)
                .show_ui(ui, |ui| {
                    namespace_changed |= ui
                        .selectable_value(&mut self.namespace_filter, None, "All namespaces")
                        .changed();
                    for namespace in &self.namespaces {
                        namespace_changed |= ui
                            .selectable_value(
                                &mut self.namespace_filter,
                                Some(namespace.clone()),
                                namespace,
                            )
                            .changed();
                    }
                });

            ui.add(
                egui::TextEdit::singleline(&mut self.search_text)
                    .hint_text("Search ConfigMaps...")
                    .desired_width(280.0),
            );

            if ui
                .button(egui_phosphor::regular::ARROWS_CLOCKWISE)
                .on_hover_text("Refresh")
                .clicked()
            {
                requests
                    .watches
                    .push(self.request_namespace_watch(cluster_id.clone()));
                requests
                    .watches
                    .push(self.request_config_map_watch(cluster_id.clone()));
            }

            ui.separator();
            ui.label(format!("{} items", self.filtered_row_count()));

            if matches!(self.row_status, LoadStatus::Loading) {
                ui.label("Loading...");
            }
            if matches!(self.namespace_status, LoadStatus::Error(_)) {
                ui.colored_label(ui.visuals().error_fg_color, "Namespaces unavailable");
            }
            if namespace_changed {
                requests
                    .watches
                    .push(self.request_config_map_watch(cluster_id.clone()));
            }
        });
    }

    fn show_body(&mut self, ui: &mut egui::Ui) {
        match &self.row_status {
            LoadStatus::Idle | LoadStatus::Loading if self.rows.is_empty() => {
                ui.centered_and_justified(|ui| {
                    ui.label("Loading configmaps...");
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
                        ui.label("No configmaps match the current filters.");
                    });
                    return;
                }

                let action = show_config_map_table(ui, &self.rows, row_indices);
                self.apply_table_action(action);
            }
        }
    }

    fn apply_table_action(&mut self, action: Option<ConfigMapTableAction>) {
        match action {
            Some(ConfigMapTableAction::Describe { key }) => {
                let Some((name, describe)) = self
                    .row_by_key(&key)
                    .map(|row| (row.name.clone(), config_map_describe_from_row(row)))
                else {
                    return;
                };
                self.describe_dialog = Some(ConfigMapDescribeDialog {
                    key,
                    name,
                    describe,
                });
            }
            Some(ConfigMapTableAction::View { key }) => {
                let Some((name, yaml)) = self
                    .row_by_key(&key)
                    .map(|row| (row.name.clone(), full_manifest_yaml(&row.raw)))
                else {
                    return;
                };
                self.view_dialog = Some(ConfigMapViewDialog { key, name, yaml });
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
            .id(egui::Id::new(("config_map-describe-dialog", &dialog.key)))
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .open(&mut open)
            .collapsible(false)
            .fixed_size([DESCRIBE_DIALOG_WIDTH, DESCRIBE_DIALOG_HEIGHT])
            .show(ctx, |ui| {
                ui.set_width(DESCRIBE_DIALOG_WIDTH);
                ui.set_height(DESCRIBE_CONTENT_HEIGHT);
                egui::ScrollArea::both()
                    .id_salt(("config_map-describe-content", &dialog.key))
                    .max_width(DESCRIBE_DIALOG_WIDTH)
                    .max_height(DESCRIBE_CONTENT_HEIGHT)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_min_width(DESCRIBE_CONTENT_WIDTH);
                        ui.style_mut().wrap_mode = Some(TextWrapMode::Extend);
                        show_config_map_describe(ui, &dialog.describe);
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
            id: egui::Id::new(("config_map-view-dialog", &dialog.key)),
            title: format!("View {}", dialog.name),
            yaml: &dialog.yaml,
            open: &mut open,
        }
        .show(ctx);

        if !response.open {
            self.view_dialog = None;
        }
    }

    #[cfg(test)]
    fn request_config_maps(&mut self, cluster_id: ClusterId) -> ResourceLoadRequest {
        let request = ResourceLoadRequest {
            request_id: self.allocate_request_id(),
            cluster_id,
            kind: ResourceLoadKind::ConfigMaps {
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

    fn request_config_map_watch(&mut self, cluster_id: ClusterId) -> ResourceWatchRequest {
        let request = ResourceWatchRequest {
            request_id: self.allocate_request_id(),
            cluster_id,
            kind: ResourceLoadKind::ConfigMaps {
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

    fn row_by_key(&self, key: &str) -> Option<&ConfigMapRow> {
        self.rows.iter().find(|row| row.key == key)
    }
}

fn show_config_map_table(
    ui: &mut egui::Ui,
    rows: &[ConfigMapRow],
    row_indices: Vec<usize>,
) -> Option<ConfigMapTableAction> {
    let row_height = ui.spacing().interact_size.y;
    let table_width: f32 = COLUMN_WIDTHS.iter().sum::<f32>()
        + ui.spacing().item_spacing.x * COLUMN_WIDTHS.len().saturating_sub(1) as f32;
    let mut action = None;

    egui::ScrollArea::horizontal()
        .id_salt("config_map_resource_table_horizontal")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.set_min_width(table_width);
            let mut table = TableBuilder::new(ui)
                .id_salt("config_map_resource_table")
                .striped(true)
                .resizable(false)
                .sense(egui::Sense::click())
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .min_scrolled_height(0.0);
            for width in COLUMN_WIDTHS {
                table = table.column(Column::exact(width));
            }
            table
                .header(row_height, |mut header| {
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

                        table_row.col(|ui| {
                            ui.label(&row.name);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.namespace);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.data_count);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.binary_data_count);
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

                        table_row.response().context_menu(|ui| {
                            if ui
                                .button(format!("{} Describe", egui_phosphor::regular::INFO))
                                .clicked()
                            {
                                action = Some(ConfigMapTableAction::Describe {
                                    key: row.key.clone(),
                                });
                                ui.close();
                            }
                            if ui
                                .button(format!("{} View", egui_phosphor::regular::EYE))
                                .clicked()
                            {
                                action = Some(ConfigMapTableAction::View {
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
    "Data",
    "Binary Data",
    "Immutable",
    "Keys",
    "Age",
];
const COLUMN_WIDTHS: [f32; 7] = [240.0, 160.0, 80.0, 110.0, 100.0, 360.0, 90.0];
const DESCRIBE_DIALOG_WIDTH: f32 = 820.0;
const DESCRIBE_DIALOG_HEIGHT: f32 = 560.0;
const DESCRIBE_CONTENT_HEIGHT: f32 = 500.0;
const DESCRIBE_CONTENT_WIDTH: f32 = 1080.0;
const DESCRIBE_SECTION_WIDTH: f32 = 1048.0;
const DESCRIBE_FIELD_LABEL_WIDTH: f32 = 130.0;
const DESCRIBE_FIELD_VALUE_WIDTH: f32 = 360.0;

#[cfg(test)]
fn filter_config_map_rows<'a>(
    rows: &'a [ConfigMapRow],
    search_text: &str,
) -> Vec<&'a ConfigMapRow> {
    rows.iter()
        .filter(|row| row_matches_search(row, search_text))
        .collect()
}

fn row_matches_search(row: &ConfigMapRow, search_text: &str) -> bool {
    let needle = search_text.trim().to_lowercase();
    needle.is_empty()
        || row.name.to_lowercase().contains(&needle)
        || row.namespace.to_lowercase().contains(&needle)
        || row.keys.to_lowercase().contains(&needle)
        || row.summary.to_lowercase().contains(&needle)
}

fn config_map_rows_from_list(items: &[ResourceSummary]) -> Vec<ConfigMapRow> {
    let mut rows = items
        .iter()
        .map(ConfigMapRow::from_summary)
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        left.namespace
            .cmp(&right.namespace)
            .then(left.name.cmp(&right.name))
    });
    rows
}

#[derive(Clone, Debug, PartialEq)]
struct ConfigMapRow {
    key: String,
    name: String,
    namespace: String,
    data_count: String,
    binary_data_count: String,
    immutable: String,
    keys: String,
    summary: String,
    age: String,
    raw: serde_json::Value,
}

impl ConfigMapRow {
    fn from_summary(summary: &ResourceSummary) -> Self {
        let raw = &summary.raw;
        let name = value_str(raw, &["metadata", "name"]).unwrap_or(&summary.name);
        let namespace = value_str(raw, &["metadata", "namespace"])
            .or(summary.namespace.as_deref())
            .unwrap_or("N/A");
        let data_count = object_key_count(raw.pointer("/data"));
        let binary_data_count = object_key_count(raw.pointer("/binaryData"));
        let keys = sorted_keys(&[raw.pointer("/data"), raw.pointer("/binaryData")]);
        let keys_label = if keys.is_empty() {
            "N/A".to_owned()
        } else {
            keys.join(", ")
        };
        let immutable = value_bool(raw, &["immutable"])
            .map_or_else(|| "N/A".to_owned(), |value| value.to_string());

        Self {
            key: namespaced_key(namespace, name),
            name: name.to_owned(),
            namespace: namespace.to_owned(),
            data_count: data_count.to_string(),
            binary_data_count: binary_data_count.to_string(),
            immutable,
            keys: keys_label.clone(),
            summary: format!(
                "data={data_count}, binaryData={binary_data_count}, keys={keys_label}"
            ),
            age: value_str(raw, &["metadata", "creationTimestamp"])
                .map(|timestamp| {
                    human_age_from_rfc3339(timestamp).unwrap_or_else(|| timestamp.to_owned())
                })
                .unwrap_or_else(|| "N/A".to_owned()),
            raw: summary.raw.clone(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ConfigMapTableAction {
    Describe { key: String },
    View { key: String },
}

#[derive(Clone, Debug, PartialEq)]
struct ConfigMapDescribeDialog {
    key: String,
    name: String,
    describe: ConfigMapDescribe,
}

#[derive(Clone, Debug, PartialEq)]
struct ConfigMapViewDialog {
    key: String,
    name: String,
    yaml: String,
}

#[derive(Clone, Debug, PartialEq)]
struct ConfigMapDescribe {
    summary: Vec<DescribeField>,
    data_keys: Vec<ResourceMapEntry>,
    binary_data_keys: Vec<ResourceMapEntry>,
    labels: Vec<ResourceMapEntry>,
    annotations: Vec<ResourceMapEntry>,
    raw_yaml: String,
}

#[derive(Clone, Debug, PartialEq)]
struct DescribeField {
    label: String,
    value: String,
}

impl DescribeField {
    fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
        }
    }
}

fn show_config_map_describe(ui: &mut egui::Ui, describe: &ConfigMapDescribe) {
    describe_group(ui, egui_phosphor::regular::GEAR, "ConfigMap", |ui| {
        describe_fields(ui, &describe.summary);
    });

    ui.add_space(10.0);
    describe_group(ui, egui_phosphor::regular::KEY, "Keys", |ui| {
        ResourceMapView {
            id_salt: "config_map-describe-data-keys",
            icon: egui_phosphor::regular::KEY,
            title: "Data",
            entries: &describe.data_keys,
            empty_label: "No data keys.",
        }
        .show(ui);
        ui.add_space(8.0);
        ResourceMapView {
            id_salt: "config_map-describe-binary-data-keys",
            icon: egui_phosphor::regular::KEY,
            title: "Binary data",
            entries: &describe.binary_data_keys,
            empty_label: "No binary data keys.",
        }
        .show(ui);
    });

    ui.add_space(10.0);
    describe_group(ui, egui_phosphor::regular::TAG, "Metadata", |ui| {
        ResourceMapView {
            id_salt: "config_map-describe-labels",
            icon: egui_phosphor::regular::TAG,
            title: "Labels",
            entries: &describe.labels,
            empty_label: "No labels.",
        }
        .show(ui);
        ui.add_space(8.0);
        ResourceMapView {
            id_salt: "config_map-describe-annotations",
            icon: egui_phosphor::regular::NOTE,
            title: "Annotations",
            entries: &describe.annotations,
            empty_label: "No annotations.",
        }
        .show(ui);
    });

    ui.add_space(10.0);
    describe_group(ui, egui_phosphor::regular::CODE, "Raw manifest", |ui| {
        egui::ScrollArea::both()
            .id_salt("config_map-describe-raw-manifest-content")
            .max_height(180.0)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add(
                    egui::Label::new(egui::RichText::new(&describe.raw_yaml).monospace())
                        .wrap_mode(TextWrapMode::Extend)
                        .selectable(true),
                );
            });
    });
}

fn describe_group(
    ui: &mut egui::Ui,
    icon: &str,
    title: &str,
    contents: impl FnOnce(&mut egui::Ui),
) {
    egui::Frame::new()
        .fill(ui.visuals().extreme_bg_color)
        .stroke(egui::Stroke::new(
            1.0,
            ui.visuals().widgets.noninteractive.bg_stroke.color,
        ))
        .corner_radius(egui::CornerRadius::same(4))
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_min_width(DESCRIBE_SECTION_WIDTH);
            ui.horizontal(|ui| {
                ui.label(icon);
                ui.strong(title);
            });
            ui.separator();
            contents(ui);
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
                        [DESCRIBE_FIELD_LABEL_WIDTH, 0.0],
                        egui::Label::new(egui::RichText::new(&field.label).weak())
                            .wrap_mode(TextWrapMode::Extend),
                    );
                    ui.add_sized(
                        [DESCRIBE_FIELD_VALUE_WIDTH, 0.0],
                        egui::Label::new(&field.value)
                            .wrap_mode(TextWrapMode::Extend)
                            .selectable(true),
                    );
                }
                if chunk.len() == 1 {
                    ui.label("");
                    ui.label("");
                }
                ui.end_row();
            }
        });
}

fn config_map_describe_from_row(row: &ConfigMapRow) -> ConfigMapDescribe {
    let raw = &row.raw;
    ConfigMapDescribe {
        summary: vec![
            DescribeField::new("Name", row.name.clone()),
            DescribeField::new("Namespace", row.namespace.clone()),
            DescribeField::new("Age", row.age.clone()),
            DescribeField::new("Immutable", row.immutable.clone()),
            DescribeField::new("Data", row.data_count.clone()),
            DescribeField::new("Binary data", row.binary_data_count.clone()),
            DescribeField::new("Keys", row.keys.clone()),
        ],
        data_keys: key_entries(raw.pointer("/data")),
        binary_data_keys: key_entries(raw.pointer("/binaryData")),
        labels: string_map_entries(raw.pointer("/metadata/labels")),
        annotations: string_map_entries(raw.pointer("/metadata/annotations")),
        raw_yaml: full_manifest_yaml(raw),
    }
}

fn namespaced_key(namespace: &str, name: &str) -> String {
    format!("{namespace}/{name}")
}

fn object_key_count(value: Option<&serde_json::Value>) -> usize {
    value
        .and_then(serde_json::Value::as_object)
        .map_or(0, |object| object.len())
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
        .map(|key| ResourceMapEntry::new(key, "present"))
        .collect()
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
    fn config_map_request_query_uses_selected_namespace() {
        let mut panel = ConfigMapResourcePanel {
            namespace_filter: Some("production".to_owned()),
            ..ConfigMapResourcePanel::default()
        };

        let request = panel.request_config_maps(ClusterId::new("local"));
        let query = request.query();

        assert_eq!(query.resource.plural, "configmaps");
        assert_eq!(query.resource.group, None);
        assert_eq!(query.namespace.as_deref(), Some("production"));
    }

    #[test]
    fn config_map_row_extracts_table_fields_from_raw_summary() {
        let row = ConfigMapRow::from_summary(&config_map_summary());

        assert_eq!(row.name, "app-config");
        assert_eq!(row.namespace, "default");
        assert_eq!(row.data_count, "2");
        assert_eq!(row.binary_data_count, "1");
        assert_eq!(row.immutable, "true");
        assert_eq!(row.keys, "app.toml, cert.bin, log-level");
        assert!(row.age.ends_with(" ago"));
    }

    #[test]
    fn config_map_row_handles_missing_optional_fields() {
        let row = ConfigMapRow::from_summary(&ResourceSummary {
            name: "minimal".to_owned(),
            namespace: Some("default".to_owned()),
            kind: "ConfigMap".to_owned(),
            status: None,
            raw: serde_json::json!({"metadata": {"name": "minimal", "namespace": "default"}}),
        });

        assert_eq!(row.data_count, "0");
        assert_eq!(row.binary_data_count, "0");
        assert_eq!(row.immutable, "N/A");
        assert_eq!(row.keys, "N/A");
        assert_eq!(row.age, "N/A");
    }

    #[test]
    fn config_map_rows_filter_by_fields_case_insensitively() {
        let rows = vec![
            ConfigMapRow::from_summary(&config_map_summary()),
            ConfigMapRow::from_summary(&config_map_summary_with_name("production", "worker-conf")),
        ];

        assert_eq!(filter_config_map_rows(&rows, "APP-CONFIG").len(), 1);
        assert_eq!(filter_config_map_rows(&rows, "PRODUCTION").len(), 1);
        assert_eq!(filter_config_map_rows(&rows, "log-level").len(), 2);
        assert_eq!(filter_config_map_rows(&rows, "binarydata=1").len(), 2);
    }

    #[test]
    fn config_map_rows_are_sorted_by_namespace_and_name() {
        let rows = config_map_rows_from_list(&[
            config_map_summary_with_name("zeta", "worker"),
            config_map_summary_with_name("default", "api-b"),
            config_map_summary_with_name("default", "api-a"),
        ]);

        let keys = rows.into_iter().map(|row| row.key).collect::<Vec<_>>();
        assert_eq!(keys, vec!["default/api-a", "default/api-b", "zeta/worker"]);
    }

    #[test]
    fn config_map_describe_extracts_metadata_and_key_lists() {
        let row = ConfigMapRow::from_summary(&config_map_summary());
        let describe = config_map_describe_from_row(&row);

        assert_eq!(describe.data_keys.len(), 2);
        assert_eq!(describe.binary_data_keys.len(), 1);
        assert_eq!(describe.labels.len(), 1);
        assert_eq!(describe.annotations.len(), 1);
        assert!(describe.raw_yaml.contains("app.toml"));
    }

    #[test]
    fn stale_resource_events_do_not_replace_current_rows() {
        let mut panel = ConfigMapResourcePanel::default();
        let cluster_id = ClusterId::new("local");
        let first = panel.request_config_maps(cluster_id.clone());
        let second = panel.request_config_maps(cluster_id);

        panel.apply_event(ResourceUiEvent::ResourcesLoaded {
            request: first,
            result: Ok(ResourceList {
                items: vec![config_map_summary_with_name("default", "stale")],
                continue_token: None,
            }),
        });
        assert!(panel.rows.is_empty());

        panel.apply_event(ResourceUiEvent::ResourcesLoaded {
            request: second,
            result: Ok(ResourceList {
                items: vec![config_map_summary()],
                continue_token: None,
            }),
        });

        assert_eq!(panel.rows[0].name, "app-config");
    }

    #[test]
    fn stale_watch_events_do_not_replace_current_rows() {
        let mut panel = ConfigMapResourcePanel::default();
        let cluster_id = ClusterId::new("local");
        let first = panel.request_config_map_watch(cluster_id.clone());
        let second = panel.request_config_map_watch(cluster_id);

        panel.apply_event(ResourceUiEvent::ResourceWatchUpdated {
            request: first,
            result: Ok(miku_api::ResourceEvent::Snapshot(ResourceList {
                items: vec![config_map_summary_with_name("default", "stale")],
                continue_token: None,
            })),
        });
        assert!(panel.rows.is_empty());

        panel.apply_event(ResourceUiEvent::ResourceWatchUpdated {
            request: second,
            result: Ok(miku_api::ResourceEvent::Snapshot(ResourceList {
                items: vec![config_map_summary()],
                continue_token: None,
            })),
        });

        assert_eq!(panel.rows[0].name, "app-config");
    }

    #[test]
    fn namespace_watch_events_from_shared_request_update_selector() {
        let mut panel = ConfigMapResourcePanel::default();
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

    fn config_map_summary() -> ResourceSummary {
        config_map_summary_with_name("default", "app-config")
    }

    fn config_map_summary_with_name(namespace: &str, name: &str) -> ResourceSummary {
        ResourceSummary {
            name: name.to_owned(),
            namespace: Some(namespace.to_owned()),
            kind: "ConfigMap".to_owned(),
            status: None,
            raw: serde_json::json!({
                "metadata": {
                    "name": name,
                    "namespace": namespace,
                    "creationTimestamp": "2026-05-18T10:00:00Z",
                    "labels": {"app": "api"},
                    "annotations": {"owner": "platform"}
                },
                "immutable": true,
                "data": {
                    "app.toml": "port = 8080",
                    "log-level": "info"
                },
                "binaryData": {
                    "cert.bin": "AA=="
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
