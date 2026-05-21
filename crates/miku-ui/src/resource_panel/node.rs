use eframe::egui::{self, TextWrapMode};
use egui_extras::{Column, TableBuilder};
use miku_api::ResourceSummary;
use miku_core::ClusterId;

#[cfg(test)]
use super::ResourceLoadRequest;
use super::components::{ResourceMapEntry, ResourceMapView, ResourceYamlViewDialog};
use super::{
    LoadStatus, ResourceLoadKind, ResourcePanelRequests, ResourceUiEvent, ResourceWatchRequest,
};
use crate::time::human_age_from_rfc3339;

#[derive(Clone, Debug, Default)]
pub(crate) struct NodeResourcePanel {
    search_text: String,
    row_status: LoadStatus,
    rows: Vec<NodeRow>,
    next_request_id: u64,
    row_request_id: Option<u64>,
    row_watch_request_id: Option<u64>,
    last_cluster_id: Option<ClusterId>,
    describe_dialog: Option<NodeDescribeDialog>,
    view_dialog: Option<NodeViewDialog>,
}

impl NodeResourcePanel {
    pub(crate) fn show(
        &mut self,
        ui: &mut egui::Ui,
        cluster_id: Option<&ClusterId>,
    ) -> ResourcePanelRequests {
        let mut requests = ResourcePanelRequests::default();
        let Some(cluster_id) = cluster_id else {
            ui.centered_and_justified(|ui| {
                ui.label("Select a cluster to load nodes.");
            });
            return requests;
        };

        self.reset_for_cluster_change(cluster_id);
        if matches!(self.row_status, LoadStatus::Idle) {
            requests
                .watches
                .push(self.request_node_watch(cluster_id.clone()));
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
                ResourceLoadKind::Nodes => {
                    if self.row_request_id != Some(request.request_id) {
                        return;
                    }
                    self.row_request_id = None;
                    match result {
                        Ok(list) => {
                            self.rows = node_rows_from_list(&list.items);
                            self.row_status = LoadStatus::Loaded;
                        }
                        Err(error) => self.row_status = LoadStatus::Error(error),
                    }
                }
                ResourceLoadKind::Namespaces
                | ResourceLoadKind::Pods { .. }
                | ResourceLoadKind::CustomResourceDefinitions => {}
            },
            ResourceUiEvent::ResourceWatchUpdated { request, result } => match request.kind {
                ResourceLoadKind::Nodes => {
                    if self.row_watch_request_id != Some(request.request_id) {
                        return;
                    }
                    match result {
                        Ok(miku_api::ResourceEvent::Snapshot(list)) => {
                            self.rows = node_rows_from_list(&list.items);
                            self.row_status = LoadStatus::Loaded;
                        }
                        Ok(_) => {}
                        Err(error) => self.row_status = LoadStatus::Error(error),
                    }
                }
                ResourceLoadKind::Namespaces
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
        self.search_text.clear();
        self.row_status = LoadStatus::Idle;
        self.rows.clear();
        self.row_request_id = None;
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
            ui.add(
                egui::TextEdit::singleline(&mut self.search_text)
                    .hint_text("Search Nodes...")
                    .desired_width(280.0),
            );

            if ui
                .button(egui_phosphor::regular::ARROWS_CLOCKWISE)
                .on_hover_text("Refresh")
                .clicked()
            {
                requests
                    .watches
                    .push(self.request_node_watch(cluster_id.clone()));
            }

            ui.separator();
            ui.label(format!("{} items", self.filtered_row_count()));

            if matches!(self.row_status, LoadStatus::Loading) {
                ui.label("Loading...");
            }
        });
    }

    fn show_body(&mut self, ui: &mut egui::Ui) {
        match &self.row_status {
            LoadStatus::Idle | LoadStatus::Loading if self.rows.is_empty() => {
                ui.centered_and_justified(|ui| {
                    ui.label("Loading nodes...");
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
                        ui.label("No nodes match the current filters.");
                    });
                    return;
                }

                let action = show_node_table(ui, &self.rows, row_indices);
                self.apply_table_action(action);
            }
        }
    }

    fn apply_table_action(&mut self, action: Option<NodeTableAction>) {
        match action {
            Some(NodeTableAction::Describe { key }) => {
                let Some((name, describe)) = self
                    .row_by_key(&key)
                    .map(|row| (row.name.clone(), node_describe_from_row(row)))
                else {
                    return;
                };
                self.describe_dialog = Some(NodeDescribeDialog {
                    key,
                    name,
                    describe,
                });
            }
            Some(NodeTableAction::View { key }) => {
                let Some((name, yaml)) = self
                    .row_by_key(&key)
                    .map(|row| (row.name.clone(), full_manifest_yaml(&row.raw)))
                else {
                    return;
                };
                self.view_dialog = Some(NodeViewDialog { key, name, yaml });
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
            .id(egui::Id::new(("node-describe-dialog", &dialog.key)))
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .open(&mut open)
            .collapsible(false)
            .fixed_size([NODE_DESCRIBE_DIALOG_WIDTH, NODE_DESCRIBE_DIALOG_HEIGHT])
            .show(ctx, |ui| {
                ui.set_width(NODE_DESCRIBE_DIALOG_WIDTH);
                ui.set_height(NODE_DESCRIBE_CONTENT_HEIGHT);
                egui::ScrollArea::both()
                    .id_salt(("node-describe-content", &dialog.key))
                    .max_width(NODE_DESCRIBE_DIALOG_WIDTH)
                    .max_height(NODE_DESCRIBE_CONTENT_HEIGHT)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_min_width(NODE_DESCRIBE_CONTENT_WIDTH);
                        ui.style_mut().wrap_mode = Some(TextWrapMode::Extend);
                        show_node_describe(ui, &dialog.describe);
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
            id: egui::Id::new(("node-view-dialog", &dialog.key)),
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
    fn request_nodes(&mut self, cluster_id: ClusterId) -> ResourceLoadRequest {
        let request = ResourceLoadRequest {
            request_id: self.allocate_request_id(),
            cluster_id,
            kind: ResourceLoadKind::Nodes,
        };
        self.row_request_id = Some(request.request_id);
        self.row_status = LoadStatus::Loading;
        request
    }

    fn request_node_watch(&mut self, cluster_id: ClusterId) -> ResourceWatchRequest {
        let request = ResourceWatchRequest {
            request_id: self.allocate_request_id(),
            cluster_id,
            kind: ResourceLoadKind::Nodes,
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

    fn row_by_key(&self, key: &str) -> Option<&NodeRow> {
        self.rows.iter().find(|row| row.key == key)
    }
}

fn show_node_table(
    ui: &mut egui::Ui,
    rows: &[NodeRow],
    row_indices: Vec<usize>,
) -> Option<NodeTableAction> {
    let row_height = ui.spacing().interact_size.y;
    let table_width: f32 = NODE_COLUMN_WIDTHS.iter().sum::<f32>()
        + ui.spacing().item_spacing.x * NODE_COLUMN_WIDTHS.len().saturating_sub(1) as f32;
    let mut action = None;

    egui::ScrollArea::horizontal()
        .id_salt("node_resource_table_horizontal")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.set_min_width(table_width);

            let mut table = TableBuilder::new(ui)
                .id_salt("node_resource_table")
                .striped(true)
                .resizable(false)
                .sense(egui::Sense::click())
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .min_scrolled_height(0.0);

            for width in NODE_COLUMN_WIDTHS {
                table = table.column(Column::exact(width));
            }

            table
                .header(row_height, |mut header| {
                    for label in NODE_COLUMNS {
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

                        table_row.col(|ui| {
                            ui.label(&row.name);
                        });
                        table_row.col(|ui| {
                            ui.colored_label(ready_color(ui, &row.ready), &row.ready);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.roles);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.version);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.os_image);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.kernel);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.container_runtime);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.internal_ip);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.pod_cidr);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.cpu);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.memory);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.age);
                        });

                        table_row.response().context_menu(|ui| {
                            if ui
                                .button(format!("{} Describe", egui_phosphor::regular::INFO))
                                .clicked()
                            {
                                action = Some(NodeTableAction::Describe {
                                    key: row.key.clone(),
                                });
                                ui.close();
                            }
                            if ui
                                .button(format!("{} View", egui_phosphor::regular::EYE))
                                .clicked()
                            {
                                action = Some(NodeTableAction::View {
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

const NODE_COLUMNS: [&str; 12] = [
    "Name",
    "Ready",
    "Roles",
    "Version",
    "OS/Image",
    "Kernel",
    "Container Runtime",
    "Internal IP",
    "Pods CIDR",
    "CPU",
    "Memory",
    "Age",
];

const NODE_COLUMN_WIDTHS: [f32; 12] = [
    240.0, 100.0, 140.0, 130.0, 240.0, 150.0, 190.0, 140.0, 150.0, 120.0, 150.0, 90.0,
];

const NODE_DESCRIBE_DIALOG_WIDTH: f32 = 860.0;
const NODE_DESCRIBE_DIALOG_HEIGHT: f32 = 580.0;
const NODE_DESCRIBE_CONTENT_HEIGHT: f32 = 520.0;
const NODE_DESCRIBE_CONTENT_WIDTH: f32 = 1160.0;
const NODE_DESCRIBE_SECTION_WIDTH: f32 = 1128.0;
const NODE_DESCRIBE_FIELD_LABEL_WIDTH: f32 = 130.0;
const NODE_DESCRIBE_FIELD_VALUE_WIDTH: f32 = 380.0;
const NODE_DESCRIBE_LINE_WIDTH: f32 = 1080.0;

fn ready_color(ui: &egui::Ui, ready: &str) -> egui::Color32 {
    match ready {
        "Ready" => egui::Color32::from_rgb(46, 160, 67),
        "NotReady" | "Unknown" => ui.visuals().error_fg_color,
        _ => ui.visuals().text_color(),
    }
}

#[cfg(test)]
fn filter_node_rows<'a>(rows: &'a [NodeRow], search_text: &str) -> Vec<&'a NodeRow> {
    rows.iter()
        .filter(|row| row_matches_search(row, search_text))
        .collect()
}

fn row_matches_search(row: &NodeRow, search_text: &str) -> bool {
    let needle = search_text.trim().to_lowercase();
    needle.is_empty() || row.name.to_lowercase().contains(&needle)
}

fn node_rows_from_list(items: &[ResourceSummary]) -> Vec<NodeRow> {
    let mut rows = items.iter().map(NodeRow::from_summary).collect::<Vec<_>>();
    rows.sort_by(|left, right| left.name.cmp(&right.name));
    rows
}

#[derive(Clone, Debug, PartialEq)]
struct NodeRow {
    key: String,
    name: String,
    ready: String,
    roles: String,
    version: String,
    os_image: String,
    kernel: String,
    container_runtime: String,
    internal_ip: String,
    pod_cidr: String,
    cpu: String,
    memory: String,
    age: String,
    raw: serde_json::Value,
}

impl NodeRow {
    fn from_summary(summary: &ResourceSummary) -> Self {
        let raw = &summary.raw;
        let name = value_str(raw, &["metadata", "name"]).unwrap_or(&summary.name);
        let cpu = allocatable_capacity(raw, "cpu");
        let memory = allocatable_capacity(raw, "memory");

        Self {
            key: name.to_owned(),
            name: name.to_owned(),
            ready: node_ready(raw),
            roles: node_roles(raw),
            version: value_str(raw, &["status", "nodeInfo", "kubeletVersion"])
                .unwrap_or("N/A")
                .to_owned(),
            os_image: value_str(raw, &["status", "nodeInfo", "osImage"])
                .unwrap_or("N/A")
                .to_owned(),
            kernel: value_str(raw, &["status", "nodeInfo", "kernelVersion"])
                .unwrap_or("N/A")
                .to_owned(),
            container_runtime: value_str(raw, &["status", "nodeInfo", "containerRuntimeVersion"])
                .unwrap_or("N/A")
                .to_owned(),
            internal_ip: node_address(raw, "InternalIP").unwrap_or_else(|| "N/A".to_owned()),
            pod_cidr: node_pod_cidr(raw),
            cpu,
            memory,
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
enum NodeTableAction {
    Describe { key: String },
    View { key: String },
}

#[derive(Clone, Debug, PartialEq)]
struct NodeDescribeDialog {
    key: String,
    name: String,
    describe: NodeDescribe,
}

#[derive(Clone, Debug, PartialEq)]
struct NodeViewDialog {
    key: String,
    name: String,
    yaml: String,
}

#[derive(Clone, Debug, PartialEq)]
struct NodeDescribe {
    summary: Vec<DescribeField>,
    resources: Vec<DescribeField>,
    system: Vec<DescribeField>,
    addresses: Vec<DescribeField>,
    conditions: Vec<NodeConditionDescribe>,
    taints: Vec<String>,
    labels: Vec<ResourceMapEntry>,
    annotations: Vec<ResourceMapEntry>,
    raw_yaml: String,
}

#[derive(Clone, Debug, PartialEq)]
struct NodeConditionDescribe {
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

fn show_node_describe(ui: &mut egui::Ui, describe: &NodeDescribe) {
    describe_group(ui, egui_phosphor::regular::HARD_DRIVES, "Node", |ui| {
        describe_fields(ui, &describe.summary);
    });

    ui.add_space(10.0);
    describe_group(ui, egui_phosphor::regular::GAUGE, "Resources", |ui| {
        describe_fields(ui, &describe.resources);
    });

    ui.add_space(10.0);
    describe_group(ui, egui_phosphor::regular::GEAR, "System", |ui| {
        describe_fields(ui, &describe.system);
    });

    ui.add_space(10.0);
    describe_group(
        ui,
        egui_phosphor::regular::ARROWS_DOWN_UP,
        "Addresses",
        |ui| {
            describe_fields(ui, &describe.addresses);
        },
    );

    ui.add_space(10.0);
    describe_group(
        ui,
        egui_phosphor::regular::CHECK_CIRCLE,
        "Conditions",
        |ui| {
            if describe.conditions.is_empty() {
                non_wrapping_value(ui, "N/A", NODE_DESCRIBE_LINE_WIDTH);
            } else {
                egui::Grid::new("node-describe-conditions")
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
                            non_wrapping_value(ui, &condition.condition_type, 180.0);
                            ui.colored_label(
                                condition_color(ui, &condition.status),
                                &condition.status,
                            );
                            non_wrapping_value(ui, &condition.reason, 220.0);
                            non_wrapping_value(ui, &condition.message, 520.0);
                            ui.end_row();
                        }
                    });
            }
        },
    );

    ui.add_space(10.0);
    describe_group(ui, egui_phosphor::regular::WARNING_CIRCLE, "Taints", |ui| {
        describe_lines(ui, &describe.taints);
    });

    ui.add_space(10.0);
    describe_group(ui, egui_phosphor::regular::TAG, "Metadata", |ui| {
        ResourceMapView {
            id_salt: "node-describe-labels",
            icon: egui_phosphor::regular::TAG,
            title: "Labels",
            entries: &describe.labels,
            empty_label: "No labels.",
        }
        .show(ui);
        ui.add_space(8.0);
        ResourceMapView {
            id_salt: "node-describe-annotations",
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
            .id_salt("node-describe-raw-manifest-content")
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
            ui.set_min_width(NODE_DESCRIBE_SECTION_WIDTH);
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
                        [NODE_DESCRIBE_FIELD_LABEL_WIDTH, 0.0],
                        egui::Label::new(egui::RichText::new(&field.label).weak())
                            .wrap_mode(TextWrapMode::Extend),
                    );
                    non_wrapping_value(ui, &field.value, NODE_DESCRIBE_FIELD_VALUE_WIDTH);
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
        non_wrapping_value(ui, "N/A", NODE_DESCRIBE_LINE_WIDTH);
        return;
    }

    for line in lines {
        non_wrapping_value(ui, line, NODE_DESCRIBE_LINE_WIDTH);
    }
}

fn non_wrapping_value(ui: &mut egui::Ui, value: &str, width: f32) {
    ui.add_sized(
        [width, 0.0],
        egui::Label::new(value)
            .wrap_mode(TextWrapMode::Extend)
            .selectable(true),
    );
}

fn condition_color(ui: &egui::Ui, status: &str) -> egui::Color32 {
    match status {
        "True" | "Ready" => egui::Color32::from_rgb(46, 160, 67),
        "False" => egui::Color32::from_rgb(191, 135, 0),
        "Unknown" => ui.visuals().error_fg_color,
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

fn node_describe_from_row(row: &NodeRow) -> NodeDescribe {
    let raw = &row.raw;
    NodeDescribe {
        summary: vec![
            DescribeField::new("Name", row.name.clone()),
            DescribeField::new("Ready", row.ready.clone()),
            DescribeField::new("Roles", row.roles.clone()),
            DescribeField::new("Age", row.age.clone()),
            DescribeField::new("Pod CIDR", row.pod_cidr.clone()),
            DescribeField::new("Internal IP", row.internal_ip.clone()),
        ],
        resources: vec![
            DescribeField::new("CPU", row.cpu.clone()),
            DescribeField::new("Memory", row.memory.clone()),
            DescribeField::new("Pods", allocatable_capacity(raw, "pods")),
            DescribeField::new(
                "Ephemeral storage",
                allocatable_capacity(raw, "ephemeral-storage"),
            ),
        ],
        system: vec![
            DescribeField::new("Kubelet", row.version.clone()),
            DescribeField::new("OS image", row.os_image.clone()),
            DescribeField::new("Kernel", row.kernel.clone()),
            DescribeField::new("Runtime", row.container_runtime.clone()),
            DescribeField::new(
                "Architecture",
                value_str(raw, &["status", "nodeInfo", "architecture"]).unwrap_or("N/A"),
            ),
            DescribeField::new(
                "Operating system",
                value_str(raw, &["status", "nodeInfo", "operatingSystem"]).unwrap_or("N/A"),
            ),
        ],
        addresses: node_address_fields(raw),
        conditions: node_condition_describes(raw),
        taints: node_taint_describes(raw),
        labels: string_map_entries(raw.pointer("/metadata/labels")),
        annotations: string_map_entries(raw.pointer("/metadata/annotations")),
        raw_yaml: full_manifest_yaml(raw),
    }
}

fn node_ready(raw: &serde_json::Value) -> String {
    let Some(ready) = node_condition(raw, "Ready") else {
        return "Unknown".to_owned();
    };

    match value_str(ready, &["status"]) {
        Some("True") => "Ready".to_owned(),
        Some("False") => "NotReady".to_owned(),
        Some("Unknown") => "Unknown".to_owned(),
        Some(status) => status.to_owned(),
        None => "Unknown".to_owned(),
    }
}

fn node_condition<'a>(
    raw: &'a serde_json::Value,
    condition_type: &str,
) -> Option<&'a serde_json::Value> {
    raw.pointer("/status/conditions")
        .and_then(serde_json::Value::as_array)?
        .iter()
        .find(|condition| value_str(condition, &["type"]) == Some(condition_type))
}

fn node_roles(raw: &serde_json::Value) -> String {
    let mut roles = raw
        .pointer("/metadata/labels")
        .and_then(serde_json::Value::as_object)
        .into_iter()
        .flat_map(|labels| {
            labels.iter().filter_map(|(key, value)| {
                if let Some(role) = key.strip_prefix("node-role.kubernetes.io/") {
                    return (!role.is_empty()).then(|| role.to_owned());
                }
                if key == "kubernetes.io/role" {
                    return value
                        .as_str()
                        .filter(|role| !role.is_empty())
                        .map(ToOwned::to_owned);
                }
                None
            })
        })
        .collect::<Vec<_>>();
    roles.sort();
    roles.dedup();

    if roles.is_empty() {
        "N/A".to_owned()
    } else {
        roles.join(", ")
    }
}

fn node_address(raw: &serde_json::Value, address_type: &str) -> Option<String> {
    raw.pointer("/status/addresses")
        .and_then(serde_json::Value::as_array)?
        .iter()
        .find(|address| value_str(address, &["type"]) == Some(address_type))
        .and_then(|address| value_str(address, &["address"]))
        .map(ToOwned::to_owned)
}

fn node_address_fields(raw: &serde_json::Value) -> Vec<DescribeField> {
    raw.pointer("/status/addresses")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .map(|address| {
            DescribeField::new(
                value_str(address, &["type"]).unwrap_or("N/A"),
                value_str(address, &["address"]).unwrap_or("N/A"),
            )
        })
        .collect()
}

fn node_pod_cidr(raw: &serde_json::Value) -> String {
    if let Some(cidrs) = raw
        .pointer("/spec/podCIDRs")
        .and_then(serde_json::Value::as_array)
    {
        let values = cidrs
            .iter()
            .filter_map(serde_json::Value::as_str)
            .collect::<Vec<_>>();
        if !values.is_empty() {
            return values.join(", ");
        }
    }

    value_str(raw, &["spec", "podCIDR"])
        .unwrap_or("N/A")
        .to_owned()
}

fn allocatable_capacity(raw: &serde_json::Value, name: &str) -> String {
    let allocatable = value_str(raw, &["status", "allocatable", name]);
    let capacity = value_str(raw, &["status", "capacity", name]);

    match (allocatable, capacity) {
        (Some(allocatable), Some(capacity)) => format!("{allocatable} / {capacity}"),
        (Some(allocatable), None) => allocatable.to_owned(),
        (None, Some(capacity)) => format!("N/A / {capacity}"),
        (None, None) => "N/A".to_owned(),
    }
}

fn node_condition_describes(raw: &serde_json::Value) -> Vec<NodeConditionDescribe> {
    raw.pointer("/status/conditions")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .map(|condition| NodeConditionDescribe {
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

fn node_taint_describes(raw: &serde_json::Value) -> Vec<String> {
    raw.pointer("/spec/taints")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .map(|taint| {
            let key = value_str(taint, &["key"]).unwrap_or("N/A");
            let value = value_str(taint, &["value"]);
            let effect = value_str(taint, &["effect"]).unwrap_or("N/A");
            match value {
                Some(value) if !value.is_empty() => format!("{key}={value}:{effect}"),
                _ => format!("{key}:{effect}"),
            }
        })
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

#[cfg(test)]
mod tests {
    use super::*;
    use miku_api::ResourceList;

    #[test]
    fn node_row_extracts_table_fields_from_raw_summary() {
        let row = NodeRow::from_summary(&node_summary());

        assert_eq!(row.name, "kind-worker");
        assert_eq!(row.ready, "Ready");
        assert_eq!(row.roles, "worker");
        assert_eq!(row.version, "v1.30.0");
        assert_eq!(row.os_image, "Ubuntu 22.04.4 LTS");
        assert_eq!(row.kernel, "6.6.0");
        assert_eq!(row.container_runtime, "containerd://1.7.13");
        assert_eq!(row.internal_ip, "172.18.0.2");
        assert_eq!(row.pod_cidr, "10.244.1.0/24, fd00:10:244:1::/64");
        assert_eq!(row.cpu, "3900m / 4");
        assert_eq!(row.memory, "7920Mi / 8192Mi");
        assert!(row.age.ends_with(" ago"));
    }

    #[test]
    fn node_rows_filter_by_name_case_insensitively() {
        let rows = vec![
            NodeRow::from_summary(&node_summary()),
            NodeRow::from_summary(&ResourceSummary {
                name: "control-plane".to_owned(),
                namespace: None,
                kind: "Node".to_owned(),
                status: None,
                raw: serde_json::json!({"metadata": {"name": "control-plane"}}),
            }),
        ];

        let filtered = filter_node_rows(&rows, "WORKER");

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "kind-worker");
    }

    #[test]
    fn node_rows_are_sorted_by_name() {
        let rows = node_rows_from_list(&[
            ResourceSummary {
                name: "worker-b".to_owned(),
                namespace: None,
                kind: "Node".to_owned(),
                status: None,
                raw: serde_json::json!({"metadata": {"name": "worker-b"}}),
            },
            ResourceSummary {
                name: "control-plane".to_owned(),
                namespace: None,
                kind: "Node".to_owned(),
                status: None,
                raw: serde_json::json!({"metadata": {"name": "control-plane"}}),
            },
            ResourceSummary {
                name: "worker-a".to_owned(),
                namespace: None,
                kind: "Node".to_owned(),
                status: None,
                raw: serde_json::json!({"metadata": {"name": "worker-a"}}),
            },
        ]);

        let names = rows.into_iter().map(|row| row.name).collect::<Vec<_>>();
        assert_eq!(names, vec!["control-plane", "worker-a", "worker-b"]);
    }

    #[test]
    fn node_describe_extracts_details() {
        let row = NodeRow::from_summary(&node_summary());
        let describe = node_describe_from_row(&row);

        assert_eq!(describe.conditions.len(), 2);
        assert_eq!(describe.conditions[0].condition_type, "Ready");
        assert_eq!(
            describe.taints,
            vec!["dedicated=ci:NoSchedule", "gpu:PreferNoSchedule"]
        );
        assert!(
            describe
                .labels
                .iter()
                .any(|entry| entry.key == "beta.kubernetes.io/os")
        );
        assert!(
            describe
                .addresses
                .iter()
                .any(|field| field.label == "Hostname" && field.value == "kind-worker")
        );
    }

    #[test]
    fn node_describe_handles_missing_optional_fields() {
        let row = NodeRow::from_summary(&ResourceSummary {
            name: "minimal".to_owned(),
            namespace: None,
            kind: "Node".to_owned(),
            status: None,
            raw: serde_json::json!({"metadata": {"name": "minimal"}, "status": {}}),
        });

        let describe = node_describe_from_row(&row);

        assert_eq!(row.ready, "Unknown");
        assert_eq!(row.roles, "N/A");
        assert_eq!(row.internal_ip, "N/A");
        assert_eq!(row.cpu, "N/A");
        assert!(describe.conditions.is_empty());
        assert!(describe.taints.is_empty());
    }

    #[test]
    fn stale_resource_events_do_not_replace_current_rows() {
        let mut panel = NodeResourcePanel::default();
        let cluster_id = ClusterId::new("local");
        let first = panel.request_nodes(cluster_id.clone());
        let second = panel.request_nodes(cluster_id);

        panel.apply_event(ResourceUiEvent::ResourcesLoaded {
            request: first,
            result: Ok(ResourceList {
                items: vec![ResourceSummary {
                    name: "stale".to_owned(),
                    namespace: None,
                    kind: "Node".to_owned(),
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
                items: vec![node_summary()],
                continue_token: None,
            }),
        });

        assert_eq!(panel.rows.len(), 1);
        assert_eq!(panel.rows[0].name, "kind-worker");
    }

    #[test]
    fn stale_watch_events_do_not_replace_current_rows() {
        let mut panel = NodeResourcePanel::default();
        let cluster_id = ClusterId::new("local");
        let first = panel.request_node_watch(cluster_id.clone());
        let second = panel.request_node_watch(cluster_id);

        panel.apply_event(ResourceUiEvent::ResourceWatchUpdated {
            request: first,
            result: Ok(miku_api::ResourceEvent::Snapshot(ResourceList {
                items: vec![ResourceSummary {
                    name: "stale".to_owned(),
                    namespace: None,
                    kind: "Node".to_owned(),
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
                items: vec![node_summary()],
                continue_token: None,
            })),
        });

        assert_eq!(panel.rows.len(), 1);
        assert_eq!(panel.rows[0].name, "kind-worker");
    }

    fn node_summary() -> ResourceSummary {
        ResourceSummary {
            name: "kind-worker".to_owned(),
            namespace: None,
            kind: "Node".to_owned(),
            status: None,
            raw: serde_json::json!({
                "metadata": {
                    "name": "kind-worker",
                    "creationTimestamp": "2026-05-18T10:00:00Z",
                    "labels": {
                        "node-role.kubernetes.io/worker": "",
                        "beta.kubernetes.io/os": "linux"
                    },
                    "annotations": {
                        "node.alpha.kubernetes.io/ttl": "0"
                    }
                },
                "spec": {
                    "podCIDR": "10.244.1.0/24",
                    "podCIDRs": ["10.244.1.0/24", "fd00:10:244:1::/64"],
                    "taints": [
                        {"key": "dedicated", "value": "ci", "effect": "NoSchedule"},
                        {"key": "gpu", "effect": "PreferNoSchedule"}
                    ]
                },
                "status": {
                    "addresses": [
                        {"type": "InternalIP", "address": "172.18.0.2"},
                        {"type": "Hostname", "address": "kind-worker"}
                    ],
                    "allocatable": {
                        "cpu": "3900m",
                        "memory": "7920Mi",
                        "pods": "110",
                        "ephemeral-storage": "80Gi"
                    },
                    "capacity": {
                        "cpu": "4",
                        "memory": "8192Mi",
                        "pods": "110",
                        "ephemeral-storage": "100Gi"
                    },
                    "conditions": [
                        {
                            "type": "Ready",
                            "status": "True",
                            "reason": "KubeletReady",
                            "message": "kubelet is posting ready status"
                        },
                        {
                            "type": "MemoryPressure",
                            "status": "False",
                            "reason": "KubeletHasSufficientMemory",
                            "message": "kubelet has sufficient memory"
                        }
                    ],
                    "nodeInfo": {
                        "kubeletVersion": "v1.30.0",
                        "osImage": "Ubuntu 22.04.4 LTS",
                        "kernelVersion": "6.6.0",
                        "containerRuntimeVersion": "containerd://1.7.13",
                        "architecture": "amd64",
                        "operatingSystem": "linux"
                    }
                }
            }),
        }
    }
}
