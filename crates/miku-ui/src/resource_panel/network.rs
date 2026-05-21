use eframe::egui::{self, TextWrapMode};
use egui_extras::{Column, TableBuilder};
use miku_api::ResourceSummary;
use miku_core::ClusterId;

#[cfg(test)]
use super::ResourceLoadRequest;
use super::components::ResourceYamlViewDialog;
use super::{
    LoadStatus, ResourceLoadKind, ResourcePanelRequests, ResourceUiEvent, ResourceWatchRequest,
    namespaces_from_list,
};
use crate::time::human_age_from_rfc3339;

macro_rules! network_panel {
    ($name:ident, $kind:expr) => {
        #[derive(Clone, Debug)]
        pub(crate) struct $name {
            inner: NetworkResourcePanel,
        }

        impl Default for $name {
            fn default() -> Self {
                Self {
                    inner: NetworkResourcePanel::new($kind),
                }
            }
        }

        impl $name {
            pub(crate) fn show(
                &mut self,
                ui: &mut egui::Ui,
                cluster_id: Option<&ClusterId>,
            ) -> ResourcePanelRequests {
                self.inner.show(ui, cluster_id)
            }

            pub(crate) fn apply_event(&mut self, event: ResourceUiEvent) {
                self.inner.apply_event(event);
            }
        }
    };
}

network_panel!(ServiceResourcePanel, NetworkResourceKind::Services);
network_panel!(
    EndpointSliceResourcePanel,
    NetworkResourceKind::EndpointSlices
);
network_panel!(EndpointsResourcePanel, NetworkResourceKind::Endpoints);
network_panel!(IngressResourcePanel, NetworkResourceKind::Ingresses);
network_panel!(
    IngressClassResourcePanel,
    NetworkResourceKind::IngressClasses
);
network_panel!(
    NetworkPolicyResourcePanel,
    NetworkResourceKind::NetworkPolicies
);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NetworkResourceKind {
    Services,
    EndpointSlices,
    Endpoints,
    Ingresses,
    IngressClasses,
    NetworkPolicies,
}

#[derive(Clone, Debug)]
struct NetworkResourcePanel {
    kind: NetworkResourceKind,
    namespace_filter: Option<String>,
    search_text: String,
    namespaces: Vec<String>,
    namespace_status: LoadStatus,
    row_status: LoadStatus,
    rows: Vec<NetworkRow>,
    next_request_id: u64,
    namespace_request_id: Option<u64>,
    row_request_id: Option<u64>,
    namespace_watch_request_id: Option<u64>,
    row_watch_request_id: Option<u64>,
    last_cluster_id: Option<ClusterId>,
    describe_dialog: Option<NetworkDescribeDialog>,
    view_dialog: Option<NetworkViewDialog>,
}

impl NetworkResourcePanel {
    fn new(kind: NetworkResourceKind) -> Self {
        Self {
            kind,
            namespace_filter: None,
            search_text: String::new(),
            namespaces: Vec::new(),
            namespace_status: LoadStatus::Idle,
            row_status: LoadStatus::Idle,
            rows: Vec::new(),
            next_request_id: 0,
            namespace_request_id: None,
            row_request_id: None,
            namespace_watch_request_id: None,
            row_watch_request_id: None,
            last_cluster_id: None,
            describe_dialog: None,
            view_dialog: None,
        }
    }

    fn show(&mut self, ui: &mut egui::Ui, cluster_id: Option<&ClusterId>) -> ResourcePanelRequests {
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
        requests
    }

