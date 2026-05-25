use std::collections::BTreeSet;

use eframe::egui;
use miku_core::ResourceRef;

use super::ResourceYamlEditDialog;
use crate::resource_panel::{ResourceActionKind, ResourceActionRequest, ResourceDeleteTarget};

pub(in crate::resource_panel) const SELECT_COLUMN_WIDTH: f32 = 36.0;

#[derive(Clone, Debug)]
pub(in crate::resource_panel) struct ResourceMetadata {
    pub(in crate::resource_panel) id: String,
    pub(in crate::resource_panel) title: String,
    pub(in crate::resource_panel) api_version: String,
    pub(in crate::resource_panel) kind: String,
    pub(in crate::resource_panel) resource: ResourceRef,
    pub(in crate::resource_panel) namespaced: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub(in crate::resource_panel) struct GenericCreateDialog {
    pub(in crate::resource_panel) yaml: String,
    pub(in crate::resource_panel) parse_error: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub(in crate::resource_panel) struct GenericBatchDeleteDialog {
    pub(in crate::resource_panel) targets: Vec<ResourceDeleteTarget>,
}

#[derive(Clone, Debug, PartialEq)]
pub(in crate::resource_panel) struct ResourceRowTarget {
    pub(in crate::resource_panel) key: String,
    pub(in crate::resource_panel) namespace: Option<String>,
    pub(in crate::resource_panel) name: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(in crate::resource_panel) struct ParsedResourceApply {
    pub(in crate::resource_panel) namespace: Option<String>,
    pub(in crate::resource_panel) name: String,
    pub(in crate::resource_panel) manifest: serde_json::Value,
}

pub(in crate::resource_panel) fn default_resource_yaml(
    metadata: ResourceMetadata,
    namespace: Option<&str>,
) -> String {
    let namespace = namespace.unwrap_or("default");
    if metadata.namespaced {
        format!(
            r#"apiVersion: {api_version}
kind: {kind}
metadata:
  name: example-{id}
  namespace: {namespace}
"#,
            api_version = metadata.api_version,
            kind = metadata.kind,
            id = metadata.id.replace('_', "-"),
        )
    } else {
        format!(
            r#"apiVersion: {api_version}
kind: {kind}
metadata:
  name: example-{id}
"#,
            api_version = metadata.api_version,
            kind = metadata.kind,
            id = metadata.id.replace('_', "-"),
        )
    }
}

pub(in crate::resource_panel) fn parse_resource_apply_yaml(
    yaml: &str,
    namespaced: bool,
    namespace_default: Option<&str>,
) -> Result<ParsedResourceApply, String> {
    let manifest =
        serde_yaml::from_str::<serde_json::Value>(yaml).map_err(|error| error.to_string())?;
    let name = value_str(&manifest, &["metadata", "name"])
        .filter(|name| !name.trim().is_empty())
        .ok_or_else(|| "metadata.name is required".to_owned())?
        .to_owned();
    let namespace = if namespaced {
        value_str(&manifest, &["metadata", "namespace"])
            .filter(|namespace| !namespace.trim().is_empty())
            .or(namespace_default)
            .map(ToOwned::to_owned)
    } else {
        None
    };

    Ok(ParsedResourceApply {
        namespace,
        name,
        manifest,
    })
}

pub(in crate::resource_panel) fn visible_keys<'a>(
    targets: impl IntoIterator<Item = &'a ResourceRowTarget>,
) -> BTreeSet<String> {
    targets
        .into_iter()
        .map(|target| target.key.clone())
        .collect()
}

pub(in crate::resource_panel) fn selected_delete_targets<'a>(
    targets: impl IntoIterator<Item = &'a ResourceRowTarget>,
    selected_rows: &BTreeSet<String>,
) -> Vec<ResourceDeleteTarget> {
    targets
        .into_iter()
        .filter(|target| selected_rows.contains(&target.key))
        .map(|target| ResourceDeleteTarget {
            namespace: target.namespace.clone(),
            name: target.name.clone(),
        })
        .collect()
}

pub(in crate::resource_panel) fn show_row_selection_checkbox(
    ui: &mut egui::Ui,
    selected_rows: &mut BTreeSet<String>,
    key: &str,
) -> bool {
    let mut selected = selected_rows.contains(key);
    if !ui.checkbox(&mut selected, "").changed() {
        return false;
    }

    if selected {
        selected_rows.insert(key.to_owned());
    } else {
        selected_rows.remove(key);
    }
    true
}

pub(in crate::resource_panel) struct ResourceCreateDialogInput<'a> {
    pub(in crate::resource_panel) metadata: ResourceMetadata,
    pub(in crate::resource_panel) dialog: &'a mut GenericCreateDialog,
    pub(in crate::resource_panel) action_error: Option<&'a str>,
    pub(in crate::resource_panel) action_in_flight: bool,
    pub(in crate::resource_panel) namespace_default: Option<&'a str>,
}

