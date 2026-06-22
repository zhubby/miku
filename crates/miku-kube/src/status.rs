use async_trait::async_trait;
use kube::ResourceExt;
use kube::core::DynamicObject;
use miku_api::{
    ClusterConfigStore, ClusterRegistry, ClusterStatusCondition, ClusterStatusEventSummary,
    ClusterStatusOverview, ClusterStatusReader, ClusterStatusReport, ClusterStatusRequest,
    ClusterStatusSeverity, ClusterStatusWorkloadSummary, LocalPreferenceStore, ResourceQuery,
};
use miku_core::ResourceRef;

use crate::client::KubeServices;

#[async_trait]
impl<S> ClusterStatusReader for KubeServices<S>
where
    S: ClusterConfigStore + ClusterRegistry + LocalPreferenceStore + Clone + Send + Sync,
{
    #[tracing::instrument(name = "kube.get_cluster_status", skip(self), fields(cluster_id = %request.cluster_id))]
    async fn get_cluster_status(
        &self,
        request: ClusterStatusRequest,
    ) -> miku_core::Result<ClusterStatusReport> {
        let cluster_id = request.cluster_id;
        let client = self.client_for_cluster(&cluster_id).await?;

        let version_client = client.clone();
        let version = async move {
            version_client
                .apiserver_version()
                .await
                .map_err(|error| miku_core::MikuError::Kubernetes(error.to_string()))
        };
        let namespaces = self.cached_snapshot(
            client.clone(),
            ResourceQuery::new(cluster_id.clone(), ResourceRef::core("v1", "namespaces")),
        );
        let nodes = self.cached_snapshot(
            client.clone(),
            ResourceQuery::new(cluster_id.clone(), ResourceRef::core("v1", "nodes")),
        );
        let pods = self.cached_snapshot(
            client.clone(),
            ResourceQuery::new(cluster_id.clone(), ResourceRef::core("v1", "pods")),
        );
        let deployments = self.cached_snapshot(
            client.clone(),
            ResourceQuery::new(
                cluster_id.clone(),
                ResourceRef::grouped("apps", "v1", "deployments"),
            ),
        );
        let services = self.cached_snapshot(
            client.clone(),
            ResourceQuery::new(cluster_id.clone(), ResourceRef::core("v1", "services")),
        );
        let config_maps = self.cached_snapshot(
            client.clone(),
            ResourceQuery::new(cluster_id.clone(), ResourceRef::core("v1", "configmaps")),
        );
        let secrets = self.cached_snapshot(
            client.clone(),
            ResourceQuery::new(cluster_id.clone(), ResourceRef::core("v1", "secrets")),
        );
        let events = self.cached_snapshot(
            client,
            ResourceQuery::new(cluster_id, ResourceRef::core("v1", "events")),
        );

        let (version, namespaces, nodes, pods, deployments, services, config_maps, secrets, events) = tokio::join!(
            version,
            namespaces,
            nodes,
            pods,
            deployments,
            services,
            config_maps,
            secrets,
            events
        );

        let version = version?;
        let mut collection_errors = Vec::new();
        let namespaces = snapshot_or_empty("namespaces", namespaces, &mut collection_errors);
        let nodes = snapshot_or_empty("nodes", nodes, &mut collection_errors);
        let pods = snapshot_or_empty("pods", pods, &mut collection_errors);
        let deployments = snapshot_or_empty("deployments", deployments, &mut collection_errors);
        let services = snapshot_or_empty("services", services, &mut collection_errors);
        let config_maps = snapshot_or_empty("configmaps", config_maps, &mut collection_errors);
        let secrets = snapshot_or_empty("secrets", secrets, &mut collection_errors);
        let events = snapshot_or_empty("events", events, &mut collection_errors);

        Ok(build_cluster_status_report(
            version.git_version,
            (!version.platform.is_empty()).then_some(version.platform),
            ClusterStatusSnapshots {
                namespaces: &namespaces,
                nodes: &nodes,
                pods: &pods,
                deployments: &deployments,
                services: &services,
                config_maps: &config_maps,
                secrets: &secrets,
                events: &events,
                collection_errors: &collection_errors,
            },
        ))
    }
}