    fn apply_event(&mut self, event: ResourceUiEvent) {
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
            ResourceUiEvent::ResourceActionCompleted { .. }
            | ResourceUiEvent::PodLogsLoaded { .. }
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
                self.rows = self.kind.rows_from_list(&list.items);
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
                self.rows = self.kind.rows_from_list(&list.items);
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
            let mut namespace_changed = false;
            if self.kind.is_namespaced() {
                egui::ComboBox::from_id_salt((self.kind.id(), "namespace_filter"))
                    .selected_text(
                        self.namespace_filter
                            .as_deref()
                            .unwrap_or("All namespaces")
                            .to_owned(),
                    )
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
            }

            ui.add(
                egui::TextEdit::singleline(&mut self.search_text)
                    .hint_text(format!("Search {}...", self.kind.title()))
                    .desired_width(280.0),
            );
            if ui
                .button(egui_phosphor::regular::ARROWS_CLOCKWISE)
                .on_hover_text("Refresh")
                .clicked()
            {
                if self.kind.is_namespaced() {
                    requests
                        .watches
                        .push(self.request_namespace_watch(cluster_id.clone()));
                }
                requests
                    .watches
                    .push(self.request_resource_watch(cluster_id.clone()));
            }
            ui.separator();
            ui.label(format!("{} items", self.filtered_row_count()));
            if matches!(self.row_status, LoadStatus::Loading) {
                ui.label("Loading...");
            }
            if matches!(self.namespace_status, LoadStatus::Error(_)) && self.kind.is_namespaced() {
                ui.colored_label(ui.visuals().error_fg_color, "Namespaces unavailable");
            }
            if namespace_changed {
                requests
                    .watches
                    .push(self.request_resource_watch(cluster_id.clone()));
            }
        });
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
                let action = show_network_table(ui, self.kind, &self.rows, indices);
                self.apply_table_action(action);
            }
        }
    }

    fn apply_table_action(&mut self, action: Option<NetworkTableAction>) {
        match action {
            Some(NetworkTableAction::Describe { key }) => {
                let Some((name, describe)) = self
                    .row_by_key(&key)
                    .map(|row| (row.cells[0].clone(), describe_from_row(self.kind, row)))
                else {
                    return;
                };
                self.describe_dialog = Some(NetworkDescribeDialog {
                    key,
                    name,
                    describe,
                });
            }
            Some(NetworkTableAction::View { key }) => {
                let Some((name, yaml)) = self
                    .row_by_key(&key)
                    .map(|row| (row.cells[0].clone(), full_manifest_yaml(&row.raw)))
                else {
                    return;
                };
                self.view_dialog = Some(NetworkViewDialog { key, name, yaml });
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
                        show_network_describe(ui, &dialog.describe);
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

    fn row_by_key(&self, key: &str) -> Option<&NetworkRow> {
        self.rows.iter().find(|row| row.key == key)
    }
}

impl NetworkResourceKind {
    fn id(self) -> &'static str {
        match self {
            Self::Services => "services",
            Self::EndpointSlices => "endpoint_slices",
            Self::Endpoints => "endpoints",
            Self::Ingresses => "ingresses",
            Self::IngressClasses => "ingress_classes",
            Self::NetworkPolicies => "network_policies",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::Services => "Services",
            Self::EndpointSlices => "EndpointSlices",
            Self::Endpoints => "Endpoints",
            Self::Ingresses => "Ingresses",
            Self::IngressClasses => "IngressClasses",
            Self::NetworkPolicies => "NetworkPolicies",
        }
    }

    fn is_namespaced(self) -> bool {
        !matches!(self, Self::IngressClasses)
    }

    fn columns(self) -> &'static [&'static str] {
        match self {
            Self::Services => &[
                "Name",
                "Namespace",
                "Type",
                "Cluster IP",
                "External IPs",
                "Ports",
                "Selector",
                "Age",
            ],
            Self::EndpointSlices => &[
                "Name",
                "Namespace",
                "Service",
                "Address Type",
                "Ports",
                "Endpoints",
                "Ready",
                "Age",
            ],
            Self::Endpoints => &[
                "Name",
                "Namespace",
                "Addresses",
                "Not Ready",
                "Ports",
                "Age",
            ],
            Self::Ingresses => &[
                "Name",
                "Namespace",
                "Class",
                "Hosts",
                "Address",
                "TLS",
                "Rules",
                "Age",
            ],
            Self::IngressClasses => &["Name", "Controller", "Parameters", "Default", "Age"],
            Self::NetworkPolicies => &[
                "Name",
                "Namespace",
                "Pod Selector",
                "Policy Types",
                "Ingress",
                "Egress",
                "Age",
            ],
        }
    }

    fn widths(self) -> &'static [f32] {
        match self {
            Self::Services => &[220.0, 150.0, 120.0, 140.0, 180.0, 240.0, 260.0, 90.0],
            Self::EndpointSlices => &[220.0, 150.0, 180.0, 120.0, 220.0, 110.0, 90.0, 90.0],
            Self::Endpoints => &[220.0, 150.0, 320.0, 110.0, 240.0, 90.0],
            Self::Ingresses => &[220.0, 150.0, 140.0, 280.0, 180.0, 120.0, 320.0, 90.0],
            Self::IngressClasses => &[220.0, 340.0, 280.0, 90.0, 90.0],
            Self::NetworkPolicies => &[220.0, 150.0, 260.0, 180.0, 90.0, 90.0, 90.0],
        }
    }

    fn load_kind(self, namespace: Option<String>) -> ResourceLoadKind {
        match self {
            Self::Services => ResourceLoadKind::Services { namespace },
            Self::EndpointSlices => ResourceLoadKind::EndpointSlices { namespace },
            Self::Endpoints => ResourceLoadKind::Endpoints { namespace },
            Self::Ingresses => ResourceLoadKind::Ingresses { namespace },
            Self::IngressClasses => ResourceLoadKind::IngressClasses,
            Self::NetworkPolicies => ResourceLoadKind::NetworkPolicies { namespace },
        }
    }

    fn matches_load_kind(self, kind: &ResourceLoadKind) -> bool {
        matches!(
            (self, kind),
            (Self::Services, ResourceLoadKind::Services { .. })
                | (
                    Self::EndpointSlices,
                    ResourceLoadKind::EndpointSlices { .. }
                )
                | (Self::Endpoints, ResourceLoadKind::Endpoints { .. })
                | (Self::Ingresses, ResourceLoadKind::Ingresses { .. })
                | (Self::IngressClasses, ResourceLoadKind::IngressClasses)
                | (
                    Self::NetworkPolicies,
                    ResourceLoadKind::NetworkPolicies { .. }
                )
        )
    }

    fn rows_from_list(self, items: &[ResourceSummary]) -> Vec<NetworkRow> {
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

fn show_network_table(
    ui: &mut egui::Ui,
    kind: NetworkResourceKind,
    rows: &[NetworkRow],
    row_indices: Vec<usize>,
) -> Option<NetworkTableAction> {
    let row_height = ui.spacing().interact_size.y;
    let widths = kind.widths();
    let table_width = widths.iter().sum::<f32>()
        + ui.spacing().item_spacing.x * widths.len().saturating_sub(1) as f32;
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
            for width in widths {
                table = table.column(Column::exact(*width));
            }
            table
                .header(row_height, |mut header| {
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
                        for cell in &row.cells {
                            table_row.col(|ui| {
                                ui.label(cell);
                            });
                        }
                        table_row.response().context_menu(|ui| {
                            if ui
                                .button(format!("{} Describe", egui_phosphor::regular::INFO))
                                .clicked()
                            {
                                action = Some(NetworkTableAction::Describe {
                                    key: row.key.clone(),
                                });
                                ui.close();
                            }
                            if ui
                                .button(format!("{} View", egui_phosphor::regular::EYE))
                                .clicked()
                            {
                                action = Some(NetworkTableAction::View {
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

fn row_matches_search(row: &NetworkRow, search_text: &str) -> bool {
    let needle = search_text.trim().to_lowercase();
    needle.is_empty() || row.search_text.contains(&needle)
}

#[derive(Clone, Debug, PartialEq)]
struct NetworkRow {
    key: String,
    name: String,
    namespace: String,
    cells: Vec<String>,
    details: Vec<(String, String)>,
    search_text: String,
    raw: serde_json::Value,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum NetworkTableAction {
    Describe { key: String },
    View { key: String },
}

#[derive(Clone, Debug, PartialEq)]
struct NetworkDescribeDialog {
    key: String,
    name: String,
    describe: NetworkDescribe,
}

#[derive(Clone, Debug, PartialEq)]
struct NetworkViewDialog {
    key: String,
    name: String,
    yaml: String,
}

#[derive(Clone, Debug, PartialEq)]
struct NetworkDescribe {
    title: &'static str,
    summary: Vec<(String, String)>,
    details: Vec<(String, String)>,
    labels: String,
    annotations: String,
    raw_yaml: String,
}

fn row_from_summary(kind: NetworkResourceKind, summary: &ResourceSummary) -> NetworkRow {
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
        NetworkResourceKind::Services => service_cells(raw, &name, &namespace, &age),
        NetworkResourceKind::EndpointSlices => endpoint_slice_cells(raw, &name, &namespace, &age),
        NetworkResourceKind::Endpoints => endpoints_cells(raw, &name, &namespace, &age),
        NetworkResourceKind::Ingresses => ingress_cells(raw, &name, &namespace, &age),
        NetworkResourceKind::IngressClasses => ingress_class_cells(raw, &name, &age),
        NetworkResourceKind::NetworkPolicies => network_policy_cells(raw, &name, &namespace, &age),
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
    NetworkRow {
        key,
        name,
        namespace,
        cells,
        details,
        search_text,
        raw: summary.raw.clone(),
    }
}

fn service_cells(raw: &serde_json::Value, name: &str, namespace: &str, age: &str) -> Vec<String> {
    vec![
        name.to_owned(),
        namespace.to_owned(),
        value_str(raw, &["spec", "type"])
            .unwrap_or("ClusterIP")
            .to_owned(),
        value_str(raw, &["spec", "clusterIP"])
            .unwrap_or("N/A")
            .to_owned(),
        service_external_ips(raw),
        service_ports(raw),
        resource_map(raw.pointer("/spec/selector")).unwrap_or_else(|| "N/A".to_owned()),
        age.to_owned(),
    ]
}

fn endpoint_slice_cells(
    raw: &serde_json::Value,
    name: &str,
    namespace: &str,
    age: &str,
) -> Vec<String> {
    let endpoints = array_len(raw.pointer("/endpoints"));
    vec![
        name.to_owned(),
        namespace.to_owned(),
        value_str(raw, &["metadata", "labels", "kubernetes.io/service-name"])
            .unwrap_or("N/A")
            .to_owned(),
        value_str(raw, &["addressType"]).unwrap_or("N/A").to_owned(),
        endpoint_slice_ports(raw),
        endpoints.to_string(),
        ready_endpoint_count(raw).to_string(),
        age.to_owned(),
    ]
}

fn endpoints_cells(raw: &serde_json::Value, name: &str, namespace: &str, age: &str) -> Vec<String> {
    vec![
        name.to_owned(),
        namespace.to_owned(),
        endpoint_addresses(raw, "/subsets", "addresses"),
        endpoint_addresses(raw, "/subsets", "notReadyAddresses"),
        endpoints_ports(raw),
        age.to_owned(),
    ]
}

fn ingress_cells(raw: &serde_json::Value, name: &str, namespace: &str, age: &str) -> Vec<String> {
    vec![
        name.to_owned(),
        namespace.to_owned(),
        value_str(raw, &["spec", "ingressClassName"])
            .unwrap_or("N/A")
            .to_owned(),
        ingress_hosts(raw),
        ingress_addresses(raw),
        array_len(raw.pointer("/spec/tls")).to_string(),
        ingress_rules(raw),
        age.to_owned(),
    ]
}

fn ingress_class_cells(raw: &serde_json::Value, name: &str, age: &str) -> Vec<String> {
    vec![
        name.to_owned(),
        value_str(raw, &["spec", "controller"])
            .unwrap_or("N/A")
            .to_owned(),
        ingress_class_parameters(raw),
        value_str(
            raw,
            &[
                "metadata",
                "annotations",
                "ingressclass.kubernetes.io/is-default-class",
            ],
        )
        .unwrap_or("false")
        .to_owned(),
        age.to_owned(),
    ]
}

fn network_policy_cells(
    raw: &serde_json::Value,
    name: &str,
    namespace: &str,
    age: &str,
) -> Vec<String> {
    vec![
        name.to_owned(),
        namespace.to_owned(),
        selector(raw.pointer("/spec/podSelector")),
        string_array(raw.pointer("/spec/policyTypes")),
        array_len(raw.pointer("/spec/ingress")).to_string(),
        array_len(raw.pointer("/spec/egress")).to_string(),
        age.to_owned(),
    ]
}

fn describe_from_row(kind: NetworkResourceKind, row: &NetworkRow) -> NetworkDescribe {
    NetworkDescribe {
        title: kind.title(),
        summary: row.details.clone(),
        details: vec![
            (
                "Labels".to_owned(),
                resource_map(row.raw.pointer("/metadata/labels"))
                    .unwrap_or_else(|| "N/A".to_owned()),
            ),
            (
                "Annotations".to_owned(),
                resource_map(row.raw.pointer("/metadata/annotations"))
                    .unwrap_or_else(|| "N/A".to_owned()),
            ),
        ],
        labels: resource_map(row.raw.pointer("/metadata/labels"))
            .unwrap_or_else(|| "N/A".to_owned()),
        annotations: resource_map(row.raw.pointer("/metadata/annotations"))
            .unwrap_or_else(|| "N/A".to_owned()),
        raw_yaml: full_manifest_yaml(&row.raw),
    }
}

fn show_network_describe(ui: &mut egui::Ui, describe: &NetworkDescribe) {
    ui.heading(describe.title);
    ui.separator();
    egui::Grid::new("network_describe_summary")
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

fn service_external_ips(raw: &serde_json::Value) -> String {
    let mut values = string_array_values(raw.pointer("/spec/externalIPs"));
    if let Some(ip) = value_str(raw, &["status", "loadBalancer", "ingress", "0", "ip"]) {
        values.push(ip.to_owned());
    }
    if let Some(hostname) = value_str(raw, &["status", "loadBalancer", "ingress", "0", "hostname"])
    {
        values.push(hostname.to_owned());
    }
    if values.is_empty() {
        "N/A".to_owned()
    } else {
        values.join(", ")
    }
}

fn service_ports(raw: &serde_json::Value) -> String {
    ports_from_array(raw.pointer("/spec/ports"))
}

fn endpoint_slice_ports(raw: &serde_json::Value) -> String {
    ports_from_array(raw.pointer("/ports"))
}

fn endpoints_ports(raw: &serde_json::Value) -> String {
    let ports = raw
        .pointer("/subsets")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(|subset| {
            subset
                .get("ports")
                .and_then(serde_json::Value::as_array)
                .into_iter()
                .flatten()
        })
        .map(port_label)
        .collect::<Vec<_>>();
    fallback_join(ports)
}

fn ports_from_array(value: Option<&serde_json::Value>) -> String {
    fallback_join(
        value
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .map(port_label)
            .collect::<Vec<_>>(),
    )
}

fn port_label(port: &serde_json::Value) -> String {
    let name = value_str(port, &["name"]).unwrap_or("");
    let protocol = value_str(port, &["protocol"]).unwrap_or("TCP");
    let port_value = port
        .get("port")
        .and_then(serde_json::Value::as_i64)
        .map_or_else(|| "N/A".to_owned(), |value| value.to_string());
    if name.is_empty() {
        format!("{port_value}/{protocol}")
    } else {
        format!("{name}:{port_value}/{protocol}")
    }
}

fn ready_endpoint_count(raw: &serde_json::Value) -> usize {
    raw.pointer("/endpoints")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter(|endpoint| {
            endpoint
                .pointer("/conditions/ready")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(true)
        })
        .count()
}

fn endpoint_addresses(raw: &serde_json::Value, subset_path: &str, field: &str) -> String {
    let values = raw
        .pointer(subset_path)
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(|subset| {
            subset
                .get(field)
                .and_then(serde_json::Value::as_array)
                .into_iter()
                .flatten()
        })
        .filter_map(|address| value_str(address, &["ip"]))
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    fallback_join(values)
}

fn ingress_hosts(raw: &serde_json::Value) -> String {
    let mut hosts = raw
        .pointer("/spec/rules")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|rule| value_str(rule, &["host"]))
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    hosts.sort();
    hosts.dedup();
    fallback_join(hosts)
}

fn ingress_addresses(raw: &serde_json::Value) -> String {
    let values = raw
        .pointer("/status/loadBalancer/ingress")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            value_str(entry, &["ip"])
                .or_else(|| value_str(entry, &["hostname"]))
                .map(ToOwned::to_owned)
        })
        .collect::<Vec<_>>();
    fallback_join(values)
}

fn ingress_rules(raw: &serde_json::Value) -> String {
    let rules = raw
        .pointer("/spec/rules")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .map(|rule| {
            let host = value_str(rule, &["host"]).unwrap_or("*");
            let paths = rule
                .pointer("/http/paths")
                .and_then(serde_json::Value::as_array)
                .map_or(0, |paths| paths.len());
            format!("{host} ({paths} paths)")
        })
        .collect::<Vec<_>>();
    fallback_join(rules)
}

fn ingress_class_parameters(raw: &serde_json::Value) -> String {
    raw.pointer("/spec/parameters")
        .map(|value| {
            let kind = value_str(value, &["kind"]).unwrap_or("N/A");
            let name = value_str(value, &["name"]).unwrap_or("N/A");
            format!("{kind}/{name}")
        })
        .unwrap_or_else(|| "N/A".to_owned())
}

fn selector(value: Option<&serde_json::Value>) -> String {
    value
        .and_then(|value| resource_map(value.pointer("/matchLabels")))
        .unwrap_or_else(|| "N/A".to_owned())
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

fn string_array(value: Option<&serde_json::Value>) -> String {
    fallback_join(string_array_values(value))
}

fn string_array_values(value: Option<&serde_json::Value>) -> Vec<String> {
    value
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .map(ToOwned::to_owned)
        .collect()
}

fn array_len(value: Option<&serde_json::Value>) -> usize {
    value
        .and_then(serde_json::Value::as_array)
        .map_or(0, |array| array.len())
}

fn fallback_join(values: Vec<String>) -> String {
    if values.is_empty() {
        "N/A".to_owned()
    } else {
        values.join(", ")
    }
}

fn value_str<'a>(value: &'a serde_json::Value, path: &[&str]) -> Option<&'a str> {
    let mut current = value;
    for key in path {
        current = if let Ok(index) = key.parse::<usize>() {
            current.get(index)?
        } else {
            current.get(*key)?
        };
    }
    current.as_str()
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
    fn service_request_query_uses_selected_namespace() {
        let mut panel = NetworkResourcePanel::new(NetworkResourceKind::Services);
        panel.namespace_filter = Some("production".to_owned());
        let query = panel.request_resources(ClusterId::new("local")).query();
        assert_eq!(query.resource.plural, "services");
        assert_eq!(query.namespace.as_deref(), Some("production"));
    }

    #[test]
    fn ingress_class_request_query_is_cluster_scoped() {
        let mut panel = NetworkResourcePanel::new(NetworkResourceKind::IngressClasses);
        let query = panel.request_resources(ClusterId::new("local")).query();
        assert_eq!(query.resource.plural, "ingressclasses");
        assert_eq!(query.namespace, None);
        assert!(matches!(
            query.resource.scope,
            miku_core::ResourceScope::Cluster
        ));
    }

    #[test]
    fn network_rows_extract_common_fields() {
        let service = row_from_summary(NetworkResourceKind::Services, &service_summary());
        assert_eq!(service.cells[2], "LoadBalancer");
        assert_eq!(service.cells[3], "10.0.0.1");
        assert_eq!(service.cells[5], "http:80/TCP");
        assert_eq!(service.cells[6], "app=api");

        let slice = row_from_summary(
            NetworkResourceKind::EndpointSlices,
            &endpoint_slice_summary(),
        );
        assert_eq!(slice.cells[2], "api");
        assert_eq!(slice.cells[3], "IPv4");
        assert_eq!(slice.cells[5], "2");
        assert_eq!(slice.cells[6], "1");

        let ingress = row_from_summary(NetworkResourceKind::Ingresses, &ingress_summary());
        assert_eq!(ingress.cells[2], "nginx");
        assert_eq!(ingress.cells[3], "api.example.com");
        assert_eq!(ingress.cells[5], "1");

        let endpoints = row_from_summary(NetworkResourceKind::Endpoints, &endpoints_summary());
        assert_eq!(endpoints.cells[2], "10.1.1.1");
        assert_eq!(endpoints.cells[3], "10.1.1.2");
        assert_eq!(endpoints.cells[4], "http:80/TCP");

        let ingress_class = row_from_summary(
            NetworkResourceKind::IngressClasses,
            &ingress_class_summary(),
        );
        assert_eq!(ingress_class.cells[1], "k8s.io/ingress-nginx");
        assert_eq!(ingress_class.cells[2], "IngressParameters/nginx");
        assert_eq!(ingress_class.cells[3], "true");

        let policy = row_from_summary(
            NetworkResourceKind::NetworkPolicies,
            &network_policy_summary(),
        );
        assert_eq!(policy.cells[2], "app=api");
        assert_eq!(policy.cells[3], "Ingress, Egress");
        assert_eq!(policy.cells[4], "1");
        assert_eq!(policy.cells[5], "1");
    }

    #[test]
    fn network_rows_handle_missing_fields() {
        let row = row_from_summary(NetworkResourceKind::Services, &minimal_summary("Service"));
        assert_eq!(row.cells[2], "ClusterIP");
        assert_eq!(row.cells[3], "N/A");
        assert_eq!(row.cells[4], "N/A");
        assert_eq!(row.cells[5], "N/A");
        assert_eq!(row.cells[6], "N/A");
    }

    #[test]
    fn rows_sort_and_filter_case_insensitively() {
        let rows = NetworkResourceKind::Services.rows_from_list(&[
            service_summary_with_name("zeta", "worker"),
            service_summary_with_name("default", "api-b"),
            service_summary_with_name("default", "api-a"),
        ]);
        let keys = rows.iter().map(|row| row.key.as_str()).collect::<Vec<_>>();
        assert_eq!(keys, vec!["default/api-a", "default/api-b", "zeta/worker"]);
        assert_eq!(
            rows.iter()
                .filter(|row| row_matches_search(row, "LOADBALANCER"))
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
        let mut panel = NetworkResourcePanel::new(NetworkResourceKind::Services);
        let cluster_id = ClusterId::new("local");
        let first = panel.request_resource_watch(cluster_id.clone());
        let second = panel.request_resource_watch(cluster_id);
        panel.apply_event(ResourceUiEvent::ResourceWatchUpdated {
            request: first,
            result: Ok(miku_api::ResourceEvent::Snapshot(ResourceList {
                items: vec![service_summary_with_name("default", "stale")],
                continue_token: None,
            })),
        });
        assert!(panel.rows.is_empty());
        panel.apply_event(ResourceUiEvent::ResourceWatchUpdated {
            request: second,
            result: Ok(miku_api::ResourceEvent::Snapshot(ResourceList {
                items: vec![service_summary()],
                continue_token: None,
            })),
        });
        assert_eq!(panel.rows[0].name, "api");
    }

    #[test]
    fn namespace_watch_events_update_namespaced_selector() {
        let mut panel = NetworkResourcePanel::new(NetworkResourceKind::NetworkPolicies);
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

    fn service_summary() -> ResourceSummary {
        service_summary_with_name("default", "api")
    }

    fn service_summary_with_name(namespace: &str, name: &str) -> ResourceSummary {
        ResourceSummary {
            name: name.to_owned(),
            namespace: Some(namespace.to_owned()),
            kind: "Service".to_owned(),
            status: None,
            raw: serde_json::json!({
                "metadata": {"name": name, "namespace": namespace, "creationTimestamp": "2026-05-18T10:00:00Z"},
                "spec": {
                    "type": "LoadBalancer",
                    "clusterIP": "10.0.0.1",
                    "ports": [{"name": "http", "port": 80, "protocol": "TCP"}],
                    "selector": {"app": "api"}
                },
                "status": {"loadBalancer": {"ingress": [{"ip": "203.0.113.10"}]}}
            }),
        }
    }

    fn endpoint_slice_summary() -> ResourceSummary {
        ResourceSummary {
            name: "api-abc".to_owned(),
            namespace: Some("default".to_owned()),
            kind: "EndpointSlice".to_owned(),
            status: None,
            raw: serde_json::json!({
                "metadata": {
                    "name": "api-abc",
                    "namespace": "default",
                    "labels": {"kubernetes.io/service-name": "api"}
                },
                "addressType": "IPv4",
                "ports": [{"name": "http", "port": 80}],
                "endpoints": [
                    {"addresses": ["10.1.1.1"], "conditions": {"ready": true}},
                    {"addresses": ["10.1.1.2"], "conditions": {"ready": false}}
                ]
            }),
        }
    }

    fn ingress_summary() -> ResourceSummary {
        ResourceSummary {
            name: "api".to_owned(),
            namespace: Some("default".to_owned()),
            kind: "Ingress".to_owned(),
            status: None,
            raw: serde_json::json!({
                "metadata": {"name": "api", "namespace": "default"},
                "spec": {
                    "ingressClassName": "nginx",
                    "tls": [{"hosts": ["api.example.com"]}],
                    "rules": [{"host": "api.example.com", "http": {"paths": [{"path": "/"}]}}]
                }
            }),
        }
    }

    fn endpoints_summary() -> ResourceSummary {
        ResourceSummary {
            name: "api".to_owned(),
            namespace: Some("default".to_owned()),
            kind: "Endpoints".to_owned(),
            status: None,
            raw: serde_json::json!({
                "metadata": {"name": "api", "namespace": "default"},
                "subsets": [{
                    "addresses": [{"ip": "10.1.1.1"}],
                    "notReadyAddresses": [{"ip": "10.1.1.2"}],
                    "ports": [{"name": "http", "port": 80, "protocol": "TCP"}]
                }]
            }),
        }
    }

    fn ingress_class_summary() -> ResourceSummary {
        ResourceSummary {
            name: "nginx".to_owned(),
            namespace: None,
            kind: "IngressClass".to_owned(),
            status: None,
            raw: serde_json::json!({
                "metadata": {
                    "name": "nginx",
                    "annotations": {"ingressclass.kubernetes.io/is-default-class": "true"}
                },
                "spec": {
                    "controller": "k8s.io/ingress-nginx",
                    "parameters": {"kind": "IngressParameters", "name": "nginx"}
                }
            }),
        }
    }

    fn network_policy_summary() -> ResourceSummary {
        ResourceSummary {
            name: "api".to_owned(),
            namespace: Some("default".to_owned()),
            kind: "NetworkPolicy".to_owned(),
            status: None,
            raw: serde_json::json!({
                "metadata": {"name": "api", "namespace": "default"},
                "spec": {
                    "podSelector": {"matchLabels": {"app": "api"}},
                    "policyTypes": ["Ingress", "Egress"],
                    "ingress": [{}],
                    "egress": [{}]
                }
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
}