#[derive(Clone, Debug, PartialEq)]
pub(in crate::resource_panel) enum ResourceCreateDialogResponse {
    None,
    Cancel,
    Apply(ParsedResourceApply),
}

pub(in crate::resource_panel) fn show_resource_create_dialog(
    ctx: &egui::Context,
    input: ResourceCreateDialogInput<'_>,
) -> ResourceCreateDialogResponse {
    let response = ResourceYamlEditDialog {
        id: egui::Id::new((&input.metadata.id, "create-dialog")),
        title: format!("Create {}", input.metadata.kind),
        yaml: &mut input.dialog.yaml,
        error: input.action_error.or(input.dialog.parse_error.as_deref()),
        save_enabled: !input.action_in_flight,
        save_label: "Confirm",
    }
    .show(ctx);

    if response.cancel_clicked {
        return ResourceCreateDialogResponse::Cancel;
    }
    if !response.save_clicked {
        return ResourceCreateDialogResponse::None;
    }

    match parse_resource_apply_yaml(
        &input.dialog.yaml,
        input.metadata.namespaced,
        input.namespace_default,
    ) {
        Ok(parsed) => {
            input.dialog.parse_error = None;
            ResourceCreateDialogResponse::Apply(parsed)
        }
        Err(error) => {
            input.dialog.parse_error = Some(error);
            ResourceCreateDialogResponse::None
        }
    }
}

pub(in crate::resource_panel) struct ResourceBatchDeleteDialogInput<'a> {
    pub(in crate::resource_panel) metadata: ResourceMetadata,
    pub(in crate::resource_panel) targets: &'a [ResourceDeleteTarget],
    pub(in crate::resource_panel) action_error: Option<&'a str>,
    pub(in crate::resource_panel) action_in_flight: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::resource_panel) enum ResourceDeleteDialogResponse {
    None,
    Cancel,
    Delete,
}

pub(in crate::resource_panel) fn show_resource_batch_delete_dialog(
    ctx: &egui::Context,
    input: ResourceBatchDeleteDialogInput<'_>,
) -> ResourceDeleteDialogResponse {
    let mut cancel_clicked = false;
    let mut delete_clicked = false;
    egui::Window::new(format!("Delete selected {}", input.metadata.title))
        .id(egui::Id::new((&input.metadata.id, "batch-delete-dialog")))
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            if let Some(error) = input.action_error {
                ui.colored_label(ui.visuals().error_fg_color, error);
                ui.separator();
            }
            ui.label(format!(
                "Delete {} selected {}?",
                input.targets.len(),
                input.metadata.title
            ));
            ui.separator();
            egui::ScrollArea::vertical()
                .id_salt((input.metadata.id, "batch-delete-targets"))
                .max_height(160.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    for target in input.targets {
                        if let Some(namespace) = target.namespace.as_deref() {
                            ui.label(format!("{namespace}/{}", target.name));
                        } else {
                            ui.label(&target.name);
                        }
                    }
                });
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    cancel_clicked = true;
                }
                let delete_text =
                    egui::RichText::new(format!("{} Delete", egui_phosphor::regular::TRASH))
                        .color(ui.visuals().error_fg_color);
                if ui
                    .add_enabled(!input.action_in_flight, egui::Button::new(delete_text))
                    .clicked()
                {
                    delete_clicked = true;
                }
            });
        });

    if cancel_clicked {
        ResourceDeleteDialogResponse::Cancel
    } else if delete_clicked {
        ResourceDeleteDialogResponse::Delete
    } else {
        ResourceDeleteDialogResponse::None
    }
}

pub(in crate::resource_panel) fn apply_resource_request(
    request_id: u64,
    cluster_id: miku_core::ClusterId,
    metadata: ResourceMetadata,
    parsed: ParsedResourceApply,
) -> ResourceActionRequest {
    ResourceActionRequest {
        request_id,
        cluster_id,
        kind: ResourceActionKind::ApplyResource {
            resource: metadata.resource,
            namespace: parsed.namespace,
            name: parsed.name,
            manifest: parsed.manifest,
        },
    }
}

pub(in crate::resource_panel) fn batch_delete_resource_request(
    request_id: u64,
    cluster_id: miku_core::ClusterId,
    metadata: ResourceMetadata,
    targets: Vec<ResourceDeleteTarget>,
) -> ResourceActionRequest {
    ResourceActionRequest {
        request_id,
        cluster_id,
        kind: ResourceActionKind::BatchDeleteResources {
            resource: metadata.resource,
            targets,
        },
    }
}

fn value_str<'a>(raw: &'a serde_json::Value, path: &[&str]) -> Option<&'a str> {
    let mut value = raw;
    for segment in path {
        value = value.get(*segment)?;
    }
    value.as_str()
}
