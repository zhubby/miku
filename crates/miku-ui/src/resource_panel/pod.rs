use std::collections::BTreeSet;

use eframe::egui;
use egui_table::{CellInfo, Column, HeaderCellInfo, HeaderRow, Table, TableDelegate};
use miku_api::ResourceSummary;
use miku_core::ClusterId;

use super::components::ResourceToolbar;
use super::{
    LoadStatus, ResourceLoadKind, ResourceLoadRequest, ResourceUiEvent, namespaces_from_list,
};

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
    last_cluster_id: Option<ClusterId>,
}

impl PodResourcePanel {
    pub(crate) fn show(
        &mut self,
        ui: &mut egui::Ui,
        cluster_id: Option<&ClusterId>,
    ) -> Vec<ResourceLoadRequest> {
        let mut requests = Vec::new();
        let Some(cluster_id) = cluster_id else {
            ui.centered_and_justified(|ui| {
                ui.label("Select a cluster to load pods.");
            });
            return requests;
        };

        self.reset_for_cluster_change(cluster_id);
        if matches!(self.namespace_status, LoadStatus::Idle) {
            requests.push(self.request_namespaces(cluster_id.clone()));
        }
        if matches!(self.row_status, LoadStatus::Idle) {
            requests.push(self.request_pods(cluster_id.clone()));
        }

        self.show_toolbar(ui, cluster_id, &mut requests);
        ui.separator();
        self.show_body(ui);

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
                            self.rows = list.items.iter().map(PodRow::from_summary).collect();
                            self.row_status = LoadStatus::Loaded;
                        }
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
    }

    fn show_toolbar(
        &mut self,
        ui: &mut egui::Ui,
        cluster_id: &ClusterId,
        requests: &mut Vec<ResourceLoadRequest>,
    ) {
        let filtered_rows = self.filtered_rows();
        let response = ResourceToolbar {
            id_salt: "pod_resource_toolbar",
            namespaces: &self.namespaces,
            namespace_filter: &mut self.namespace_filter,
            search_text: &mut self.search_text,
            search_hint: "Search Pods...",
            item_count: filtered_rows.len(),
            loading: matches!(self.row_status, LoadStatus::Loading),
        }
        .show(ui);

        if response.namespace_changed {
            requests.push(self.request_pods(cluster_id.clone()));
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
            requests.push(self.request_namespaces(cluster_id.clone()));
            requests.push(self.request_pods(cluster_id.clone()));
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

                let mut delegate = PodTableDelegate {
                    rows,
                    selected_rows: &mut self.selected_rows,
                };
                let columns = pod_columns();
                Table::new()
                    .id_salt("pod_resource_table")
                    .num_rows(delegate.rows.len() as u64)
                    .columns(columns)
                    .headers([HeaderRow::new(28.0)])
                    .auto_size_mode(egui_table::AutoSizeMode::OnParentResize)
                    .show(ui, &mut delegate);
            }
        }
    }

    fn request_namespaces(&mut self, cluster_id: ClusterId) -> ResourceLoadRequest {
        let request = ResourceLoadRequest {
            request_id: self.allocate_request_id(),
            cluster_id,
            kind: ResourceLoadKind::Namespaces,
        };
        self.namespace_request_id = Some(request.request_id);
        self.namespace_status = LoadStatus::Loading;
        request
    }

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

    fn allocate_request_id(&mut self) -> u64 {
        self.next_request_id += 1;
        self.next_request_id
    }

    fn filtered_rows(&self) -> Vec<PodRow> {
        filter_pod_rows(&self.rows, &self.search_text)
    }
}

fn pod_columns() -> Vec<Column> {
    [
        32.0, 220.0, 150.0, 90.0, 110.0, 90.0, 90.0, 140.0, 150.0, 90.0, 100.0, 80.0,
    ]
    .into_iter()
    .enumerate()
    .map(|(index, width)| Column {
        current: width,
        range: egui::Rangef::new(if index == 0 { 28.0 } else { 56.0 }, 360.0),
        id: Some(egui::Id::new(("pod-column", index))),
        resizable: index != 0,
        auto_size_this_frame: false,
    })
    .collect()
}

