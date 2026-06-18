use eframe::egui::{self, TextWrapMode};

use super::map_view::{ResourceMapEntry, ResourceMapView};

pub(crate) const RESOURCE_DESCRIBE_LINE_WIDTH: f32 = 1080.0;

const RESOURCE_DESCRIBE_DIALOG_WIDTH: f32 = 860.0;
const RESOURCE_DESCRIBE_DIALOG_HEIGHT: f32 = 580.0;
const RESOURCE_DESCRIBE_CONTENT_HEIGHT: f32 = 520.0;
const RESOURCE_DESCRIBE_CONTENT_WIDTH: f32 = 1160.0;
const RESOURCE_DESCRIBE_SECTION_WIDTH: f32 = 1128.0;
const RESOURCE_DESCRIBE_FIELD_LABEL_WIDTH: f32 = 140.0;
const RESOURCE_DESCRIBE_FIELD_VALUE_WIDTH: f32 = 370.0;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DescribeField {
    pub(crate) label: String,
    pub(crate) value: String,
}

impl DescribeField {
    pub(crate) fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DescribeCondition {
    pub(crate) condition_type: String,
    pub(crate) status: String,
    pub(crate) reason: String,
    pub(crate) message: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ContainerTemplateDescribe {
    pub(crate) name: String,
    pub(crate) image: String,
    pub(crate) resources: Vec<DescribeField>,
    pub(crate) ports: String,
    pub(crate) env_count: String,
    pub(crate) volume_mounts: String,
    pub(crate) probes: Vec<DescribeField>,
}

pub(crate) fn show_resource_describe_window(
    ctx: &egui::Context,
    id: egui::Id,
    title: String,
    open: &mut bool,
    contents: impl FnOnce(&mut egui::Ui),
) {
    let content_id = id.with("content");
    egui::Window::new(title)
        .id(id)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .open(open)
        .collapsible(false)
        .fixed_size([
            RESOURCE_DESCRIBE_DIALOG_WIDTH,
            RESOURCE_DESCRIBE_DIALOG_HEIGHT,
        ])
        .show(ctx, |ui| {
            ui.set_width(RESOURCE_DESCRIBE_DIALOG_WIDTH);
            ui.set_height(RESOURCE_DESCRIBE_CONTENT_HEIGHT);
            egui::ScrollArea::both()
                .id_salt(content_id)
                .max_width(RESOURCE_DESCRIBE_DIALOG_WIDTH)
                .max_height(RESOURCE_DESCRIBE_CONTENT_HEIGHT)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_min_width(RESOURCE_DESCRIBE_CONTENT_WIDTH);
                    ui.style_mut().wrap_mode = Some(TextWrapMode::Extend);
                    contents(ui);
                });
        });
}

pub(crate) fn describe_group(
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
            ui.set_min_width(RESOURCE_DESCRIBE_SECTION_WIDTH);
            describe_subsection(ui, icon, title);
            ui.separator();
            contents(ui);
        });
}

pub(crate) fn describe_subsection(ui: &mut egui::Ui, icon: &str, title: &str) {
    ui.horizontal(|ui| {
        ui.label(icon);
        ui.strong(title);
    });
}

pub(crate) fn describe_fields(ui: &mut egui::Ui, fields: &[DescribeField]) {
    egui::Grid::new(ui.next_auto_id())
        .num_columns(4)
        .spacing([16.0, 4.0])
        .show(ui, |ui| {
            for chunk in fields.chunks(2) {
                for field in chunk {
                    ui.add_sized(
                        [RESOURCE_DESCRIBE_FIELD_LABEL_WIDTH, 0.0],
                        egui::Label::new(egui::RichText::new(&field.label).weak())
                            .wrap_mode(TextWrapMode::Extend),
                    );
                    non_wrapping_value(ui, &field.value, RESOURCE_DESCRIBE_FIELD_VALUE_WIDTH);
                }
                if chunk.len() == 1 {
                    ui.label("");
                    ui.label("");
                }
                ui.end_row();
            }
        });
}

pub(crate) fn non_wrapping_value(ui: &mut egui::Ui, value: &str, width: f32) {
    ui.add_sized(
        [width, 0.0],
        egui::Label::new(value)
            .wrap_mode(TextWrapMode::Extend)
            .selectable(true),
    );
}

pub(crate) fn describe_lines(ui: &mut egui::Ui, lines: &[String]) {
    if lines.is_empty() {
        non_wrapping_value(ui, "N/A", RESOURCE_DESCRIBE_LINE_WIDTH);
        return;
    }

    for line in lines {
        non_wrapping_value(ui, line, RESOURCE_DESCRIBE_LINE_WIDTH);
    }
}

