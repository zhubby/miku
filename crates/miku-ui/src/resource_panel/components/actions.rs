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
pub(in crate::resource_panel) struct GenericDeleteDialog {
    pub(in crate::resource_panel) target: ResourceDeleteTarget,
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
    if let Some(yaml) = default_builtin_resource_yaml(&metadata, namespace) {
        return yaml;
    }

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

fn default_builtin_resource_yaml(metadata: &ResourceMetadata, namespace: &str) -> Option<String> {
    let name = format!("example-{}", metadata.id.replace('_', "-"));
    let yaml = match metadata.kind.as_str() {
        "Deployment" | "ReplicaSet" => format!(
            r#"apiVersion: {api_version}
kind: {kind}
metadata:
  name: {name}
  namespace: {namespace}
spec:
  replicas: 1
  selector:
    matchLabels:
      app: {name}
  template:
    metadata:
      labels:
        app: {name}
    spec:
      containers:
        - name: app
          image: nginx:latest
"#,
            api_version = metadata.api_version,
            kind = metadata.kind
        ),
        "DaemonSet" => format!(
            r#"apiVersion: apps/v1
kind: DaemonSet
metadata:
  name: {name}
  namespace: {namespace}
spec:
  selector:
    matchLabels:
      app: {name}
  template:
    metadata:
      labels:
        app: {name}
    spec:
      containers:
        - name: app
          image: nginx:latest
"#
        ),
        "StatefulSet" => format!(
            r#"apiVersion: apps/v1
kind: StatefulSet
metadata:
  name: {name}
  namespace: {namespace}
spec:
  serviceName: {name}
  replicas: 1
  selector:
    matchLabels:
      app: {name}
  template:
    metadata:
      labels:
        app: {name}
    spec:
      containers:
        - name: app
          image: nginx:latest
"#
        ),
        "Job" => format!(
            r#"apiVersion: batch/v1
kind: Job
metadata:
  name: {name}
  namespace: {namespace}
spec:
  template:
    spec:
      restartPolicy: Never
      containers:
        - name: app
          image: busybox:latest
          command:
            - /bin/sh
            - -c
            - date
"#
        ),
        "Service" => format!(
            r#"apiVersion: v1
kind: Service
metadata:
  name: {name}
  namespace: {namespace}
spec:
  selector:
    app: {name}
  ports:
    - name: http
      port: 80
      targetPort: 80
"#
        ),
        "EndpointSlice" => format!(
            r#"apiVersion: discovery.k8s.io/v1
kind: EndpointSlice
metadata:
  name: {name}
  namespace: {namespace}
  labels:
    kubernetes.io/service-name: {name}
addressType: IPv4
ports:
  - name: http
    protocol: TCP
    port: 80
endpoints: []
"#
        ),
        "Ingress" => format!(
            r#"apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: {name}
  namespace: {namespace}
spec:
  rules:
    - http:
        paths:
          - path: /
            pathType: Prefix
            backend:
              service:
                name: {name}
                port:
                  number: 80
"#
        ),
        "NetworkPolicy" => format!(
            r#"apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: {name}
  namespace: {namespace}
spec:
  podSelector: {{}}
  policyTypes:
    - Ingress
"#
        ),
        "PersistentVolumeClaim" => format!(
            r#"apiVersion: v1
kind: PersistentVolumeClaim
metadata:
  name: {name}
  namespace: {namespace}
spec:
  accessModes:
    - ReadWriteOnce
  resources:
    requests:
      storage: 1Gi
"#
        ),
        "PersistentVolume" => format!(
            r#"apiVersion: v1
kind: PersistentVolume
metadata:
  name: {name}
spec:
  capacity:
    storage: 1Gi
  accessModes:
    - ReadWriteOnce
  persistentVolumeReclaimPolicy: Retain
  storageClassName: manual
  hostPath:
    path: /tmp/{name}
"#
        ),
        "StorageClass" => format!(
            r#"apiVersion: storage.k8s.io/v1
kind: StorageClass
metadata:
  name: {name}
provisioner: kubernetes.io/no-provisioner
volumeBindingMode: WaitForFirstConsumer
"#
        ),
        "HorizontalPodAutoscaler" => format!(
            r#"apiVersion: autoscaling/v2
kind: HorizontalPodAutoscaler
metadata:
  name: {name}
  namespace: {namespace}
spec:
  scaleTargetRef:
    apiVersion: apps/v1
    kind: Deployment
    name: {name}
  minReplicas: 1
  maxReplicas: 3
  metrics:
    - type: Resource
      resource:
        name: cpu
        target:
          type: Utilization
          averageUtilization: 80
"#
        ),
        "PodDisruptionBudget" => format!(
            r#"apiVersion: policy/v1
kind: PodDisruptionBudget
metadata:
  name: {name}
  namespace: {namespace}
spec:
  minAvailable: 1
  selector:
    matchLabels:
      app: {name}
"#
        ),
        "PriorityClass" => format!(
            r#"apiVersion: scheduling.k8s.io/v1
kind: PriorityClass
metadata:
  name: {name}
value: 1000
globalDefault: false
description: Example priority class
"#
        ),
        "RuntimeClass" => format!(
            r#"apiVersion: node.k8s.io/v1
kind: RuntimeClass
metadata:
  name: {name}
handler: runc
"#
        ),
        "Lease" => format!(
            r#"apiVersion: coordination.k8s.io/v1
kind: Lease
metadata:
  name: {name}
  namespace: {namespace}
spec:
  holderIdentity: {name}
  leaseDurationSeconds: 30
"#
        ),
        "ResourceQuota" => format!(
            r#"apiVersion: v1
kind: ResourceQuota
metadata:
  name: {name}
  namespace: {namespace}
spec:
  hard:
    pods: "10"
    requests.cpu: "2"
    requests.memory: 2Gi
"#
        ),
        "LimitRange" => format!(
            r#"apiVersion: v1
kind: LimitRange
metadata:
  name: {name}
  namespace: {namespace}
spec:
  limits:
    - type: Container
      default:
        cpu: 500m
        memory: 512Mi
      defaultRequest:
        cpu: 100m
        memory: 128Mi
"#
        ),
        "Role" | "ClusterRole" => format!(
            r#"apiVersion: rbac.authorization.k8s.io/v1
kind: {kind}
metadata:
  name: {name}
{namespace_line}rules:
  - apiGroups: [""]
    resources: ["pods"]
    verbs: ["get", "list"]
"#,
            kind = metadata.kind,
            namespace_line = namespace_line(metadata.namespaced, namespace)
        ),
        "RoleBinding" | "ClusterRoleBinding" => format!(
            r#"apiVersion: rbac.authorization.k8s.io/v1
kind: {kind}
metadata:
  name: {name}
{namespace_line}subjects:
  - kind: ServiceAccount
    name: default
    namespace: {namespace}
roleRef:
  apiGroup: rbac.authorization.k8s.io
  kind: {role_kind}
  name: {role_name}
"#,
            kind = metadata.kind,
            namespace_line = namespace_line(metadata.namespaced, namespace),
            role_kind = if metadata.namespaced {
                "Role"
            } else {
                "ClusterRole"
            },
            role_name = if metadata.namespaced { &name } else { "view" }
        ),
        "CustomResourceDefinition" => format!(
            r#"apiVersion: apiextensions.k8s.io/v1
kind: CustomResourceDefinition
metadata:
  name: examples.miku.io
spec:
  group: miku.io
  scope: Namespaced
  names:
    plural: examples
    singular: example
    kind: Example
  versions:
    - name: v1
      served: true
      storage: true
      schema:
        openAPIV3Schema:
          type: object
"#
        ),
        _ => return None,
    };
    Some(yaml)
}

fn namespace_line(namespaced: bool, namespace: &str) -> String {
    if namespaced {
        format!("  namespace: {namespace}\n")
    } else {
        String::new()
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

pub(in crate::resource_panel) struct ResourceDeleteDialogInput<'a> {
    pub(in crate::resource_panel) metadata: ResourceMetadata,
    pub(in crate::resource_panel) target: &'a ResourceDeleteTarget,
    pub(in crate::resource_panel) action_error: Option<&'a str>,
    pub(in crate::resource_panel) action_in_flight: bool,
}

pub(in crate::resource_panel) fn show_resource_delete_dialog(
    ctx: &egui::Context,
    input: ResourceDeleteDialogInput<'_>,
) -> ResourceDeleteDialogResponse {
    let mut cancel_clicked = false;
    let mut delete_clicked = false;
    egui::Window::new(format!("Delete {}", input.metadata.kind))
        .id(egui::Id::new((&input.metadata.id, "delete-dialog")))
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            if let Some(error) = input.action_error {
                ui.colored_label(ui.visuals().error_fg_color, error);
                ui.separator();
            }
            let target_label = resource_target_label(input.target);
            ui.label(format!("Delete {} {target_label}?", input.metadata.kind));
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
                        ui.label(resource_target_label(target));
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

pub(in crate::resource_panel) fn delete_resource_request(
    request_id: u64,
    cluster_id: miku_core::ClusterId,
    metadata: ResourceMetadata,
    target: ResourceDeleteTarget,
) -> ResourceActionRequest {
    ResourceActionRequest {
        request_id,
        cluster_id,
        kind: ResourceActionKind::DeleteResource {
            resource: metadata.resource,
            namespace: target.namespace,
            name: target.name,
        },
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

pub(in crate::resource_panel) fn patch_resource_request(
    request_id: u64,
    cluster_id: miku_core::ClusterId,
    metadata: ResourceMetadata,
    target: ResourceDeleteTarget,
    patch: serde_json::Value,
) -> ResourceActionRequest {
    ResourceActionRequest {
        request_id,
        cluster_id,
        kind: ResourceActionKind::PatchResource {
            resource: metadata.resource,
            namespace: target.namespace,
            name: target.name,
            patch,
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

fn resource_target_label(target: &ResourceDeleteTarget) -> String {
    if let Some(namespace) = target.namespace.as_deref() {
        format!("{namespace}/{}", target.name)
    } else {
        target.name.clone()
    }
}

fn value_str<'a>(raw: &'a serde_json::Value, path: &[&str]) -> Option<&'a str> {
    let mut value = raw;
    for segment in path {
        value = value.get(*segment)?;
    }
    value.as_str()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_builtin_resource_yamls_parse_and_include_required_specs() {
        for metadata in builtin_template_metadata() {
            let yaml = default_resource_yaml(metadata.clone(), Some("production"));
            let parsed =
                parse_resource_apply_yaml(&yaml, metadata.namespaced, Some("fallback")).unwrap();

            assert_eq!(parsed.manifest["apiVersion"], metadata.api_version);
            assert_eq!(parsed.manifest["kind"], metadata.kind);
            if metadata.namespaced {
                assert_eq!(parsed.namespace.as_deref(), Some("production"));
            } else {
                assert_eq!(parsed.namespace, None);
            }
            assert_builtin_required_fields(&metadata.kind, &parsed.manifest);
        }
    }

    fn builtin_template_metadata() -> Vec<ResourceMetadata> {
        vec![
            metadata("deployment", "apps/v1", "Deployment", true),
            metadata("daemon_set", "apps/v1", "DaemonSet", true),
            metadata("stateful_set", "apps/v1", "StatefulSet", true),
            metadata("replica_set", "apps/v1", "ReplicaSet", true),
            metadata("job", "batch/v1", "Job", true),
            metadata("service", "v1", "Service", true),
            metadata(
                "endpoint_slice",
                "discovery.k8s.io/v1",
                "EndpointSlice",
                true,
            ),
            metadata("ingress", "networking.k8s.io/v1", "Ingress", true),
            metadata(
                "network_policy",
                "networking.k8s.io/v1",
                "NetworkPolicy",
                true,
            ),
            metadata(
                "persistent_volume_claim",
                "v1",
                "PersistentVolumeClaim",
                true,
            ),
            metadata("persistent_volume", "v1", "PersistentVolume", false),
            metadata("storage_class", "storage.k8s.io/v1", "StorageClass", false),
            metadata(
                "horizontal_pod_autoscaler",
                "autoscaling/v2",
                "HorizontalPodAutoscaler",
                true,
            ),
            metadata(
                "pod_disruption_budget",
                "policy/v1",
                "PodDisruptionBudget",
                true,
            ),
            metadata(
                "priority_class",
                "scheduling.k8s.io/v1",
                "PriorityClass",
                false,
            ),
            metadata("runtime_class", "node.k8s.io/v1", "RuntimeClass", false),
            metadata("lease", "coordination.k8s.io/v1", "Lease", true),
            metadata("resource_quota", "v1", "ResourceQuota", true),
            metadata("limit_range", "v1", "LimitRange", true),
            metadata("role", "rbac.authorization.k8s.io/v1", "Role", true),
            metadata(
                "cluster_role",
                "rbac.authorization.k8s.io/v1",
                "ClusterRole",
                false,
            ),
            metadata(
                "role_binding",
                "rbac.authorization.k8s.io/v1",
                "RoleBinding",
                true,
            ),
            metadata(
                "cluster_role_binding",
                "rbac.authorization.k8s.io/v1",
                "ClusterRoleBinding",
                false,
            ),
            metadata(
                "custom_resource_definition",
                "apiextensions.k8s.io/v1",
                "CustomResourceDefinition",
                false,
            ),
        ]
    }

    fn metadata(id: &str, api_version: &str, kind: &str, namespaced: bool) -> ResourceMetadata {
        ResourceMetadata {
            id: id.to_owned(),
            title: format!("{kind}s"),
            api_version: api_version.to_owned(),
            kind: kind.to_owned(),
            resource: ResourceRef::core(api_version, &format!("{}s", kind.to_lowercase())),
            namespaced,
        }
    }

    fn assert_builtin_required_fields(kind: &str, manifest: &serde_json::Value) {
        match kind {
            "Deployment" | "DaemonSet" | "ReplicaSet" => {
                assert!(manifest.pointer("/spec/selector/matchLabels").is_some());
                assert!(
                    manifest
                        .pointer("/spec/template/spec/containers/0/image")
                        .is_some()
                );
            }
            "StatefulSet" => {
                assert!(manifest.pointer("/spec/serviceName").is_some());
                assert!(
                    manifest
                        .pointer("/spec/template/spec/containers/0/image")
                        .is_some()
                );
            }
            "Job" => {
                assert_eq!(
                    manifest.pointer("/spec/template/spec/restartPolicy"),
                    Some(&serde_json::Value::String("Never".to_owned()))
                );
                assert!(
                    manifest
                        .pointer("/spec/template/spec/containers/0/image")
                        .is_some()
                );
            }
            "Service" => assert!(manifest.pointer("/spec/ports/0/port").is_some()),
            "EndpointSlice" => {
                assert!(manifest.pointer("/addressType").is_some());
                assert!(manifest.pointer("/endpoints").is_some());
            }
            "Ingress" => assert!(
                manifest
                    .pointer("/spec/rules/0/http/paths/0/backend")
                    .is_some()
            ),
            "NetworkPolicy" => assert!(manifest.pointer("/spec/podSelector").is_some()),
            "PersistentVolumeClaim" => {
                assert!(
                    manifest
                        .pointer("/spec/resources/requests/storage")
                        .is_some()
                );
            }
            "PersistentVolume" => {
                assert!(manifest.pointer("/spec/capacity/storage").is_some());
                assert!(manifest.pointer("/spec/hostPath/path").is_some());
            }
            "StorageClass" => assert!(manifest.pointer("/provisioner").is_some()),
            "HorizontalPodAutoscaler" => {
                assert!(manifest.pointer("/spec/scaleTargetRef/name").is_some());
                assert!(manifest.pointer("/spec/maxReplicas").is_some());
            }
            "PodDisruptionBudget" => assert!(manifest.pointer("/spec/selector").is_some()),
            "PriorityClass" => assert!(manifest.pointer("/value").is_some()),
            "RuntimeClass" => assert!(manifest.pointer("/handler").is_some()),
            "Lease" => assert!(manifest.pointer("/spec/leaseDurationSeconds").is_some()),
            "ResourceQuota" => assert!(manifest.pointer("/spec/hard/pods").is_some()),
            "LimitRange" => assert!(manifest.pointer("/spec/limits/0/type").is_some()),
            "Role" | "ClusterRole" => assert!(manifest.pointer("/rules/0/verbs/0").is_some()),
            "RoleBinding" | "ClusterRoleBinding" => {
                assert!(manifest.pointer("/subjects/0/kind").is_some());
                assert!(manifest.pointer("/roleRef/name").is_some());
            }
            "CustomResourceDefinition" => {
                assert!(manifest.pointer("/spec/group").is_some());
                assert!(
                    manifest
                        .pointer("/spec/versions/0/schema/openAPIV3Schema")
                        .is_some()
                );
            }
            other => panic!("missing required field assertions for {other}"),
        }
    }
}
