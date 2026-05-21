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
pub(crate) struct EventResourcePanel {
    namespace_filter: Option<String>,
    search_text: String,
    namespaces: Vec<String>,
    namespace_status: LoadStatus,
    row_status: LoadStatus,
    rows: Vec<EventRow>,
    next_request_id: u64,
    namespace_request_id: Option<u64>,
    row_request_id: Option<u64>,
    namespace_watch_request_id: Option<u64>,
    row_watch_request_id: Option<u64>,
    last_cluster_id: Option<ClusterId>,
    describe_dialog: Option<EventDescribeDialog>,
    view_dialog: Option<EventViewDialog>,
}

impl EventResourcePanel {
    pub(crate) fn show(
        &mut self,
        ui: &mut egui::Ui,
        cluster_id: Option<&ClusterId>,
    ) -> ResourcePanelRequests {
        let mut requests = ResourcePanelRequests::default();
        let Some(cluster_id) = cluster_id else {
            ui.centered_and_justified(|ui| {
                ui.label("Select a cluster to load events.");
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
                .push(self.request_event_watch(cluster_id.clone()));
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
                ResourceLoadKind::Events { .. } => {
                    if self.row_request_id != Some(request.request_id) {
                        return;
                    }
                    self.row_request_id = None;
                    match result {
                        Ok(list) => {
                            self.rows = event_rows_from_list(&list.items);
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
                | ResourceLoadKind::Deployments { .. }
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
                ResourceLoadKind::Events { .. } => {
                    if self.row_watch_request_id != Some(request.request_id) {
                        return;
                    }
                    match result {
                        Ok(miku_api::ResourceEvent::Snapshot(list)) => {
                            self.rows = event_rows_from_list(&list.items);
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
                | ResourceLoadKind::Deployments { .. }
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
            egui::ComboBox::from_id_salt("event_resource_namespace_filter")
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

            let search_changed = ui
                .add(
                    egui::TextEdit::singleline(&mut self.search_text)
                        .hint_text("Search Events...")
                        .desired_width(280.0),
                )
                .changed();

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
                    .push(self.request_event_watch(cluster_id.clone()));
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
                    .push(self.request_event_watch(cluster_id.clone()));
            }

            if search_changed {
                ui.ctx().request_repaint();
            }
        });
    }

    fn show_body(&mut self, ui: &mut egui::Ui) {
        match &self.row_status {
            LoadStatus::Idle | LoadStatus::Loading if self.rows.is_empty() => {
                ui.centered_and_justified(|ui| {
                    ui.label("Loading events...");
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
                        ui.label("No events match the current filters.");
                    });
                    return;
                }

                let action = show_event_table(ui, &self.rows, row_indices);
                self.apply_table_action(action);
            }
        }
    }

    fn apply_table_action(&mut self, action: Option<EventTableAction>) {
        match action {
            Some(EventTableAction::Describe { key }) => {
                let Some((name, describe)) = self
                    .row_by_key(&key)
                    .map(|row| (row.name.clone(), event_describe_from_row(row)))
                else {
                    return;
                };
                self.describe_dialog = Some(EventDescribeDialog {
                    key,
                    name,
                    describe,
                });
            }
            Some(EventTableAction::View { key }) => {
                let Some((name, yaml)) = self
                    .row_by_key(&key)
                    .map(|row| (row.name.clone(), full_manifest_yaml(&row.raw)))
                else {
                    return;
                };
                self.view_dialog = Some(EventViewDialog { key, name, yaml });
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
            .id(egui::Id::new(("event-describe-dialog", &dialog.key)))
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .open(&mut open)
            .collapsible(false)
            .fixed_size([EVENT_DESCRIBE_DIALOG_WIDTH, EVENT_DESCRIBE_DIALOG_HEIGHT])
            .show(ctx, |ui| {
                ui.set_width(EVENT_DESCRIBE_DIALOG_WIDTH);
                ui.set_height(EVENT_DESCRIBE_CONTENT_HEIGHT);
                egui::ScrollArea::both()
                    .id_salt(("event-describe-content", &dialog.key))
                    .max_width(EVENT_DESCRIBE_DIALOG_WIDTH)
                    .max_height(EVENT_DESCRIBE_CONTENT_HEIGHT)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_min_width(EVENT_DESCRIBE_CONTENT_WIDTH);
                        ui.style_mut().wrap_mode = Some(TextWrapMode::Extend);
                        show_event_describe(ui, &dialog.describe);
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
            id: egui::Id::new(("event-view-dialog", &dialog.key)),
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
    fn request_events(&mut self, cluster_id: ClusterId) -> ResourceLoadRequest {
        let request = ResourceLoadRequest {
            request_id: self.allocate_request_id(),
            cluster_id,
            kind: ResourceLoadKind::Events {
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

    fn request_event_watch(&mut self, cluster_id: ClusterId) -> ResourceWatchRequest {
        let request = ResourceWatchRequest {
            request_id: self.allocate_request_id(),
            cluster_id,
            kind: ResourceLoadKind::Events {
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

    fn row_by_key(&self, key: &str) -> Option<&EventRow> {
        self.rows.iter().find(|row| row.key == key)
    }
}

fn show_event_table(
    ui: &mut egui::Ui,
    rows: &[EventRow],
    row_indices: Vec<usize>,
) -> Option<EventTableAction> {
    let row_height = ui.spacing().interact_size.y;
    let table_width: f32 = EVENT_COLUMN_WIDTHS.iter().sum::<f32>()
        + ui.spacing().item_spacing.x * EVENT_COLUMN_WIDTHS.len().saturating_sub(1) as f32;
    let mut action = None;

    egui::ScrollArea::horizontal()
        .id_salt("event_resource_table_horizontal")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.set_min_width(table_width);

            let mut table = TableBuilder::new(ui)
                .id_salt("event_resource_table")
                .striped(true)
                .resizable(false)
                .sense(egui::Sense::click())
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .min_scrolled_height(0.0);

            for width in EVENT_COLUMN_WIDTHS {
                table = table.column(Column::exact(width));
            }

            table
                .header(row_height, |mut header| {
                    for label in EVENT_COLUMNS {
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
                            ui.label(&row.namespace);
                        });
                        table_row.col(|ui| {
                            ui.colored_label(
                                event_type_color(ui, &row.event_type),
                                &row.event_type,
                            );
                        });
                        table_row.col(|ui| {
                            ui.label(&row.reason);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.involved_object);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.message);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.count);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.source);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.last_seen);
                        });
                        table_row.col(|ui| {
                            ui.label(&row.age);
                        });

                        table_row.response().context_menu(|ui| {
                            if ui
                                .button(format!("{} Describe", egui_phosphor::regular::INFO))
                                .clicked()
                            {
                                action = Some(EventTableAction::Describe {
                                    key: row.key.clone(),
                                });
                                ui.close();
                            }
                            if ui
                                .button(format!("{} View", egui_phosphor::regular::EYE))
                                .clicked()
                            {
                                action = Some(EventTableAction::View {
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

const EVENT_COLUMNS: [&str; 9] = [
    "Namespace",
    "Type",
    "Reason",
    "Object",
    "Message",
    "Count",
    "Source",
    "Last Seen",
    "Age",
];
const EVENT_COLUMN_WIDTHS: [f32; 9] = [160.0, 100.0, 180.0, 220.0, 420.0, 80.0, 200.0, 110.0, 90.0];
const EVENT_DESCRIBE_DIALOG_WIDTH: f32 = 860.0;
const EVENT_DESCRIBE_DIALOG_HEIGHT: f32 = 580.0;
const EVENT_DESCRIBE_CONTENT_HEIGHT: f32 = 520.0;
const EVENT_DESCRIBE_CONTENT_WIDTH: f32 = 1160.0;
const EVENT_DESCRIBE_SECTION_WIDTH: f32 = 1128.0;
const EVENT_DESCRIBE_FIELD_LABEL_WIDTH: f32 = 140.0;
const EVENT_DESCRIBE_FIELD_VALUE_WIDTH: f32 = 370.0;
const EVENT_DESCRIBE_LINE_WIDTH: f32 = 1080.0;

fn event_type_color(ui: &egui::Ui, event_type: &str) -> egui::Color32 {
    match event_type {
        "Normal" => egui::Color32::from_rgb(46, 160, 67),
        "Warning" => egui::Color32::from_rgb(191, 135, 0),
        _ => ui.visuals().text_color(),
    }
}

#[cfg(test)]
fn filter_event_rows<'a>(rows: &'a [EventRow], search_text: &str) -> Vec<&'a EventRow> {
    rows.iter()
        .filter(|row| row_matches_search(row, search_text))
        .collect()
}

fn row_matches_search(row: &EventRow, search_text: &str) -> bool {
    let needle = search_text.trim().to_lowercase();
    needle.is_empty()
        || row.namespace.to_lowercase().contains(&needle)
        || row.event_type.to_lowercase().contains(&needle)
        || row.reason.to_lowercase().contains(&needle)
        || row.involved_object.to_lowercase().contains(&needle)
        || row.message.to_lowercase().contains(&needle)
        || row.source.to_lowercase().contains(&needle)
}

fn event_rows_from_list(items: &[ResourceSummary]) -> Vec<EventRow> {
    let mut rows = items.iter().map(EventRow::from_summary).collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        right
            .sort_timestamp
            .cmp(&left.sort_timestamp)
            .then(left.namespace.cmp(&right.namespace))
            .then(left.name.cmp(&right.name))
    });
    rows
}

#[derive(Clone, Debug, PartialEq)]
struct EventRow {
    key: String,
    name: String,
    namespace: String,
    event_type: String,
    reason: String,
    involved_object: String,
    message: String,
    count: String,
    source: String,
    first_seen: String,
    last_seen: String,
    age: String,
    sort_timestamp: String,
    raw: serde_json::Value,
}

impl EventRow {
    fn from_summary(summary: &ResourceSummary) -> Self {
        let raw = &summary.raw;
        let name = value_str(raw, &["metadata", "name"]).unwrap_or(&summary.name);
        let namespace = value_str(raw, &["metadata", "namespace"])
            .or(summary.namespace.as_deref())
            .unwrap_or("N/A");
        let event_type = value_str(raw, &["type"]).unwrap_or("N/A");
        let reason = value_str(raw, &["reason"]).unwrap_or("N/A");
        let message = value_str(raw, &["message"]).unwrap_or("N/A");
        let involved_object = involved_object_name(raw);
        let source = event_source(raw);
        let first_timestamp = first_event_timestamp(raw);
        let last_timestamp = last_event_timestamp(raw);
        let created_timestamp = value_str(raw, &["metadata", "creationTimestamp"]);
        let sort_timestamp = last_timestamp
            .or(created_timestamp)
            .unwrap_or("")
            .to_owned();

        Self {
            key: event_key(namespace, name),
            name: name.to_owned(),
            namespace: namespace.to_owned(),
            event_type: event_type.to_owned(),
            reason: reason.to_owned(),
            involved_object,
            message: message.to_owned(),
            count: event_count(raw),
            source,
            first_seen: first_timestamp.map(format_timestamp).unwrap_or_else(na),
            last_seen: last_timestamp.map(format_timestamp).unwrap_or_else(na),
            age: created_timestamp.map(format_timestamp).unwrap_or_else(na),
            sort_timestamp,
            raw: summary.raw.clone(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum EventTableAction {
    Describe { key: String },
    View { key: String },
}

#[derive(Clone, Debug, PartialEq)]
struct EventDescribeDialog {
    key: String,
    name: String,
    describe: EventDescribe,
}

#[derive(Clone, Debug, PartialEq)]
struct EventViewDialog {
    key: String,
    name: String,
    yaml: String,
}

#[derive(Clone, Debug, PartialEq)]
struct EventDescribe {
    summary: Vec<DescribeField>,
    involved: Vec<DescribeField>,
    timing: Vec<DescribeField>,
    labels: Vec<ResourceMapEntry>,
    annotations: Vec<ResourceMapEntry>,
    message: String,
    raw_yaml: String,
}

#[derive(Clone, Debug, PartialEq)]
struct DescribeField {
    label: String,
    value: String,
}

fn show_event_describe(ui: &mut egui::Ui, describe: &EventDescribe) {
    describe_group(ui, egui_phosphor::regular::BELL, "Event", |ui| {
        describe_fields(ui, &describe.summary);
    });

    ui.add_space(10.0);
    describe_group(ui, egui_phosphor::regular::CUBE, "Involved object", |ui| {
        describe_fields(ui, &describe.involved);
    });

    ui.add_space(10.0);
    describe_group(ui, egui_phosphor::regular::CLOCK, "Timing", |ui| {
        describe_fields(ui, &describe.timing);
    });

    ui.add_space(10.0);
    describe_group(ui, egui_phosphor::regular::CHAT_TEXT, "Message", |ui| {
        non_wrapping_value(ui, &describe.message, EVENT_DESCRIBE_LINE_WIDTH);
    });

    ui.add_space(10.0);
    describe_group(ui, egui_phosphor::regular::TAG, "Metadata", |ui| {
        ResourceMapView {
            id_salt: "event-describe-labels",
            icon: egui_phosphor::regular::TAG,
            title: "Labels",
            entries: &describe.labels,
            empty_label: "No labels.",
        }
        .show(ui);
        ui.add_space(8.0);
        ResourceMapView {
            id_salt: "event-describe-annotations",
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
            .id_salt("event-describe-raw-manifest-content")
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
            ui.set_min_width(EVENT_DESCRIBE_SECTION_WIDTH);
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
                        [EVENT_DESCRIBE_FIELD_LABEL_WIDTH, 0.0],
                        egui::Label::new(egui::RichText::new(&field.label).weak())
                            .wrap_mode(TextWrapMode::Extend),
                    );
                    non_wrapping_value(ui, &field.value, EVENT_DESCRIBE_FIELD_VALUE_WIDTH);
                }
                if chunk.len() == 1 {
                    ui.label("");
                    ui.label("");
                }
                ui.end_row();
            }
        });
}

fn non_wrapping_value(ui: &mut egui::Ui, value: &str, width: f32) {
    ui.add_sized(
        [width, 0.0],
        egui::Label::new(value)
            .wrap_mode(TextWrapMode::Extend)
            .selectable(true),
    );
}

impl DescribeField {
    fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
        }
    }
}

fn event_describe_from_row(row: &EventRow) -> EventDescribe {
    let raw = &row.raw;
    EventDescribe {
        summary: vec![
            DescribeField::new("Name", row.name.clone()),
            DescribeField::new("Namespace", row.namespace.clone()),
            DescribeField::new("Type", row.event_type.clone()),
            DescribeField::new("Reason", row.reason.clone()),
            DescribeField::new("Count", row.count.clone()),
            DescribeField::new("Source", row.source.clone()),
        ],
        involved: vec![
            DescribeField::new("Object", row.involved_object.clone()),
            DescribeField::new(
                "Kind",
                value_str(raw, &["involvedObject", "kind"]).unwrap_or("N/A"),
            ),
            DescribeField::new(
                "Name",
                value_str(raw, &["involvedObject", "name"]).unwrap_or("N/A"),
            ),
            DescribeField::new(
                "Namespace",
                value_str(raw, &["involvedObject", "namespace"]).unwrap_or("N/A"),
            ),
            DescribeField::new(
                "UID",
                value_str(raw, &["involvedObject", "uid"]).unwrap_or("N/A"),
            ),
            DescribeField::new(
                "Field path",
                value_str(raw, &["involvedObject", "fieldPath"]).unwrap_or("N/A"),
            ),
        ],
        timing: vec![
            DescribeField::new("First seen", row.first_seen.clone()),
            DescribeField::new("Last seen", row.last_seen.clone()),
            DescribeField::new("Created", row.age.clone()),
            DescribeField::new(
                "Reporting controller",
                value_str(raw, &["reportingController"]).unwrap_or("N/A"),
            ),
            DescribeField::new(
                "Reporting instance",
                value_str(raw, &["reportingInstance"]).unwrap_or("N/A"),
            ),
        ],
        labels: string_map_entries(raw.pointer("/metadata/labels")),
        annotations: string_map_entries(raw.pointer("/metadata/annotations")),
        message: row.message.clone(),
        raw_yaml: full_manifest_yaml(raw),
    }
}

fn event_key(namespace: &str, name: &str) -> String {
    format!("{namespace}/{name}")
}

fn involved_object_name(raw: &serde_json::Value) -> String {
    let kind = value_str(raw, &["involvedObject", "kind"]).unwrap_or("N/A");
    let name = value_str(raw, &["involvedObject", "name"]).unwrap_or("N/A");
    format!("{kind}/{name}")
}

fn event_source(raw: &serde_json::Value) -> String {
    value_str(raw, &["source", "component"])
        .or_else(|| value_str(raw, &["reportingController"]))
        .or_else(|| value_str(raw, &["reportingInstance"]))
        .unwrap_or("N/A")
        .to_owned()
}

fn event_count(raw: &serde_json::Value) -> String {
    raw.get("count")
        .and_then(serde_json::Value::as_u64)
        .or_else(|| {
            raw.get("series")
                .and_then(|series| series.get("count"))
                .and_then(serde_json::Value::as_u64)
        })
        .map_or_else(na, |count| count.to_string())
}

fn first_event_timestamp(raw: &serde_json::Value) -> Option<&str> {
    value_str(raw, &["firstTimestamp"]).or_else(|| value_str(raw, &["eventTime"]))
}

fn last_event_timestamp(raw: &serde_json::Value) -> Option<&str> {
    value_str(raw, &["lastTimestamp"])
        .or_else(|| value_str(raw, &["eventTime"]))
        .or_else(|| value_str(raw, &["metadata", "creationTimestamp"]))
}

fn format_timestamp(timestamp: &str) -> String {
    human_age_from_rfc3339(timestamp).unwrap_or_else(|| timestamp.to_owned())
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

fn na() -> String {
    "N/A".to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use miku_api::ResourceList;

    #[test]
    fn event_request_query_uses_selected_namespace() {
        let mut panel = EventResourcePanel {
            namespace_filter: Some("production".to_owned()),
            ..EventResourcePanel::default()
        };

        let request = panel.request_events(ClusterId::new("local"));
        let query = request.query();

        assert_eq!(query.resource.plural, "events");
        assert_eq!(query.namespace.as_deref(), Some("production"));
    }

    #[test]
    fn event_row_extracts_table_fields_from_raw_summary() {
        let row = EventRow::from_summary(&event_summary(
            "api.17",
            "default",
            "Warning",
            "BackOff",
            "2026-05-18T10:04:00Z",
        ));

        assert_eq!(row.name, "api.17");
        assert_eq!(row.namespace, "default");
        assert_eq!(row.event_type, "Warning");
        assert_eq!(row.reason, "BackOff");
        assert_eq!(row.involved_object, "Pod/api");
        assert_eq!(row.message, "Back-off restarting failed container");
        assert_eq!(row.count, "7");
        assert_eq!(row.source, "kubelet");
        assert!(row.first_seen.ends_with(" ago"));
        assert!(row.last_seen.ends_with(" ago"));
        assert!(row.age.ends_with(" ago"));
    }

    #[test]
    fn event_row_handles_missing_optional_fields() {
        let row = EventRow::from_summary(&ResourceSummary {
            name: "minimal".to_owned(),
            namespace: None,
            kind: "Event".to_owned(),
            status: None,
            raw: serde_json::json!({"metadata": {"name": "minimal"}}),
        });

        assert_eq!(row.namespace, "N/A");
        assert_eq!(row.event_type, "N/A");
        assert_eq!(row.reason, "N/A");
        assert_eq!(row.involved_object, "N/A/N/A");
        assert_eq!(row.message, "N/A");
        assert_eq!(row.count, "N/A");
        assert_eq!(row.source, "N/A");
        assert_eq!(row.last_seen, "N/A");
    }

    #[test]
    fn event_rows_filter_by_multiple_fields_case_insensitively() {
        let rows = vec![
            EventRow::from_summary(&event_summary(
                "api.17",
                "default",
                "Warning",
                "BackOff",
                "2026-05-18T10:04:00Z",
            )),
            EventRow::from_summary(&event_summary(
                "worker.11",
                "production",
                "Normal",
                "Pulled",
                "2026-05-18T10:05:00Z",
            )),
        ];

        assert_eq!(filter_event_rows(&rows, "back-off").len(), 1);
        assert_eq!(filter_event_rows(&rows, "PRODUCTION").len(), 1);
        assert_eq!(filter_event_rows(&rows, "pod/API").len(), 1);
    }

    #[test]
    fn event_rows_are_sorted_by_last_seen_descending() {
        let rows = event_rows_from_list(&[
            event_summary(
                "older",
                "default",
                "Normal",
                "Pulled",
                "2026-05-18T10:00:00Z",
            ),
            event_summary(
                "newer",
                "default",
                "Warning",
                "BackOff",
                "2026-05-18T10:05:00Z",
            ),
        ]);

        let names = rows.into_iter().map(|row| row.name).collect::<Vec<_>>();
        assert_eq!(names, vec!["newer", "older"]);
    }

    #[test]
    fn event_describe_extracts_metadata() {
        let row = EventRow::from_summary(&event_summary(
            "api.17",
            "default",
            "Warning",
            "BackOff",
            "2026-05-18T10:04:00Z",
        ));
        let describe = event_describe_from_row(&row);

        assert!(describe.labels.iter().any(|entry| entry.key == "app"));
        assert!(describe.annotations.iter().any(|entry| entry.key == "note"));
        assert!(
            describe.involved.iter().any(|field| {
                field.label == "Field path" && field.value == "spec.containers{api}"
            })
        );
    }

    #[test]
    fn stale_resource_events_do_not_replace_current_rows() {
        let mut panel = EventResourcePanel::default();
        let cluster_id = ClusterId::new("local");
        let first = panel.request_events(cluster_id.clone());
        let second = panel.request_events(cluster_id);

        panel.apply_event(ResourceUiEvent::ResourcesLoaded {
            request: first,
            result: Ok(ResourceList {
                items: vec![event_summary(
                    "stale",
                    "default",
                    "Normal",
                    "Pulled",
                    "2026-05-18T10:00:00Z",
                )],
                continue_token: None,
            }),
        });
        assert!(panel.rows.is_empty());

        panel.apply_event(ResourceUiEvent::ResourcesLoaded {
            request: second,
            result: Ok(ResourceList {
                items: vec![event_summary(
                    "api.17",
                    "default",
                    "Warning",
                    "BackOff",
                    "2026-05-18T10:04:00Z",
                )],
                continue_token: None,
            }),
        });

        assert_eq!(panel.rows.len(), 1);
        assert_eq!(panel.rows[0].name, "api.17");
    }

    #[test]
    fn stale_watch_events_do_not_replace_current_rows() {
        let mut panel = EventResourcePanel::default();
        let cluster_id = ClusterId::new("local");
        let first = panel.request_event_watch(cluster_id.clone());
        let second = panel.request_event_watch(cluster_id);

        panel.apply_event(ResourceUiEvent::ResourceWatchUpdated {
            request: first,
            result: Ok(miku_api::ResourceEvent::Snapshot(ResourceList {
                items: vec![event_summary(
                    "stale",
                    "default",
                    "Normal",
                    "Pulled",
                    "2026-05-18T10:00:00Z",
                )],
                continue_token: None,
            })),
        });
        assert!(panel.rows.is_empty());

        panel.apply_event(ResourceUiEvent::ResourceWatchUpdated {
            request: second,
            result: Ok(miku_api::ResourceEvent::Snapshot(ResourceList {
                items: vec![event_summary(
                    "api.17",
                    "default",
                    "Warning",
                    "BackOff",
                    "2026-05-18T10:04:00Z",
                )],
                continue_token: None,
            })),
        });

        assert_eq!(panel.rows.len(), 1);
        assert_eq!(panel.rows[0].name, "api.17");
    }

    #[test]
    fn namespace_watch_events_from_shared_request_update_selector() {
        let mut panel = EventResourcePanel::default();
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

    fn event_summary(
        name: &str,
        namespace: &str,
        event_type: &str,
        reason: &str,
        last_timestamp: &str,
    ) -> ResourceSummary {
        let message = if reason == "BackOff" {
            "Back-off restarting failed container"
        } else {
            "Successfully pulled container image"
        };
        let involved_name = name.split('.').next().unwrap_or("api");
        ResourceSummary {
            name: name.to_owned(),
            namespace: Some(namespace.to_owned()),
            kind: "Event".to_owned(),
            status: None,
            raw: serde_json::json!({
                "metadata": {
                    "name": name,
                    "namespace": namespace,
                    "creationTimestamp": "2026-05-18T09:58:00Z",
                    "labels": {
                        "app": "api"
                    },
                    "annotations": {
                        "note": "example"
                    }
                },
                "type": event_type,
                "reason": reason,
                "message": message,
                "count": 7,
                "firstTimestamp": "2026-05-18T10:00:00Z",
                "lastTimestamp": last_timestamp,
                "source": {
                    "component": "kubelet"
                },
                "involvedObject": {
                    "kind": "Pod",
                    "name": involved_name,
                    "namespace": namespace,
                    "uid": "pod-uid",
                    "fieldPath": "spec.containers{api}"
                },
                "reportingController": "kubelet",
                "reportingInstance": "kind-worker"
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