pub(crate) fn describe_text_block(ui: &mut egui::Ui, id_salt: impl std::hash::Hash, value: &str) {
    egui::ScrollArea::both()
        .id_salt(id_salt)
        .max_height(180.0)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add(
                egui::Label::new(egui::RichText::new(value).monospace())
                    .wrap_mode(TextWrapMode::Extend)
                    .selectable(true),
            );
        });
}

pub(crate) fn describe_metadata_maps(
    ui: &mut egui::Ui,
    id_prefix: &'static str,
    labels: &[ResourceMapEntry],
    annotations: &[ResourceMapEntry],
) {
    let labels_id = format!("{id_prefix}-labels");
    ResourceMapView {
        id_salt: &labels_id,
        icon: egui_phosphor::regular::TAG,
        title: "Labels",
        entries: labels,
        empty_label: "No labels.",
    }
    .show(ui);
    ui.add_space(8.0);
    let annotations_id = format!("{id_prefix}-annotations");
    ResourceMapView {
        id_salt: &annotations_id,
        icon: egui_phosphor::regular::NOTE,
        title: "Annotations",
        entries: annotations,
        empty_label: "No annotations.",
    }
    .show(ui);
}

pub(crate) fn describe_conditions(
    ui: &mut egui::Ui,
    id_salt: &'static str,
    conditions: &[DescribeCondition],
) {
    if conditions.is_empty() {
        non_wrapping_value(ui, "N/A", RESOURCE_DESCRIBE_LINE_WIDTH);
        return;
    }

    egui::Grid::new(id_salt)
        .num_columns(4)
        .spacing([18.0, 4.0])
        .striped(true)
        .show(ui, |ui| {
            ui.strong("Type");
            ui.strong("Status");
            ui.strong("Reason");
            ui.strong("Message");
            ui.end_row();
            for condition in conditions {
                non_wrapping_value(ui, &condition.condition_type, 180.0);
                ui.colored_label(condition_color(ui, &condition.status), &condition.status);
                non_wrapping_value(ui, &condition.reason, 220.0);
                non_wrapping_value(ui, &condition.message, 520.0);
                ui.end_row();
            }
        });
}

pub(crate) fn describe_container_templates(
    ui: &mut egui::Ui,
    containers: &[ContainerTemplateDescribe],
) {
    if containers.is_empty() {
        non_wrapping_value(ui, "N/A", RESOURCE_DESCRIBE_LINE_WIDTH);
        return;
    }

    for (index, container) in containers.iter().enumerate() {
        if index > 0 {
            ui.separator();
        }
        show_container_template_describe(ui, container);
    }
}

pub(crate) fn describe_raw_manifest(ui: &mut egui::Ui, id_salt: &'static str, raw_yaml: &str) {
    describe_text_block(ui, id_salt, raw_yaml);
}

pub(crate) fn container_template_describes(
    raw: &serde_json::Value,
    containers_pointer: &str,
) -> Vec<ContainerTemplateDescribe> {
    raw.pointer(containers_pointer)
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .map(container_template_describe)
        .collect()
}