#[derive(Debug, Eq, PartialEq)]
struct ClusterStatusCollectionError {
    resource: &'static str,
    message: String,
}

struct ClusterStatusSnapshots<'a> {
    namespaces: &'a [DynamicObject],
    nodes: &'a [DynamicObject],
    pods: &'a [DynamicObject],
    deployments: &'a [DynamicObject],
    services: &'a [DynamicObject],
    config_maps: &'a [DynamicObject],
    secrets: &'a [DynamicObject],
    events: &'a [DynamicObject],
    collection_errors: &'a [ClusterStatusCollectionError],
}

fn snapshot_or_empty(
    resource: &'static str,
    result: miku_core::Result<Vec<DynamicObject>>,
    errors: &mut Vec<ClusterStatusCollectionError>,
) -> Vec<DynamicObject> {
    match result {
        Ok(items) => items,
        Err(error) => {
            tracing::warn!(resource, %error, "cluster status resource snapshot failed");
            errors.push(ClusterStatusCollectionError {
                resource,
                message: error.to_string(),
            });
            Vec::new()
        }
    }
}

fn build_cluster_status_report(
    version: String,
    platform: Option<String>,
    snapshots: ClusterStatusSnapshots<'_>,
) -> ClusterStatusReport {
    let ready_nodes = count_ready_nodes(snapshots.nodes);
    let unhealthy_pods = count_unhealthy_pods(snapshots.pods);
    let warning_events = snapshots
        .events
        .iter()
        .filter(|event| json_str(&event.data, "/type") == Some("Warning"))
        .count();

    ClusterStatusReport {
        overview: ClusterStatusOverview {
            version,
            platform,
            namespaces: snapshots.namespaces.len(),
            nodes: snapshots.nodes.len(),
            pods: snapshots.pods.len(),
            ready_nodes,
            unhealthy_pods,
        },
        conditions: cluster_status_conditions(
            snapshots.nodes.len(),
            ready_nodes,
            snapshots.pods.len(),
            unhealthy_pods,
            warning_events,
            snapshots.collection_errors,
        ),
        workloads: ClusterStatusWorkloadSummary {
            pods: snapshots.pods.len(),
            deployments: snapshots.deployments.len(),
            services: snapshots.services.len(),
            config_maps: snapshots.config_maps.len(),
            secrets: snapshots.secrets.len(),
        },
        recent_events: recent_event_summaries(snapshots.events),
    }
}

fn count_ready_nodes(nodes: &[DynamicObject]) -> usize {
    nodes.iter().filter(|node| node_is_ready(node)).count()
}

fn node_is_ready(node: &DynamicObject) -> bool {
    node.data
        .pointer("/status/conditions")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|conditions| {
            conditions.iter().any(|condition| {
                json_str(condition, "/type") == Some("Ready")
                    && json_str(condition, "/status") == Some("True")
            })
        })
}

fn count_unhealthy_pods(pods: &[DynamicObject]) -> usize {
    pods.iter().filter(|pod| pod_is_unhealthy(pod)).count()
}

fn pod_is_unhealthy(pod: &DynamicObject) -> bool {
    let phase = json_str(&pod.data, "/status/phase").unwrap_or_default();
    if !matches!(phase, "Running" | "Succeeded") {
        return true;
    }

    pod.data
        .pointer("/status/containerStatuses")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|statuses| {
            statuses.iter().any(|status| {
                status
                    .pointer("/ready")
                    .and_then(serde_json::Value::as_bool)
                    == Some(false)
            })
        })
}

fn cluster_status_conditions(
    nodes: usize,
    ready_nodes: usize,
    pods: usize,
    unhealthy_pods: usize,
    warning_events: usize,
    collection_errors: &[ClusterStatusCollectionError],
) -> Vec<ClusterStatusCondition> {
    let mut conditions = vec![
        node_condition(nodes, ready_nodes, collection_errors),
        pod_condition(pods, unhealthy_pods, collection_errors),
        event_condition(warning_events, collection_errors),
    ];

    if !collection_errors.is_empty() {
        conditions.push(collection_status_condition(collection_errors));
    }

    conditions
}