#[derive(Debug)]
struct PodTableDelegate<'a> {
    rows: Vec<PodRow>,
    selected_rows: &'a mut BTreeSet<String>,
}

impl TableDelegate for PodTableDelegate<'_> {
    fn header_cell_ui(&mut self, ui: &mut egui::Ui, cell: &HeaderCellInfo) {
        if let Some(label) = POD_COLUMNS.get(cell.col_range.start) {
            ui.strong(*label);
        }
    }

    fn cell_ui(&mut self, ui: &mut egui::Ui, cell: &CellInfo) {
        let row_index = cell.row_nr as usize;
        let Some(row) = self.rows.get(row_index) else {
            return;
        };

        match cell.col_nr {
            0 => {
                let mut selected = self.selected_rows.contains(&row.key);
                if ui.checkbox(&mut selected, "").changed() {
                    if selected {
                        self.selected_rows.insert(row.key.clone());
                    } else {
                        self.selected_rows.remove(&row.key);
                    }
                }
            }
            1 => {
                ui.label(&row.name);
            }
            2 => {
                ui.label(&row.namespace);
            }
            3 => {
                ui.label(&row.cpu);
            }
            4 => {
                ui.label(&row.memory);
            }
            5 => {
                ui.label(&row.containers);
            }
            6 => {
                ui.label(&row.restarts);
            }
            7 => {
                ui.label(&row.controlled_by);
            }
            8 => {
                ui.label(&row.node);
            }
            9 => {
                ui.label(&row.qos);
            }
            10 => {
                ui.colored_label(status_color(ui, &row.status), &row.status);
            }
            11 => {
                ui.label(&row.age);
            }
            _ => {}
        }
    }

    fn default_row_height(&self) -> f32 {
        32.0
    }
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

fn status_color(ui: &egui::Ui, status: &str) -> egui::Color32 {
    match status {
        "Running" => egui::Color32::from_rgb(46, 160, 67),
        "Pending" => egui::Color32::from_rgb(191, 135, 0),
        "Succeeded" => ui.visuals().weak_text_color(),
        "Failed" | "CrashLoopBackOff" | "Error" => ui.visuals().error_fg_color,
        _ => ui.visuals().text_color(),
    }
}

fn filter_pod_rows(rows: &[PodRow], search_text: &str) -> Vec<PodRow> {
    let needle = search_text.trim().to_lowercase();
    rows.iter()
        .filter(|row| needle.is_empty() || row.name.to_lowercase().contains(&needle))
        .cloned()
        .collect()
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PodRow {
    key: String,
    name: String,
    namespace: String,
    cpu: String,
    memory: String,
    containers: String,
    restarts: String,
    controlled_by: String,
    node: String,
    qos: String,
    status: String,
    age: String,
}

impl PodRow {
    fn from_summary(summary: &ResourceSummary) -> Self {
        let raw = &summary.raw;
        let name = value_str(raw, &["metadata", "name"]).unwrap_or(&summary.name);
        let namespace = value_str(raw, &["metadata", "namespace"])
            .or(summary.namespace.as_deref())
            .unwrap_or("N/A");
        let key = format!("{namespace}/{name}");
        let container_statuses = raw
            .pointer("/status/containerStatuses")
            .and_then(serde_json::Value::as_array);
        let total_containers = raw
            .pointer("/spec/containers")
            .and_then(serde_json::Value::as_array)
            .map_or(0, Vec::len);
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
                .map(age_from_timestamp)
                .unwrap_or_else(|| "N/A".to_owned()),
        }
    }
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

fn age_from_timestamp(timestamp: &str) -> String {
    timestamp
        .split('T')
        .next()
        .filter(|date| !date.is_empty())
        .unwrap_or(timestamp)
        .to_owned()
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
        assert_eq!(row.restarts, "3");
        assert_eq!(row.controlled_by, "ReplicaSet/api-75f");
        assert_eq!(row.node, "kind-worker");
        assert_eq!(row.qos, "Burstable");
        assert_eq!(row.status, "CrashLoopBackOff");
        assert_eq!(row.age, "2026-05-18");
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
