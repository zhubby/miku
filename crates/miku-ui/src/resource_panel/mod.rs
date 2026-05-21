use miku_api::{
    LogLine, PodAttachInput, PodAttachOutput, PodAttachRequest as ApiPodAttachRequest,
    PodEvictRequest, PodLogQuery, ResourceApplyRequest, ResourceDeleteRequest, ResourceEvent,
    ResourceList, ResourceSummary,
};
use miku_core::{ClusterId, ResourceRef};

mod components;
mod config_map;
mod cron_job;
mod custom_resources;
mod daemon_set;
mod deployment;
mod event;
mod job;
mod limit_range;
mod namespace;
mod node;
mod pod;
mod replica_set;
mod resource_quota;
mod secret;
mod stateful_set;

pub(crate) use config_map::ConfigMapResourcePanel;
pub(crate) use cron_job::CronJobResourcePanel;
pub(crate) use custom_resources::CustomResourcesPanel;
pub(crate) use daemon_set::DaemonSetResourcePanel;
pub(crate) use deployment::DeploymentResourcePanel;
pub(crate) use event::EventResourcePanel;
pub(crate) use job::JobResourcePanel;
pub(crate) use limit_range::LimitRangeResourcePanel;
pub(crate) use namespace::NamespaceResourcePanel;
pub(crate) use node::NodeResourcePanel;
pub(crate) use pod::PodResourcePanel;
pub(crate) use replica_set::ReplicaSetResourcePanel;
pub(crate) use resource_quota::ResourceQuotaResourcePanel;
pub(crate) use secret::SecretResourcePanel;
pub(crate) use stateful_set::StatefulSetResourcePanel;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ResourceLoadRequest {
    pub(crate) request_id: u64,
    pub(crate) cluster_id: ClusterId,
    pub(crate) kind: ResourceLoadKind,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ResourceWatchKey {
    pub(crate) cluster_id: ClusterId,
    pub(crate) kind: ResourceLoadKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ResourceWatchRequest {
    pub(crate) request_id: u64,
    pub(crate) cluster_id: ClusterId,
    pub(crate) kind: ResourceLoadKind,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ResourceActionRequest {
    pub(crate) request_id: u64,
    pub(crate) cluster_id: ClusterId,
    pub(crate) kind: ResourceActionKind,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ResourceActionKind {
    ApplyPod {
        namespace: Option<String>,
        name: String,
        manifest: serde_json::Value,
    },
    DeletePod {
        namespace: Option<String>,
        name: String,
    },
    BatchDeletePods {
        targets: Vec<ResourceDeleteTarget>,
    },
    EvictPod {
        namespace: String,
        name: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ResourceDeleteTarget {
    pub(crate) namespace: Option<String>,
    pub(crate) name: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ResourcePanelRequests {
    pub(crate) loads: Vec<ResourceLoadRequest>,
    pub(crate) watches: Vec<ResourceWatchRequest>,
    pub(crate) actions: Vec<ResourceActionRequest>,
    pub(crate) logs: Vec<PodLogRequest>,
    pub(crate) attaches: Vec<PodAttachRequest>,
    pub(crate) attach_inputs: Vec<PodAttachInputRequest>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum ResourceLoadKind {
    Namespaces,
    Nodes,
    ConfigMaps { namespace: Option<String> },
    Events { namespace: Option<String> },
    CronJobs { namespace: Option<String> },
    DaemonSets { namespace: Option<String> },
    Deployments { namespace: Option<String> },
    Jobs { namespace: Option<String> },
    LimitRanges { namespace: Option<String> },
    ReplicaSets { namespace: Option<String> },
    ResourceQuotas { namespace: Option<String> },
    Secrets { namespace: Option<String> },
    StatefulSets { namespace: Option<String> },
    Pods { namespace: Option<String> },
    CustomResourceDefinitions,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PodLogRequest {
    pub(crate) request_id: u64,
    pub(crate) cluster_id: ClusterId,
    pub(crate) namespace: String,
    pub(crate) pod: String,
    pub(crate) container: Option<String>,
    pub(crate) tail_lines: Option<u32>,
}

impl PodLogRequest {
    pub(crate) fn query(&self) -> PodLogQuery {
        PodLogQuery {
            cluster_id: self.cluster_id.clone(),
            namespace: self.namespace.clone(),
            pod: self.pod.clone(),
            container: self.container.clone(),
            tail_lines: self.tail_lines,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PodAttachRequest {
    pub(crate) request_id: u64,
    pub(crate) cluster_id: ClusterId,
    pub(crate) namespace: String,
    pub(crate) pod: String,
    pub(crate) container: Option<String>,
    pub(crate) tty: bool,
}

impl PodAttachRequest {
    pub(crate) fn query(&self) -> ApiPodAttachRequest {
        ApiPodAttachRequest {
            cluster_id: self.cluster_id.clone(),
            namespace: self.namespace.clone(),
            pod: self.pod.clone(),
            container: self.container.clone(),
            tty: self.tty,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PodAttachInputRequest {
    pub(crate) request_id: u64,
    pub(crate) input: PodAttachInput,
}

impl ResourceLoadRequest {
    pub(crate) fn query(&self) -> miku_api::ResourceQuery {
        resource_query_for_kind(self.cluster_id.clone(), &self.kind)
    }
}

impl ResourceWatchRequest {
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn key(&self) -> ResourceWatchKey {
        let kind = match &self.kind {
            ResourceLoadKind::Namespaces => ResourceLoadKind::Namespaces,
            ResourceLoadKind::Nodes => ResourceLoadKind::Nodes,
            ResourceLoadKind::ConfigMaps { .. } => ResourceLoadKind::ConfigMaps { namespace: None },
            ResourceLoadKind::Events { .. } => ResourceLoadKind::Events { namespace: None },
            ResourceLoadKind::CronJobs { .. } => ResourceLoadKind::CronJobs { namespace: None },
            ResourceLoadKind::DaemonSets { .. } => ResourceLoadKind::DaemonSets { namespace: None },
            ResourceLoadKind::Deployments { .. } => {
                ResourceLoadKind::Deployments { namespace: None }
            }
            ResourceLoadKind::StatefulSets { .. } => {
                ResourceLoadKind::StatefulSets { namespace: None }
            }
            ResourceLoadKind::Jobs { .. } => ResourceLoadKind::Jobs { namespace: None },
            ResourceLoadKind::LimitRanges { .. } => {
                ResourceLoadKind::LimitRanges { namespace: None }
            }
            ResourceLoadKind::ReplicaSets { .. } => {
                ResourceLoadKind::ReplicaSets { namespace: None }
            }
            ResourceLoadKind::ResourceQuotas { .. } => {
                ResourceLoadKind::ResourceQuotas { namespace: None }
            }
            ResourceLoadKind::Secrets { .. } => ResourceLoadKind::Secrets { namespace: None },
            ResourceLoadKind::Pods { .. } => ResourceLoadKind::Pods { namespace: None },
            ResourceLoadKind::CustomResourceDefinitions => {
                ResourceLoadKind::CustomResourceDefinitions
            }
        };
        ResourceWatchKey {
            cluster_id: self.cluster_id.clone(),
            kind,
        }
    }

    pub(crate) fn query(&self) -> miku_api::ResourceQuery {
        resource_query_for_kind(self.cluster_id.clone(), &self.kind)
    }
}

impl ResourceActionRequest {
    pub(crate) fn apply_request(&self) -> Option<ResourceApplyRequest> {
        match &self.kind {
            ResourceActionKind::ApplyPod {
                namespace,
                name,
                manifest,
            } => Some(ResourceApplyRequest {
                cluster_id: self.cluster_id.clone(),
                resource: ResourceRef::core("v1", "pods"),
                namespace: namespace.clone(),
                name: name.clone(),
                manifest: manifest.clone(),
            }),
            ResourceActionKind::DeletePod { .. }
            | ResourceActionKind::BatchDeletePods { .. }
            | ResourceActionKind::EvictPod { .. } => None,
        }
    }

    pub(crate) fn delete_request(&self) -> Option<ResourceDeleteRequest> {
        match &self.kind {
            ResourceActionKind::ApplyPod { .. }
            | ResourceActionKind::BatchDeletePods { .. }
            | ResourceActionKind::EvictPod { .. } => None,
            ResourceActionKind::DeletePod { namespace, name } => Some(ResourceDeleteRequest {
                cluster_id: self.cluster_id.clone(),
                resource: ResourceRef::core("v1", "pods"),
                namespace: namespace.clone(),
                name: name.clone(),
            }),
        }
    }

    pub(crate) fn batch_delete_requests(&self) -> Option<Vec<ResourceDeleteRequest>> {
        let ResourceActionKind::BatchDeletePods { targets } = &self.kind else {
            return None;
        };

        Some(
            targets
                .iter()
                .map(|target| ResourceDeleteRequest {
                    cluster_id: self.cluster_id.clone(),
                    resource: ResourceRef::core("v1", "pods"),
                    namespace: target.namespace.clone(),
                    name: target.name.clone(),
                })
                .collect(),
        )
    }

    pub(crate) fn evict_request(&self) -> Option<PodEvictRequest> {
        match &self.kind {
            ResourceActionKind::EvictPod { namespace, name } => Some(PodEvictRequest {
                cluster_id: self.cluster_id.clone(),
                namespace: namespace.clone(),
                pod: name.clone(),
            }),
            ResourceActionKind::ApplyPod { .. }
            | ResourceActionKind::DeletePod { .. }
            | ResourceActionKind::BatchDeletePods { .. } => None,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) enum ResourceUiEvent {
    ResourcesLoaded {
        request: ResourceLoadRequest,
        result: Result<ResourceList, String>,
    },
    ResourceActionCompleted {
        request: ResourceActionRequest,
        result: Result<ResourceActionOutcome, String>,
    },
    PodLogsLoaded {
        request: PodLogRequest,
        result: Result<Vec<LogLine>, String>,
    },
    PodAttachConnected {
        request: PodAttachRequest,
        result: Result<futures::channel::mpsc::UnboundedSender<PodAttachInput>, String>,
    },
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    PodAttachOutput {
        request: PodAttachRequest,
        result: Result<PodAttachOutput, String>,
    },
    ResourceWatchUpdated {
        request: ResourceWatchRequest,
        result: Result<ResourceEvent, String>,
    },
}

impl ResourceUiEvent {
    pub(crate) fn cluster_id(&self) -> &ClusterId {
        match self {
            Self::ResourcesLoaded { request, .. } => &request.cluster_id,
            Self::ResourceActionCompleted { request, .. } => &request.cluster_id,
            Self::PodLogsLoaded { request, .. } => &request.cluster_id,
            Self::PodAttachConnected { request, .. } => &request.cluster_id,
            Self::PodAttachOutput { request, .. } => &request.cluster_id,
            Self::ResourceWatchUpdated { request, .. } => &request.cluster_id,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ResourceActionOutcome {
    Applied(ResourceSummary),
    Deleted,
    BatchDeleted(Vec<ResourceDeleteTarget>),
    Evicted,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
enum LoadStatus {
    #[default]
    Idle,
    Loading,
    Loaded,
    Error(String),
}

fn namespaces_from_list(list: &ResourceList) -> Vec<String> {
    let mut namespaces = list
        .items
        .iter()
        .map(|item| item.name.clone())
        .collect::<Vec<_>>();
    namespaces.sort();
    namespaces.dedup();
    namespaces
}

fn resource_query_for_kind(
    cluster_id: ClusterId,
    kind: &ResourceLoadKind,
) -> miku_api::ResourceQuery {
    match kind {
        ResourceLoadKind::Namespaces => miku_api::ResourceQuery::new(
            cluster_id,
            ResourceRef::core("v1", "namespaces").cluster_scoped(),
        ),
        ResourceLoadKind::Nodes => miku_api::ResourceQuery::new(
            cluster_id,
            ResourceRef::core("v1", "nodes").cluster_scoped(),
        ),
        ResourceLoadKind::ConfigMaps { namespace } => {
            let mut query =
                miku_api::ResourceQuery::new(cluster_id, ResourceRef::core("v1", "configmaps"));
            if let Some(namespace) = namespace {
                query = query.namespace(namespace.clone());
            }
            query
        }
        ResourceLoadKind::Events { namespace } => {
            let mut query =
                miku_api::ResourceQuery::new(cluster_id, ResourceRef::core("v1", "events"));
            if let Some(namespace) = namespace {
                query = query.namespace(namespace.clone());
            }
            query
        }
        ResourceLoadKind::CronJobs { namespace } => {
            let mut query = miku_api::ResourceQuery::new(
                cluster_id,
                ResourceRef::grouped("batch", "v1", "cronjobs"),
            );
            if let Some(namespace) = namespace {
                query = query.namespace(namespace.clone());
            }
            query
        }
        ResourceLoadKind::DaemonSets { namespace } => {
            let mut query = miku_api::ResourceQuery::new(
                cluster_id,
                ResourceRef::grouped("apps", "v1", "daemonsets"),
            );
            if let Some(namespace) = namespace {
                query = query.namespace(namespace.clone());
            }
            query
        }
        ResourceLoadKind::Deployments { namespace } => {
            let mut query = miku_api::ResourceQuery::new(
                cluster_id,
                ResourceRef::grouped("apps", "v1", "deployments"),
            );
            if let Some(namespace) = namespace {
                query = query.namespace(namespace.clone());
            }
            query
        }
        ResourceLoadKind::StatefulSets { namespace } => {
            let mut query = miku_api::ResourceQuery::new(
                cluster_id,
                ResourceRef::grouped("apps", "v1", "statefulsets"),
            );
            if let Some(namespace) = namespace {
                query = query.namespace(namespace.clone());
            }
            query
        }
        ResourceLoadKind::Jobs { namespace } => {
            let mut query = miku_api::ResourceQuery::new(
                cluster_id,
                ResourceRef::grouped("batch", "v1", "jobs"),
            );
            if let Some(namespace) = namespace {
                query = query.namespace(namespace.clone());
            }
            query
        }
        ResourceLoadKind::LimitRanges { namespace } => {
            let mut query =
                miku_api::ResourceQuery::new(cluster_id, ResourceRef::core("v1", "limitranges"));
            if let Some(namespace) = namespace {
                query = query.namespace(namespace.clone());
            }
            query
        }
        ResourceLoadKind::ReplicaSets { namespace } => {
            let mut query = miku_api::ResourceQuery::new(
                cluster_id,
                ResourceRef::grouped("apps", "v1", "replicasets"),
            );
            if let Some(namespace) = namespace {
                query = query.namespace(namespace.clone());
            }
            query
        }
        ResourceLoadKind::ResourceQuotas { namespace } => {
            let mut query =
                miku_api::ResourceQuery::new(cluster_id, ResourceRef::core("v1", "resourcequotas"));
            if let Some(namespace) = namespace {
                query = query.namespace(namespace.clone());
            }
            query
        }
        ResourceLoadKind::Secrets { namespace } => {
            let mut query =
                miku_api::ResourceQuery::new(cluster_id, ResourceRef::core("v1", "secrets"));
            if let Some(namespace) = namespace {
                query = query.namespace(namespace.clone());
            }
            query
        }
        ResourceLoadKind::Pods { namespace } => {
            let mut query =
                miku_api::ResourceQuery::new(cluster_id, ResourceRef::core("v1", "pods"));
            if let Some(namespace) = namespace {
                query = query.namespace(namespace.clone());
            }
            query
        }
        ResourceLoadKind::CustomResourceDefinitions => miku_api::ResourceQuery::new(
            cluster_id,
            ResourceRef::grouped("apiextensions.k8s.io", "v1", "customresourcedefinitions")
                .cluster_scoped(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_resource_definitions_query_uses_cluster_scoped_crd_api() {
        let query = resource_query_for_kind(
            ClusterId::new("local"),
            &ResourceLoadKind::CustomResourceDefinitions,
        );

        assert_eq!(
            query.resource,
            ResourceRef::grouped("apiextensions.k8s.io", "v1", "customresourcedefinitions")
                .cluster_scoped()
        );
        assert_eq!(query.namespace, None);
    }

    #[test]
    fn nodes_query_uses_cluster_scoped_core_api() {
        let query = resource_query_for_kind(ClusterId::new("local"), &ResourceLoadKind::Nodes);

        assert_eq!(
            query.resource,
            ResourceRef::core("v1", "nodes").cluster_scoped()
        );
        assert_eq!(query.namespace, None);
    }

    #[test]
    fn events_query_uses_core_api_without_namespace_by_default() {
        let query = resource_query_for_kind(
            ClusterId::new("local"),
            &ResourceLoadKind::Events { namespace: None },
        );

        assert_eq!(query.resource, ResourceRef::core("v1", "events"));
        assert_eq!(query.namespace, None);
    }

    #[test]
    fn events_query_uses_selected_namespace() {
        let query = resource_query_for_kind(
            ClusterId::new("local"),
            &ResourceLoadKind::Events {
                namespace: Some("production".to_owned()),
            },
        );

        assert_eq!(query.resource, ResourceRef::core("v1", "events"));
        assert_eq!(query.namespace.as_deref(), Some("production"));
    }

    #[test]
    fn config_maps_query_uses_core_api_without_namespace_by_default() {
        let query = resource_query_for_kind(
            ClusterId::new("local"),
            &ResourceLoadKind::ConfigMaps { namespace: None },
        );

        assert_eq!(query.resource, ResourceRef::core("v1", "configmaps"));
        assert_eq!(query.namespace, None);
    }

    #[test]
    fn config_maps_query_uses_selected_namespace() {
        let query = resource_query_for_kind(
            ClusterId::new("local"),
            &ResourceLoadKind::ConfigMaps {
                namespace: Some("production".to_owned()),
            },
        );

        assert_eq!(query.resource, ResourceRef::core("v1", "configmaps"));
        assert_eq!(query.namespace.as_deref(), Some("production"));
    }

    #[test]
    fn resource_quotas_query_uses_core_api_without_namespace_by_default() {
        let query = resource_query_for_kind(
            ClusterId::new("local"),
            &ResourceLoadKind::ResourceQuotas { namespace: None },
        );

        assert_eq!(query.resource, ResourceRef::core("v1", "resourcequotas"));
        assert_eq!(query.namespace, None);
    }

    #[test]
    fn resource_quotas_query_uses_selected_namespace() {
        let query = resource_query_for_kind(
            ClusterId::new("local"),
            &ResourceLoadKind::ResourceQuotas {
                namespace: Some("production".to_owned()),
            },
        );

        assert_eq!(query.resource, ResourceRef::core("v1", "resourcequotas"));
        assert_eq!(query.namespace.as_deref(), Some("production"));
    }

    #[test]
    fn limit_ranges_query_uses_core_api_without_namespace_by_default() {
        let query = resource_query_for_kind(
            ClusterId::new("local"),
            &ResourceLoadKind::LimitRanges { namespace: None },
        );

        assert_eq!(query.resource, ResourceRef::core("v1", "limitranges"));
        assert_eq!(query.namespace, None);
    }

    #[test]
    fn limit_ranges_query_uses_selected_namespace() {
        let query = resource_query_for_kind(
            ClusterId::new("local"),
            &ResourceLoadKind::LimitRanges {
                namespace: Some("production".to_owned()),
            },
        );

        assert_eq!(query.resource, ResourceRef::core("v1", "limitranges"));
        assert_eq!(query.namespace.as_deref(), Some("production"));
    }

    #[test]
    fn secrets_query_uses_core_api_without_namespace_by_default() {
        let query = resource_query_for_kind(
            ClusterId::new("local"),
            &ResourceLoadKind::Secrets { namespace: None },
        );

        assert_eq!(query.resource, ResourceRef::core("v1", "secrets"));
        assert_eq!(query.namespace, None);
    }

    #[test]
    fn secrets_query_uses_selected_namespace() {
        let query = resource_query_for_kind(
            ClusterId::new("local"),
            &ResourceLoadKind::Secrets {
                namespace: Some("production".to_owned()),
            },
        );

        assert_eq!(query.resource, ResourceRef::core("v1", "secrets"));
        assert_eq!(query.namespace.as_deref(), Some("production"));
    }

    #[test]
    fn deployments_query_uses_apps_api_without_namespace_by_default() {
        let query = resource_query_for_kind(
            ClusterId::new("local"),
            &ResourceLoadKind::Deployments { namespace: None },
        );

        assert_eq!(
            query.resource,
            ResourceRef::grouped("apps", "v1", "deployments")
        );
        assert_eq!(query.namespace, None);
    }

    #[test]
    fn deployments_query_uses_selected_namespace() {
        let query = resource_query_for_kind(
            ClusterId::new("local"),
            &ResourceLoadKind::Deployments {
                namespace: Some("production".to_owned()),
            },
        );

        assert_eq!(
            query.resource,
            ResourceRef::grouped("apps", "v1", "deployments")
        );
        assert_eq!(query.namespace.as_deref(), Some("production"));
    }

    #[test]
    fn cron_jobs_query_uses_batch_api_without_namespace_by_default() {
        let query = resource_query_for_kind(
            ClusterId::new("local"),
            &ResourceLoadKind::CronJobs { namespace: None },
        );

        assert_eq!(
            query.resource,
            ResourceRef::grouped("batch", "v1", "cronjobs")
        );
        assert_eq!(query.namespace, None);
    }

    #[test]
    fn cron_jobs_query_uses_selected_namespace() {
        let query = resource_query_for_kind(
            ClusterId::new("local"),
            &ResourceLoadKind::CronJobs {
                namespace: Some("production".to_owned()),
            },
        );

        assert_eq!(
            query.resource,
            ResourceRef::grouped("batch", "v1", "cronjobs")
        );
        assert_eq!(query.namespace.as_deref(), Some("production"));
    }

    #[test]
    fn jobs_query_uses_batch_api_without_namespace_by_default() {
        let query = resource_query_for_kind(
            ClusterId::new("local"),
            &ResourceLoadKind::Jobs { namespace: None },
        );

        assert_eq!(query.resource, ResourceRef::grouped("batch", "v1", "jobs"));
        assert_eq!(query.namespace, None);
    }

    #[test]
    fn jobs_query_uses_selected_namespace() {
        let query = resource_query_for_kind(
            ClusterId::new("local"),
            &ResourceLoadKind::Jobs {
                namespace: Some("production".to_owned()),
            },
        );

        assert_eq!(query.resource, ResourceRef::grouped("batch", "v1", "jobs"));
        assert_eq!(query.namespace.as_deref(), Some("production"));
    }

    #[test]
    fn replica_sets_query_uses_apps_api_without_namespace_by_default() {
        let query = resource_query_for_kind(
            ClusterId::new("local"),
            &ResourceLoadKind::ReplicaSets { namespace: None },
        );

        assert_eq!(
            query.resource,
            ResourceRef::grouped("apps", "v1", "replicasets")
        );
        assert_eq!(query.namespace, None);
    }

    #[test]
    fn replica_sets_query_uses_selected_namespace() {
        let query = resource_query_for_kind(
            ClusterId::new("local"),
            &ResourceLoadKind::ReplicaSets {
                namespace: Some("production".to_owned()),
            },
        );

        assert_eq!(
            query.resource,
            ResourceRef::grouped("apps", "v1", "replicasets")
        );
        assert_eq!(query.namespace.as_deref(), Some("production"));
    }

    #[test]
    fn daemon_sets_query_uses_apps_api_without_namespace_by_default() {
        let query = resource_query_for_kind(
            ClusterId::new("local"),
            &ResourceLoadKind::DaemonSets { namespace: None },
        );

        assert_eq!(
            query.resource,
            ResourceRef::grouped("apps", "v1", "daemonsets")
        );
        assert_eq!(query.namespace, None);
    }

    #[test]
    fn daemon_sets_query_uses_selected_namespace() {
        let query = resource_query_for_kind(
            ClusterId::new("local"),
            &ResourceLoadKind::DaemonSets {
                namespace: Some("production".to_owned()),
            },
        );

        assert_eq!(
            query.resource,
            ResourceRef::grouped("apps", "v1", "daemonsets")
        );
        assert_eq!(query.namespace.as_deref(), Some("production"));
    }

    #[test]
    fn stateful_sets_query_uses_apps_api_without_namespace_by_default() {
        let query = resource_query_for_kind(
            ClusterId::new("local"),
            &ResourceLoadKind::StatefulSets { namespace: None },
        );

        assert_eq!(
            query.resource,
            ResourceRef::grouped("apps", "v1", "statefulsets")
        );
        assert_eq!(query.namespace, None);
    }

    #[test]
    fn stateful_sets_query_uses_selected_namespace() {
        let query = resource_query_for_kind(
            ClusterId::new("local"),
            &ResourceLoadKind::StatefulSets {
                namespace: Some("production".to_owned()),
            },
        );

        assert_eq!(
            query.resource,
            ResourceRef::grouped("apps", "v1", "statefulsets")
        );
        assert_eq!(query.namespace.as_deref(), Some("production"));
    }
}