fn node_condition(
    nodes: usize,
    ready_nodes: usize,
    collection_errors: &[ClusterStatusCollectionError],
) -> ClusterStatusCondition {
    if let Some(error) = collection_error(collection_errors, "nodes") {
        return ClusterStatusCondition {
            name: "Nodes".to_owned(),
            status: "unavailable".to_owned(),
            severity: ClusterStatusSeverity::Critical,
            message: format!("Could not read node status: {}", error.message),
        };
    }

    ClusterStatusCondition {
        name: "Nodes".to_owned(),
        status: format!("{ready_nodes}/{nodes} ready"),
        severity: if nodes == ready_nodes {
            ClusterStatusSeverity::Ok
        } else {
            ClusterStatusSeverity::Critical
        },
        message: if nodes == ready_nodes {
            "All nodes are ready".to_owned()
        } else {
            format!(
                "{} node(s) are not ready",
                nodes.saturating_sub(ready_nodes)
            )
        },
    }
}

fn pod_condition(
    pods: usize,
    unhealthy_pods: usize,
    collection_errors: &[ClusterStatusCollectionError],
) -> ClusterStatusCondition {
    if let Some(error) = collection_error(collection_errors, "pods") {
        return ClusterStatusCondition {
            name: "Pods".to_owned(),
            status: "unavailable".to_owned(),
            severity: ClusterStatusSeverity::Warning,
            message: format!("Could not read pod status: {}", error.message),
        };
    }

    ClusterStatusCondition {
        name: "Pods".to_owned(),
        status: format!("{} unhealthy / {pods} total", unhealthy_pods),
        severity: if unhealthy_pods == 0 {
            ClusterStatusSeverity::Ok
        } else {
            ClusterStatusSeverity::Warning
        },
        message: if unhealthy_pods == 0 {
            "No unhealthy pods detected".to_owned()
        } else {
            format!("{unhealthy_pods} pod(s) need attention")
        },
    }
}

fn event_condition(
    warning_events: usize,
    collection_errors: &[ClusterStatusCollectionError],
) -> ClusterStatusCondition {
    if let Some(error) = collection_error(collection_errors, "events") {
        return ClusterStatusCondition {
            name: "Events".to_owned(),
            status: "unavailable".to_owned(),
            severity: ClusterStatusSeverity::Warning,
            message: format!("Could not read recent events: {}", error.message),
        };
    }

    ClusterStatusCondition {
        name: "Events".to_owned(),
        status: format!("{warning_events} warning"),
        severity: if warning_events == 0 {
            ClusterStatusSeverity::Ok
        } else {
            ClusterStatusSeverity::Warning
        },
        message: if warning_events == 0 {
            "No warning events in the recent event cache".to_owned()
        } else {
            format!("{warning_events} warning event(s) were found")
        },
    }
}

fn collection_status_condition(
    collection_errors: &[ClusterStatusCollectionError],
) -> ClusterStatusCondition {
    let resources = collection_errors
        .iter()
        .map(|error| error.resource)
        .collect::<Vec<_>>()
        .join(", ");

    ClusterStatusCondition {
        name: "Data collection".to_owned(),
        status: format!("{} unavailable", collection_errors.len()),
        severity: ClusterStatusSeverity::Warning,
        message: format!("Could not read: {resources}"),
    }
}

fn collection_error<'a>(
    collection_errors: &'a [ClusterStatusCollectionError],
    resource: &str,
) -> Option<&'a ClusterStatusCollectionError> {
    collection_errors
        .iter()
        .find(|error| error.resource == resource)
}

fn recent_event_summaries(events: &[DynamicObject]) -> Vec<ClusterStatusEventSummary> {
    let mut events = events.iter().collect::<Vec<_>>();
    events.sort_by_key(|event| std::cmp::Reverse(event_timestamp(event)));
    events.into_iter().take(10).map(event_summary).collect()
}