pub(crate) fn condition_describes(value: Option<&serde_json::Value>) -> Vec<DescribeCondition> {
    value
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .map(|condition| DescribeCondition {
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

pub(crate) fn resource_map_entries(value: Option<&serde_json::Value>) -> Vec<ResourceMapEntry> {
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

fn show_container_template_describe(ui: &mut egui::Ui, container: &ContainerTemplateDescribe) {
    ui.horizontal(|ui| {
        ui.strong(format!(
            "{} {}",
            egui_phosphor::regular::PACKAGE,
            container.name
        ));
        non_wrapping_value(ui, &container.image, 760.0);
    });

    ui.add_space(6.0);
    describe_subsection(ui, egui_phosphor::regular::GAUGE, "Resources");
    describe_fields(ui, &container.resources);

    ui.add_space(6.0);
    describe_fields(
        ui,
        &[
            DescribeField::new("Ports", &container.ports),
            DescribeField::new("Env", &container.env_count),
            DescribeField::new("Mounts", &container.volume_mounts),
        ],
    );

    ui.add_space(6.0);
    describe_fields(ui, &container.probes);
}

fn container_template_describe(container: &serde_json::Value) -> ContainerTemplateDescribe {
    ContainerTemplateDescribe {
        name: value_str(container, &["name"]).unwrap_or("N/A").to_owned(),
        image: value_str(container, &["image"]).unwrap_or("N/A").to_owned(),
        resources: vec![
            DescribeField::new(
                "CPU request",
                value_at_path(container, &["resources", "requests", "cpu"]),
            ),
            DescribeField::new(
                "CPU limit",
                value_at_path(container, &["resources", "limits", "cpu"]),
            ),
            DescribeField::new(
                "Memory request",
                value_at_path(container, &["resources", "requests", "memory"]),
            ),
            DescribeField::new(
                "Memory limit",
                value_at_path(container, &["resources", "limits", "memory"]),
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

pub(crate) fn condition_color(ui: &egui::Ui, status: &str) -> egui::Color32 {
    match status {
        "True" | "Available" | "Ready" | "Running" | "Complete" => {
            egui::Color32::from_rgb(46, 160, 67)
        }
        "False" | "Progressing" | "Waiting" | "Pending" | "Suspended" => {
            egui::Color32::from_rgb(191, 135, 0)
        }
        "Unknown" | "Terminated" | "Failed" | "Error" => ui.visuals().error_fg_color,
        _ => ui.visuals().text_color(),
    }
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
                .map(value_to_string)
                .unwrap_or_else(|| "N/A".to_owned());
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

fn value_at_path(value: &serde_json::Value, path: &[&str]) -> String {
    let mut current = value;
    for key in path {
        let Some(next) = current.get(*key) else {
            return "N/A".to_owned();
        };
        current = next;
    }
    value_to_string(current)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn container_template_describes_extracts_pod_template_details() {
        let raw = serde_json::json!({
            "spec": {
                "template": {
                    "spec": {
                        "containers": [
                            {
                                "name": "api",
                                "image": "ghcr.io/example/api:1.0.0",
                                "resources": {
                                    "requests": {"cpu": "250m", "memory": "128Mi"},
                                    "limits": {"cpu": "1", "memory": "512Mi"}
                                },
                                "ports": [
                                    {"name": "http", "containerPort": 8080, "protocol": "TCP"}
                                ],
                                "env": [{"name": "RUST_LOG", "value": "info"}],
                                "volumeMounts": [{"name": "config", "mountPath": "/etc/api"}],
                                "livenessProbe": {"httpGet": {"path": "/healthz", "port": 8080}}
                            }
                        ]
                    }
                }
            }
        });

        let containers = container_template_describes(&raw, "/spec/template/spec/containers");

        assert_eq!(containers.len(), 1);
        assert_eq!(containers[0].name, "api");
        assert_eq!(containers[0].image, "ghcr.io/example/api:1.0.0");
        assert_eq!(containers[0].resources[0].value, "250m");
        assert_eq!(containers[0].resources[1].value, "1");
        assert_eq!(containers[0].resources[2].value, "128Mi");
        assert_eq!(containers[0].resources[3].value, "512Mi");
        assert_eq!(containers[0].ports, "http:8080/TCP");
        assert_eq!(containers[0].env_count, "1 vars");
        assert_eq!(containers[0].volume_mounts, "config at /etc/api");
        assert!(containers[0].probes[0].value.contains("configured"));
        assert_eq!(containers[0].probes[1].value, "N/A");
    }

    #[test]
    fn container_template_describes_uses_stable_fallbacks() {
        let raw = serde_json::json!({
            "spec": {
                "template": {
                    "spec": {
                        "containers": [{"name": "worker"}]
                    }
                }
            }
        });

        let containers = container_template_describes(&raw, "/spec/template/spec/containers");

        assert_eq!(containers.len(), 1);
        assert_eq!(containers[0].name, "worker");
        assert_eq!(containers[0].image, "N/A");
        assert!(
            containers[0]
                .resources
                .iter()
                .all(|field| field.value == "N/A")
        );
        assert_eq!(containers[0].ports, "N/A");
        assert_eq!(containers[0].env_count, "0 vars");
        assert_eq!(containers[0].volume_mounts, "N/A");
        assert!(
            containers[0]
                .probes
                .iter()
                .all(|field| field.value == "N/A")
        );
    }

    #[test]
    fn resource_map_entries_sort_keys_and_stringify_values() {
        let raw = serde_json::json!({
            "metadata": {
                "labels": {
                    "tier": "api",
                    "enabled": true,
                    "replicas": 2
                }
            }
        });

        let entries = resource_map_entries(raw.pointer("/metadata/labels"));

        assert_eq!(entries[0], ResourceMapEntry::new("enabled", "true"));
        assert_eq!(entries[1], ResourceMapEntry::new("replicas", "2"));
        assert_eq!(entries[2], ResourceMapEntry::new("tier", "api"));
        assert!(resource_map_entries(raw.pointer("/metadata/annotations")).is_empty());
    }
}
