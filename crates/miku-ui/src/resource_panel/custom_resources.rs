use eframe::egui;
use egui_extras::{Column, TableBuilder};
use miku_api::ResourceSummary;
use miku_core::ClusterId;

use super::{
    LoadStatus, ResourceLoadKind, ResourceLoadRequest, ResourcePanelRequests, ResourceUiEvent,
};
use crate::time::human_age_from_rfc3339;

const CUSTOM_RESOURCE_COLUMNS: &[&str] = &[
    "Name", "Group", "Kind", "Plural", "Scope", "Versions", "Age",
];
const CUSTOM_RESOURCE_COLUMN_WIDTHS: &[f32] = &[260.0, 180.0, 160.0, 160.0, 110.0, 220.0, 120.0];

#[derive(Clone, Debug, Default)]
pub(crate) struct CustomResourcesPanel {
    search_text: String,
    status: LoadStatus,
    rows: Vec<CustomResourceRow>,
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
    age: String,
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
        self.show_body(ui);
        requests
    }

    pub(crate) fn apply_event(&mut self, event: ResourceUiEvent) {
        let ResourceUiEvent::ResourcesLoaded { request, result } = event else {
            return;
        };
        if !matches!(request.kind, ResourceLoadKind::CustomResourceDefinitions)
            || self.row_request_id != Some(request.request_id)
        {
            return;
        }

        self.row_request_id = None;
        match result {
            Ok(list) => {
                self.rows = custom_resource_rows_from_items(&list.items);
                self.status = LoadStatus::Loaded;
            }
            Err(error) => self.status = LoadStatus::Error(error),
        }
    }

    fn reset_for_cluster_change(&mut self, cluster_id: &ClusterId) {
        if self.last_cluster_id.as_ref() == Some(cluster_id) {
            return;
        }

        self.last_cluster_id = Some(cluster_id.clone());
        self.search_text.clear();
        self.rows.clear();
        self.status = LoadStatus::Idle;
        self.row_request_id = None;
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
                    .hint_text("Search Custom Resources...")
                    .desired_width(280.0),
            );

            if ui
                .button(egui_phosphor::regular::ARROWS_CLOCKWISE)
                .on_hover_text("Refresh")
                .clicked()
            {
                requests
                    .loads
                    .push(self.request_crd_load(cluster_id.clone()));
            }

            ui.separator();
            ui.label(format!("{} items", self.filtered_row_indices().len()));
            if matches!(self.status, LoadStatus::Loading) {
                ui.label("Loading...");
            }
        });
    }

    fn show_body(&self, ui: &mut egui::Ui) {
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

                show_custom_resource_table(ui, &self.rows, row_indices);
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

    fn request_crd_load(&mut self, cluster_id: ClusterId) -> ResourceLoadRequest {
        self.next_request_id += 1;
        self.row_request_id = Some(self.next_request_id);
        self.status = LoadStatus::Loading;
        ResourceLoadRequest {
            request_id: self.next_request_id,
            cluster_id,
            kind: ResourceLoadKind::CustomResourceDefinitions,
        }
    }
}

fn show_custom_resource_table(
    ui: &mut egui::Ui,
    rows: &[CustomResourceRow],
    row_indices: Vec<usize>,
) {
    let row_height = ui.spacing().interact_size.y;
    let table_width: f32 = CUSTOM_RESOURCE_COLUMN_WIDTHS.iter().sum::<f32>()
        + ui.spacing().item_spacing.x
            * CUSTOM_RESOURCE_COLUMN_WIDTHS.len().saturating_sub(1) as f32;

    egui::ScrollArea::horizontal()
        .id_salt("custom_resource_table_horizontal")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.set_min_width(table_width);

            let mut table = TableBuilder::new(ui)
                .id_salt("custom_resource_table")
                .striped(true)
                .resizable(false)
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .min_scrolled_height(0.0);

            for width in CUSTOM_RESOURCE_COLUMN_WIDTHS {
                table = table.column(Column::exact(*width));
            }

            table
                .header(row_height, |mut header| {
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
                    });
                });
        });
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
        age: summary
            .raw
            .pointer("/metadata/creationTimestamp")
            .and_then(serde_json::Value::as_str)
            .and_then(human_age_from_rfc3339)
            .unwrap_or_else(|| "N/A".to_owned()),
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

fn value_str<'a>(value: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(serde_json::Value::as_str)
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
                        { "name": "v1", "served": true, "storage": true },
                        { "name": "v1beta1", "served": true, "storage": false },
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
        assert_eq!(rows[0].versions, "v1 (storage), v1beta1");
    }
}