fn event_summary(event: &DynamicObject) -> ClusterStatusEventSummary {
    ClusterStatusEventSummary {
        namespace: event.namespace(),
        involved_object: involved_object_name(event),
        reason: json_str(&event.data, "/reason")
            .unwrap_or("Unknown")
            .to_owned(),
        message: json_str(&event.data, "/message").unwrap_or("").to_owned(),
        event_type: json_str(&event.data, "/type")
            .unwrap_or("Unknown")
            .to_owned(),
    }
}

fn involved_object_name(event: &DynamicObject) -> String {
    let kind = json_str(&event.data, "/involvedObject/kind")
        .or_else(|| json_str(&event.data, "/regarding/kind"))
        .unwrap_or("Object");
    let name = json_str(&event.data, "/involvedObject/name")
        .or_else(|| json_str(&event.data, "/regarding/name"))
        .or(event.metadata.name.as_deref())
        .unwrap_or("unknown");
    format!("{kind}/{name}")
}

fn event_timestamp(event: &DynamicObject) -> String {
    json_str(&event.data, "/lastTimestamp")
        .or_else(|| json_str(&event.data, "/eventTime"))
        .or_else(|| json_str(&event.data, "/firstTimestamp"))
        .map(ToOwned::to_owned)
        .or_else(|| {
            event
                .metadata
                .creation_timestamp
                .as_ref()
                .map(|timestamp| format!("{timestamp:?}"))
        })
        .unwrap_or_default()
}

fn json_str<'a>(value: &'a serde_json::Value, pointer: &str) -> Option<&'a str> {
    value.pointer(pointer).and_then(serde_json::Value::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_resource;

    #[test]
    fn ready_nodes_are_counted_from_ready_condition() {
        let ready = dynamic_object(
            "ready",
            "nodes",
            serde_json::json!({
                "status": {
                    "conditions": [{"type": "Ready", "status": "True"}]
                }
            }),
        );
        let not_ready = dynamic_object(
            "not-ready",
            "nodes",
            serde_json::json!({
                "status": {
                    "conditions": [{"type": "Ready", "status": "False"}]
                }
            }),
        );

        assert_eq!(count_ready_nodes(&[ready, not_ready]), 1);
    }

    #[test]
    fn unhealthy_pods_include_non_running_and_unready_containers() {
        let running = dynamic_object(
            "running",
            "pods",
            serde_json::json!({
                "status": {
                    "phase": "Running",
                    "containerStatuses": [{"ready": true}]
                }
            }),
        );
        let pending = dynamic_object(
            "pending",
            "pods",
            serde_json::json!({
                "status": {"phase": "Pending"}
            }),
        );
        let unready = dynamic_object(
            "unready",
            "pods",
            serde_json::json!({
                "status": {
                    "phase": "Running",
                    "containerStatuses": [{"ready": false}]
                }
            }),
        );

        assert_eq!(count_unhealthy_pods(&[running, pending, unready]), 2);
    }

    #[test]
    fn status_report_summarizes_workloads_and_conditions() {
        let namespace = dynamic_object("default", "namespaces", serde_json::json!({}));
        let node = dynamic_object(
            "node",
            "nodes",
            serde_json::json!({
                "status": {"conditions": [{"type": "Ready", "status": "True"}]}
            }),
        );
        let pod = dynamic_object(
            "api",
            "pods",
            serde_json::json!({
                "status": {"phase": "Running", "containerStatuses": [{"ready": true}]}
            }),
        );
        let namespaces = vec![namespace];
        let nodes = vec![node];
        let pods = vec![pod];
        let deployments = vec![dynamic_object("api", "deployments", serde_json::json!({}))];
        let services = vec![dynamic_object("api", "services", serde_json::json!({}))];
        let config_maps = vec![dynamic_object("api", "configmaps", serde_json::json!({}))];
        let secrets = vec![dynamic_object("api", "secrets", serde_json::json!({}))];
        let events = Vec::<DynamicObject>::new();

        let report = build_cluster_status_report(
            "v1.35.0".to_owned(),
            Some("darwin/arm64".to_owned()),
            ClusterStatusSnapshots {
                namespaces: &namespaces,
                nodes: &nodes,
                pods: &pods,
                deployments: &deployments,
                services: &services,
                config_maps: &config_maps,
                secrets: &secrets,
                events: &events,
                collection_errors: &[],
            },
        );

        assert_eq!(report.overview.version, "v1.35.0");
        assert_eq!(report.overview.ready_nodes, 1);
        assert_eq!(report.overview.unhealthy_pods, 0);
        assert_eq!(report.workloads.deployments, 1);
        assert_eq!(report.conditions[0].severity, ClusterStatusSeverity::Ok);
    }

    #[test]
    fn status_report_marks_unavailable_snapshots_without_failing_report() {
        let pod = dynamic_object(
            "api",
            "pods",
            serde_json::json!({
                "status": {"phase": "Running", "containerStatuses": [{"ready": true}]}
            }),
        );
        let pods = vec![pod];
        let empty = Vec::<DynamicObject>::new();
        let collection_errors = vec![
            ClusterStatusCollectionError {
                resource: "nodes",
                message: "forbidden".to_owned(),
            },
            ClusterStatusCollectionError {
                resource: "events",
                message: "timeout".to_owned(),
            },
            ClusterStatusCollectionError {
                resource: "secrets",
                message: "forbidden".to_owned(),
            },
        ];

        let report = build_cluster_status_report(
            "v1.35.0".to_owned(),
            Some("darwin/arm64".to_owned()),
            ClusterStatusSnapshots {
                namespaces: &empty,
                nodes: &empty,
                pods: &pods,
                deployments: &empty,
                services: &empty,
                config_maps: &empty,
                secrets: &empty,
                events: &empty,
                collection_errors: &collection_errors,
            },
        );

        assert_eq!(report.overview.nodes, 0);
        assert_eq!(report.workloads.pods, 1);
        assert_eq!(report.conditions[0].name, "Nodes");
        assert_eq!(report.conditions[0].status, "unavailable");
        assert_eq!(
            report.conditions[0].severity,
            ClusterStatusSeverity::Critical
        );
        assert_eq!(report.conditions[2].name, "Events");
        assert_eq!(report.conditions[2].status, "unavailable");
        assert!(
            report
                .conditions
                .iter()
                .any(|condition| condition.name == "Data collection"
                    && condition.status == "3 unavailable"
                    && condition.message.contains("secrets"))
        );
    }

    #[test]
    fn recent_events_are_sorted_and_summarized() {
        let older = event_object(
            "older",
            "default",
            "2026-01-01T00:00:00Z",
            "Normal",
            "Scheduled",
        );
        let newer = event_object(
            "newer",
            "default",
            "2026-01-02T00:00:00Z",
            "Warning",
            "BackOff",
        );

        let events = recent_event_summaries(&[older, newer]);

        assert_eq!(events[0].reason, "BackOff");
        assert_eq!(events[0].namespace.as_deref(), Some("default"));
        assert_eq!(events[0].involved_object, "Pod/api");
        assert_eq!(events[0].event_type, "Warning");
    }

    fn dynamic_object(name: &str, plural: &str, data: serde_json::Value) -> DynamicObject {
        let api_resource = api_resource(&miku_core::ResourceRef::core("v1", plural));
        let mut object = DynamicObject::new(name, &api_resource);
        object.data = data;
        object
    }

    fn event_object(
        name: &str,
        namespace: &str,
        last_timestamp: &str,
        event_type: &str,
        reason: &str,
    ) -> DynamicObject {
        let mut event = dynamic_object(
            name,
            "events",
            serde_json::json!({
                "lastTimestamp": last_timestamp,
                "type": event_type,
                "reason": reason,
                "message": "event message",
                "involvedObject": {
                    "kind": "Pod",
                    "name": "api"
                }
            }),
        );
        event.metadata.namespace = Some(namespace.to_owned());
        event
    }
}
