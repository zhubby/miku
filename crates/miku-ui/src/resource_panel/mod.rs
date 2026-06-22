use miku_api::{
    LogLine, NodeCordonRequest, NodeDrainRequest, PodAttachInput, PodAttachOutput,
    PodAttachRequest as ApiPodAttachRequest, PodEvictRequest, PodExecRequest as ApiPodExecRequest,
    PodLogQuery, ResourceApplyRequest, ResourceDeleteRequest, ResourceEvent, ResourceList,
    ResourcePatchRequest, ResourceSummary,
};
use miku_core::{ClusterId, ResourceRef};

mod access_control_shared;
mod cluster_role;
mod cluster_role_binding;
mod components;
mod config_map;
mod config_shared;
mod cron_job;
mod custom_resources;
mod daemon_set;
mod deployment;
mod endpoint_slice;
mod endpoints;
mod event;
mod horizontal_pod_autoscaler;
mod ingress;
mod ingress_class;
mod job;
mod lease;
mod limit_range;
mod mutating_webhook_configuration;
mod namespace;
mod network_policy;
mod network_shared;
mod node;
mod persistent_volume;
mod persistent_volume_claim;
mod pod;
mod pod_disruption_budget;
mod priority_class;
mod replica_set;
mod resource_quota;
mod role;
mod role_binding;
mod runtime_class;
mod secret;
mod service;
mod service_account;
mod stateful_set;
mod storage_class;
mod storage_shared;
mod validating_webhook_configuration;

pub(crate) use cluster_role::ClusterRoleResourcePanel;
pub(crate) use cluster_role_binding::ClusterRoleBindingResourcePanel;
pub(crate) use config_map::ConfigMapResourcePanel;
pub(crate) use cron_job::CronJobResourcePanel;
pub(crate) use custom_resources::CustomResourcesPanel;
pub(crate) use daemon_set::DaemonSetResourcePanel;
pub(crate) use deployment::DeploymentResourcePanel;
pub(crate) use endpoint_slice::EndpointSliceResourcePanel;
pub(crate) use endpoints::EndpointsResourcePanel;
pub(crate) use event::EventResourcePanel;
pub(crate) use horizontal_pod_autoscaler::HorizontalPodAutoscalerResourcePanel;
pub(crate) use ingress::IngressResourcePanel;
pub(crate) use ingress_class::IngressClassResourcePanel;
pub(crate) use job::JobResourcePanel;
pub(crate) use lease::LeaseResourcePanel;
pub(crate) use limit_range::LimitRangeResourcePanel;
pub(crate) use mutating_webhook_configuration::MutatingWebhookConfigurationResourcePanel;
pub(crate) use namespace::NamespaceResourcePanel;
pub(crate) use network_policy::NetworkPolicyResourcePanel;
pub(crate) use node::NodeResourcePanel;
pub(crate) use persistent_volume::PersistentVolumeResourcePanel;
pub(crate) use persistent_volume_claim::PersistentVolumeClaimResourcePanel;
pub(crate) use pod::PodResourcePanel;
pub(crate) use pod_disruption_budget::PodDisruptionBudgetResourcePanel;
pub(crate) use priority_class::PriorityClassResourcePanel;
pub(crate) use replica_set::ReplicaSetResourcePanel;
pub(crate) use resource_quota::ResourceQuotaResourcePanel;
pub(crate) use role::RoleResourcePanel;
pub(crate) use role_binding::RoleBindingResourcePanel;
pub(crate) use runtime_class::RuntimeClassResourcePanel;
pub(crate) use secret::SecretResourcePanel;
pub(crate) use service::ServiceResourcePanel;
pub(crate) use service_account::ServiceAccountResourcePanel;
pub(crate) use stateful_set::StatefulSetResourcePanel;
pub(crate) use storage_class::StorageClassResourcePanel;
pub(crate) use validating_webhook_configuration::ValidatingWebhookConfigurationResourcePanel;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ResourceLoadRequest {
    pub(crate) request_id: u64,
    pub(crate) cluster_id: ClusterId,
    pub(crate) kind: ResourceLoadKind,
}

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
    ApplyResource {
        resource: ResourceRef,
        namespace: Option<String>,
        name: String,
        manifest: serde_json::Value,
    },
    PatchResource {
        resource: ResourceRef,
        namespace: Option<String>,
        name: String,
        patch: serde_json::Value,
    },
    DeleteResource {
        resource: ResourceRef,
        namespace: Option<String>,
        name: String,
    },
    BatchDeleteResources {
        resource: ResourceRef,
        targets: Vec<ResourceDeleteTarget>,
    },
    EvictPod {
        namespace: String,
        name: String,
    },
    CordonNode {
        name: String,
    },
    DrainNode {
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
    pub(crate) execs: Vec<PodExecRequest>,
    pub(crate) attach_inputs: Vec<PodAttachInputRequest>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum ResourceLoadKind {
    Namespaces,
    Nodes,
    ClusterRoleBindings,
    ClusterRoles,
    ConfigMaps { namespace: Option<String> },
    EndpointSlices { namespace: Option<String> },
    Endpoints { namespace: Option<String> },
    Events { namespace: Option<String> },
    HorizontalPodAutoscalers { namespace: Option<String> },
    CronJobs { namespace: Option<String> },
    DaemonSets { namespace: Option<String> },
    Deployments { namespace: Option<String> },
    Jobs { namespace: Option<String> },
    Leases { namespace: Option<String> },
    LimitRanges { namespace: Option<String> },
    IngressClasses,
    Ingresses { namespace: Option<String> },
    MutatingWebhookConfigurations,
    NetworkPolicies { namespace: Option<String> },
    PersistentVolumeClaims { namespace: Option<String> },
    PersistentVolumes,
    PodDisruptionBudgets { namespace: Option<String> },
    PriorityClasses,
    ReplicaSets { namespace: Option<String> },
    ResourceQuotas { namespace: Option<String> },
    RoleBindings { namespace: Option<String> },
    Roles { namespace: Option<String> },
    Secrets { namespace: Option<String> },
    ServiceAccounts { namespace: Option<String> },
    Services { namespace: Option<String> },
    StorageClasses,
    StatefulSets { namespace: Option<String> },
    RuntimeClasses,
    ValidatingWebhookConfigurations,
    Pods { namespace: Option<String> },
    CustomResourceDefinitions,
    CustomResources { resource: ResourceRef },
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PodExecRequest {
    pub(crate) request_id: u64,
    pub(crate) cluster_id: ClusterId,
    pub(crate) namespace: String,
    pub(crate) pod: String,
    pub(crate) container: Option<String>,
    pub(crate) command: Vec<String>,
    pub(crate) tty: bool,
}

impl PodExecRequest {
    pub(crate) fn query(&self) -> ApiPodExecRequest {
        ApiPodExecRequest {
            cluster_id: self.cluster_id.clone(),
            namespace: self.namespace.clone(),
            pod: self.pod.clone(),
            container: self.container.clone(),
            command: self.command.clone(),
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
    pub(crate) fn key(&self) -> ResourceWatchKey {
        let kind = match &self.kind {
            ResourceLoadKind::Namespaces => ResourceLoadKind::Namespaces,
            ResourceLoadKind::Nodes => ResourceLoadKind::Nodes,
            ResourceLoadKind::ClusterRoleBindings => ResourceLoadKind::ClusterRoleBindings,
            ResourceLoadKind::ClusterRoles => ResourceLoadKind::ClusterRoles,
            ResourceLoadKind::ConfigMaps { .. } => ResourceLoadKind::ConfigMaps { namespace: None },
            ResourceLoadKind::EndpointSlices { .. } => {
                ResourceLoadKind::EndpointSlices { namespace: None }
            }
            ResourceLoadKind::Endpoints { .. } => ResourceLoadKind::Endpoints { namespace: None },
            ResourceLoadKind::Events { .. } => ResourceLoadKind::Events { namespace: None },
            ResourceLoadKind::HorizontalPodAutoscalers { .. } => {
                ResourceLoadKind::HorizontalPodAutoscalers { namespace: None }
            }
            ResourceLoadKind::CronJobs { .. } => ResourceLoadKind::CronJobs { namespace: None },
            ResourceLoadKind::DaemonSets { .. } => ResourceLoadKind::DaemonSets { namespace: None },
            ResourceLoadKind::Deployments { .. } => {
                ResourceLoadKind::Deployments { namespace: None }
            }
            ResourceLoadKind::StatefulSets { .. } => {
                ResourceLoadKind::StatefulSets { namespace: None }
            }
            ResourceLoadKind::Jobs { .. } => ResourceLoadKind::Jobs { namespace: None },
            ResourceLoadKind::Leases { .. } => ResourceLoadKind::Leases { namespace: None },
            ResourceLoadKind::LimitRanges { .. } => {
                ResourceLoadKind::LimitRanges { namespace: None }
            }
            ResourceLoadKind::IngressClasses => ResourceLoadKind::IngressClasses,
            ResourceLoadKind::Ingresses { .. } => ResourceLoadKind::Ingresses { namespace: None },
            ResourceLoadKind::MutatingWebhookConfigurations => {
                ResourceLoadKind::MutatingWebhookConfigurations
            }
            ResourceLoadKind::NetworkPolicies { .. } => {
                ResourceLoadKind::NetworkPolicies { namespace: None }
            }
            ResourceLoadKind::PersistentVolumeClaims { .. } => {
                ResourceLoadKind::PersistentVolumeClaims { namespace: None }
            }
            ResourceLoadKind::PersistentVolumes => ResourceLoadKind::PersistentVolumes,
            ResourceLoadKind::PodDisruptionBudgets { .. } => {
                ResourceLoadKind::PodDisruptionBudgets { namespace: None }
            }
            ResourceLoadKind::PriorityClasses => ResourceLoadKind::PriorityClasses,
            ResourceLoadKind::ReplicaSets { .. } => {
                ResourceLoadKind::ReplicaSets { namespace: None }
            }
            ResourceLoadKind::ResourceQuotas { .. } => {
                ResourceLoadKind::ResourceQuotas { namespace: None }
            }
            ResourceLoadKind::RoleBindings { .. } => {
                ResourceLoadKind::RoleBindings { namespace: None }
            }
            ResourceLoadKind::Roles { .. } => ResourceLoadKind::Roles { namespace: None },
            ResourceLoadKind::Secrets { .. } => ResourceLoadKind::Secrets { namespace: None },
            ResourceLoadKind::ServiceAccounts { .. } => {
                ResourceLoadKind::ServiceAccounts { namespace: None }
            }
            ResourceLoadKind::Services { .. } => ResourceLoadKind::Services { namespace: None },
            ResourceLoadKind::StorageClasses => ResourceLoadKind::StorageClasses,
            ResourceLoadKind::RuntimeClasses => ResourceLoadKind::RuntimeClasses,
            ResourceLoadKind::ValidatingWebhookConfigurations => {
                ResourceLoadKind::ValidatingWebhookConfigurations
            }
            ResourceLoadKind::Pods { .. } => ResourceLoadKind::Pods { namespace: None },
            ResourceLoadKind::CustomResourceDefinitions => {
                ResourceLoadKind::CustomResourceDefinitions
            }
            ResourceLoadKind::CustomResources { resource } => ResourceLoadKind::CustomResources {
                resource: resource.clone(),
            },
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
            ResourceActionKind::ApplyResource {
                resource,
                namespace,
                name,
                manifest,
            } => Some(ResourceApplyRequest {
                cluster_id: self.cluster_id.clone(),
                resource: resource.clone(),
                namespace: namespace.clone(),
                name: name.clone(),
                manifest: manifest.clone(),
            }),
            ResourceActionKind::DeleteResource { .. }
            | ResourceActionKind::PatchResource { .. }
            | ResourceActionKind::BatchDeleteResources { .. }
            | ResourceActionKind::EvictPod { .. }
            | ResourceActionKind::CordonNode { .. }
            | ResourceActionKind::DrainNode { .. } => None,
        }
    }

    pub(crate) fn patch_request(&self) -> Option<ResourcePatchRequest> {
        match &self.kind {
            ResourceActionKind::PatchResource {
                resource,
                namespace,
                name,
                patch,
            } => Some(ResourcePatchRequest {
                cluster_id: self.cluster_id.clone(),
                resource: resource.clone(),
                namespace: namespace.clone(),
                name: name.clone(),
                patch: patch.clone(),
            }),
            ResourceActionKind::ApplyResource { .. }
            | ResourceActionKind::DeleteResource { .. }
            | ResourceActionKind::BatchDeleteResources { .. }
            | ResourceActionKind::EvictPod { .. }
            | ResourceActionKind::CordonNode { .. }
            | ResourceActionKind::DrainNode { .. } => None,
        }
    }

    pub(crate) fn delete_request(&self) -> Option<ResourceDeleteRequest> {
        match &self.kind {
            ResourceActionKind::ApplyResource { .. }
            | ResourceActionKind::PatchResource { .. }
            | ResourceActionKind::BatchDeleteResources { .. }
            | ResourceActionKind::EvictPod { .. }
            | ResourceActionKind::CordonNode { .. }
            | ResourceActionKind::DrainNode { .. } => None,
            ResourceActionKind::DeleteResource {
                resource,
                namespace,
                name,
            } => Some(ResourceDeleteRequest {
                cluster_id: self.cluster_id.clone(),
                resource: resource.clone(),
                namespace: namespace.clone(),
                name: name.clone(),
            }),
        }
    }

    pub(crate) fn batch_delete_requests(&self) -> Option<Vec<ResourceDeleteRequest>> {
        let ResourceActionKind::BatchDeleteResources { resource, targets } = &self.kind else {
            return None;
        };

        Some(
            targets
                .iter()
                .map(|target| ResourceDeleteRequest {
                    cluster_id: self.cluster_id.clone(),
                    resource: resource.clone(),
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
            ResourceActionKind::ApplyResource { .. }
            | ResourceActionKind::PatchResource { .. }
            | ResourceActionKind::DeleteResource { .. }
            | ResourceActionKind::BatchDeleteResources { .. }
            | ResourceActionKind::CordonNode { .. }
            | ResourceActionKind::DrainNode { .. } => None,
        }
    }

    pub(crate) fn cordon_node_request(&self) -> Option<NodeCordonRequest> {
        match &self.kind {
            ResourceActionKind::CordonNode { name } => Some(NodeCordonRequest {
                cluster_id: self.cluster_id.clone(),
                node: name.clone(),
            }),
            ResourceActionKind::ApplyResource { .. }
            | ResourceActionKind::PatchResource { .. }
            | ResourceActionKind::DeleteResource { .. }
            | ResourceActionKind::BatchDeleteResources { .. }
            | ResourceActionKind::EvictPod { .. }
            | ResourceActionKind::DrainNode { .. } => None,
        }
    }

    pub(crate) fn drain_node_request(&self) -> Option<NodeDrainRequest> {
        match &self.kind {
            ResourceActionKind::DrainNode { name } => Some(NodeDrainRequest {
                cluster_id: self.cluster_id.clone(),
                node: name.clone(),
            }),
            ResourceActionKind::ApplyResource { .. }
            | ResourceActionKind::PatchResource { .. }
            | ResourceActionKind::DeleteResource { .. }
            | ResourceActionKind::BatchDeleteResources { .. }
            | ResourceActionKind::EvictPod { .. }
            | ResourceActionKind::CordonNode { .. } => None,
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
    PodExecConnected {
        request: PodExecRequest,
        result: Result<futures::channel::mpsc::UnboundedSender<PodAttachInput>, String>,
    },
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    PodExecOutput {
        request: PodExecRequest,
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
            Self::PodExecConnected { request, .. } => &request.cluster_id,
            Self::PodExecOutput { request, .. } => &request.cluster_id,
            Self::ResourceWatchUpdated { request, .. } => &request.cluster_id,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ResourceActionOutcome {
    Applied(ResourceSummary),
    Patched(ResourceSummary),
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
        ResourceLoadKind::ClusterRoleBindings => miku_api::ResourceQuery::new(
            cluster_id,
            ResourceRef::grouped("rbac.authorization.k8s.io", "v1", "clusterrolebindings")
                .cluster_scoped(),
        ),
        ResourceLoadKind::ClusterRoles => miku_api::ResourceQuery::new(
            cluster_id,
            ResourceRef::grouped("rbac.authorization.k8s.io", "v1", "clusterroles")
                .cluster_scoped(),
        ),
        ResourceLoadKind::ConfigMaps { namespace } => {
            let mut query =
                miku_api::ResourceQuery::new(cluster_id, ResourceRef::core("v1", "configmaps"));
            if let Some(namespace) = namespace {
                query = query.namespace(namespace.clone());
            }
            query
        }
        ResourceLoadKind::EndpointSlices { namespace } => {
            let mut query = miku_api::ResourceQuery::new(
                cluster_id,
                ResourceRef::grouped("discovery.k8s.io", "v1", "endpointslices"),
            );
            if let Some(namespace) = namespace {
                query = query.namespace(namespace.clone());
            }
            query
        }
        ResourceLoadKind::Endpoints { namespace } => {
            let mut query =
                miku_api::ResourceQuery::new(cluster_id, ResourceRef::core("v1", "endpoints"));
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
        ResourceLoadKind::HorizontalPodAutoscalers { namespace } => {
            let mut query = miku_api::ResourceQuery::new(
                cluster_id,
                ResourceRef::grouped("autoscaling", "v2", "horizontalpodautoscalers"),
            );
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
        ResourceLoadKind::Leases { namespace } => {
            let mut query = miku_api::ResourceQuery::new(
                cluster_id,
                ResourceRef::grouped("coordination.k8s.io", "v1", "leases"),
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
        ResourceLoadKind::IngressClasses => miku_api::ResourceQuery::new(
            cluster_id,
            ResourceRef::grouped("networking.k8s.io", "v1", "ingressclasses").cluster_scoped(),
        ),
        ResourceLoadKind::Ingresses { namespace } => {
            let mut query = miku_api::ResourceQuery::new(
                cluster_id,
                ResourceRef::grouped("networking.k8s.io", "v1", "ingresses"),
            );
            if let Some(namespace) = namespace {
                query = query.namespace(namespace.clone());
            }
            query
        }
        ResourceLoadKind::MutatingWebhookConfigurations => miku_api::ResourceQuery::new(
            cluster_id,
            ResourceRef::grouped(
                "admissionregistration.k8s.io",
                "v1",
                "mutatingwebhookconfigurations",
            )
            .cluster_scoped(),
        ),
        ResourceLoadKind::NetworkPolicies { namespace } => {
            let mut query = miku_api::ResourceQuery::new(
                cluster_id,
                ResourceRef::grouped("networking.k8s.io", "v1", "networkpolicies"),
            );
            if let Some(namespace) = namespace {
                query = query.namespace(namespace.clone());
            }
            query
        }
        ResourceLoadKind::PersistentVolumeClaims { namespace } => {
            let mut query = miku_api::ResourceQuery::new(
                cluster_id,
                ResourceRef::core("v1", "persistentvolumeclaims"),
            );
            if let Some(namespace) = namespace {
                query = query.namespace(namespace.clone());
            }
            query
        }
        ResourceLoadKind::PersistentVolumes => miku_api::ResourceQuery::new(
            cluster_id,
            ResourceRef::core("v1", "persistentvolumes").cluster_scoped(),
        ),
        ResourceLoadKind::PodDisruptionBudgets { namespace } => {
            let mut query = miku_api::ResourceQuery::new(
                cluster_id,
                ResourceRef::grouped("policy", "v1", "poddisruptionbudgets"),
            );
            if let Some(namespace) = namespace {
                query = query.namespace(namespace.clone());
            }
            query
        }
        ResourceLoadKind::PriorityClasses => miku_api::ResourceQuery::new(
            cluster_id,
            ResourceRef::grouped("scheduling.k8s.io", "v1", "priorityclasses").cluster_scoped(),
        ),
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
        ResourceLoadKind::RoleBindings { namespace } => {
            let mut query = miku_api::ResourceQuery::new(
                cluster_id,
                ResourceRef::grouped("rbac.authorization.k8s.io", "v1", "rolebindings"),
            );
            if let Some(namespace) = namespace {
                query = query.namespace(namespace.clone());
            }
            query
        }
        ResourceLoadKind::Roles { namespace } => {
            let mut query = miku_api::ResourceQuery::new(
                cluster_id,
                ResourceRef::grouped("rbac.authorization.k8s.io", "v1", "roles"),
            );
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
        ResourceLoadKind::Services { namespace } => {
            let mut query =
                miku_api::ResourceQuery::new(cluster_id, ResourceRef::core("v1", "services"));
            if let Some(namespace) = namespace {
                query = query.namespace(namespace.clone());
            }
            query
        }
        ResourceLoadKind::ServiceAccounts { namespace } => {
            let mut query = miku_api::ResourceQuery::new(
                cluster_id,
                ResourceRef::core("v1", "serviceaccounts"),
            );
            if let Some(namespace) = namespace {
                query = query.namespace(namespace.clone());
            }
            query
        }
        ResourceLoadKind::StorageClasses => miku_api::ResourceQuery::new(
            cluster_id,
            ResourceRef::grouped("storage.k8s.io", "v1", "storageclasses").cluster_scoped(),
        ),
        ResourceLoadKind::RuntimeClasses => miku_api::ResourceQuery::new(
            cluster_id,
            ResourceRef::grouped("node.k8s.io", "v1", "runtimeclasses").cluster_scoped(),
        ),
        ResourceLoadKind::ValidatingWebhookConfigurations => miku_api::ResourceQuery::new(
            cluster_id,
            ResourceRef::grouped(
                "admissionregistration.k8s.io",
                "v1",
                "validatingwebhookconfigurations",
            )
            .cluster_scoped(),
        ),
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
        ResourceLoadKind::CustomResources { resource } => {
            miku_api::ResourceQuery::new(cluster_id, resource.clone())
        }
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
    fn custom_resources_query_uses_selected_dynamic_resource() {
        let resource = ResourceRef::grouped("example.com", "v1", "widgets").cluster_scoped();
        let query = resource_query_for_kind(
            ClusterId::new("local"),
            &ResourceLoadKind::CustomResources {
                resource: resource.clone(),
            },
        );

        assert_eq!(query.resource, resource);
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
    fn service_accounts_query_uses_core_api_and_selected_namespace() {
        let query = resource_query_for_kind(
            ClusterId::new("local"),
            &ResourceLoadKind::ServiceAccounts {
                namespace: Some("production".to_owned()),
            },
        );

        assert_eq!(query.resource, ResourceRef::core("v1", "serviceaccounts"));
        assert_eq!(query.namespace.as_deref(), Some("production"));
    }

    #[test]
    fn roles_query_uses_rbac_api_and_selected_namespace() {
        let query = resource_query_for_kind(
            ClusterId::new("local"),
            &ResourceLoadKind::Roles {
                namespace: Some("production".to_owned()),
            },
        );

        assert_eq!(
            query.resource,
            ResourceRef::grouped("rbac.authorization.k8s.io", "v1", "roles")
        );
        assert_eq!(query.namespace.as_deref(), Some("production"));
    }

    #[test]
    fn role_bindings_query_uses_rbac_api_and_selected_namespace() {
        let query = resource_query_for_kind(
            ClusterId::new("local"),
            &ResourceLoadKind::RoleBindings {
                namespace: Some("production".to_owned()),
            },
        );

        assert_eq!(
            query.resource,
            ResourceRef::grouped("rbac.authorization.k8s.io", "v1", "rolebindings")
        );
        assert_eq!(query.namespace.as_deref(), Some("production"));
    }

    #[test]
    fn cluster_roles_query_uses_cluster_scoped_rbac_api() {
        let query =
            resource_query_for_kind(ClusterId::new("local"), &ResourceLoadKind::ClusterRoles);

        assert_eq!(
            query.resource,
            ResourceRef::grouped("rbac.authorization.k8s.io", "v1", "clusterroles")
                .cluster_scoped()
        );
        assert_eq!(query.namespace, None);
    }

    #[test]
    fn cluster_role_bindings_query_uses_cluster_scoped_rbac_api() {
        let query = resource_query_for_kind(
            ClusterId::new("local"),
            &ResourceLoadKind::ClusterRoleBindings,
        );

        assert_eq!(
            query.resource,
            ResourceRef::grouped("rbac.authorization.k8s.io", "v1", "clusterrolebindings")
                .cluster_scoped()
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
    fn horizontal_pod_autoscalers_query_uses_autoscaling_api_and_selected_namespace() {
        let query = resource_query_for_kind(
            ClusterId::new("local"),
            &ResourceLoadKind::HorizontalPodAutoscalers {
                namespace: Some("production".to_owned()),
            },
        );

        assert_eq!(
            query.resource,
            ResourceRef::grouped("autoscaling", "v2", "horizontalpodautoscalers")
        );
        assert_eq!(query.namespace.as_deref(), Some("production"));
    }

    #[test]
    fn pod_disruption_budgets_query_uses_policy_api_and_selected_namespace() {
        let query = resource_query_for_kind(
            ClusterId::new("local"),
            &ResourceLoadKind::PodDisruptionBudgets {
                namespace: Some("production".to_owned()),
            },
        );

        assert_eq!(
            query.resource,
            ResourceRef::grouped("policy", "v1", "poddisruptionbudgets")
        );
        assert_eq!(query.namespace.as_deref(), Some("production"));
    }

    #[test]
    fn leases_query_uses_coordination_api_and_selected_namespace() {
        let query = resource_query_for_kind(
            ClusterId::new("local"),
            &ResourceLoadKind::Leases {
                namespace: Some("production".to_owned()),
            },
        );

        assert_eq!(
            query.resource,
            ResourceRef::grouped("coordination.k8s.io", "v1", "leases")
        );
        assert_eq!(query.namespace.as_deref(), Some("production"));
    }

    #[test]
    fn priority_classes_query_uses_cluster_scoped_scheduling_api() {
        let query =
            resource_query_for_kind(ClusterId::new("local"), &ResourceLoadKind::PriorityClasses);

        assert_eq!(
            query.resource,
            ResourceRef::grouped("scheduling.k8s.io", "v1", "priorityclasses").cluster_scoped()
        );
        assert_eq!(query.namespace, None);
    }

    #[test]
    fn runtime_classes_query_uses_cluster_scoped_node_api() {
        let query =
            resource_query_for_kind(ClusterId::new("local"), &ResourceLoadKind::RuntimeClasses);

        assert_eq!(
            query.resource,
            ResourceRef::grouped("node.k8s.io", "v1", "runtimeclasses").cluster_scoped()
        );
        assert_eq!(query.namespace, None);
    }

    #[test]
    fn mutating_webhook_configurations_query_uses_cluster_scoped_admission_api() {
        let query = resource_query_for_kind(
            ClusterId::new("local"),
            &ResourceLoadKind::MutatingWebhookConfigurations,
        );

        assert_eq!(
            query.resource,
            ResourceRef::grouped(
                "admissionregistration.k8s.io",
                "v1",
                "mutatingwebhookconfigurations"
            )
            .cluster_scoped()
        );
        assert_eq!(query.namespace, None);
    }

    #[test]
    fn validating_webhook_configurations_query_uses_cluster_scoped_admission_api() {
        let query = resource_query_for_kind(
            ClusterId::new("local"),
            &ResourceLoadKind::ValidatingWebhookConfigurations,
        );

        assert_eq!(
            query.resource,
            ResourceRef::grouped(
                "admissionregistration.k8s.io",
                "v1",
                "validatingwebhookconfigurations"
            )
            .cluster_scoped()
        );
        assert_eq!(query.namespace, None);
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
    fn services_query_uses_core_api_and_selected_namespace() {
        let query = resource_query_for_kind(
            ClusterId::new("local"),
            &ResourceLoadKind::Services {
                namespace: Some("production".to_owned()),
            },
        );

        assert_eq!(query.resource, ResourceRef::core("v1", "services"));
        assert_eq!(query.namespace.as_deref(), Some("production"));
    }

    #[test]
    fn endpoint_slices_query_uses_discovery_api_and_selected_namespace() {
        let query = resource_query_for_kind(
            ClusterId::new("local"),
            &ResourceLoadKind::EndpointSlices {
                namespace: Some("production".to_owned()),
            },
        );

        assert_eq!(
            query.resource,
            ResourceRef::grouped("discovery.k8s.io", "v1", "endpointslices")
        );
        assert_eq!(query.namespace.as_deref(), Some("production"));
    }

    #[test]
    fn endpoints_query_uses_core_api_and_selected_namespace() {
        let query = resource_query_for_kind(
            ClusterId::new("local"),
            &ResourceLoadKind::Endpoints {
                namespace: Some("production".to_owned()),
            },
        );

        assert_eq!(query.resource, ResourceRef::core("v1", "endpoints"));
        assert_eq!(query.namespace.as_deref(), Some("production"));
    }

    #[test]
    fn ingresses_query_uses_networking_api_and_selected_namespace() {
        let query = resource_query_for_kind(
            ClusterId::new("local"),
            &ResourceLoadKind::Ingresses {
                namespace: Some("production".to_owned()),
            },
        );

        assert_eq!(
            query.resource,
            ResourceRef::grouped("networking.k8s.io", "v1", "ingresses")
        );
        assert_eq!(query.namespace.as_deref(), Some("production"));
    }

    #[test]
    fn ingress_classes_query_uses_cluster_scoped_networking_api() {
        let query =
            resource_query_for_kind(ClusterId::new("local"), &ResourceLoadKind::IngressClasses);

        assert_eq!(
            query.resource,
            ResourceRef::grouped("networking.k8s.io", "v1", "ingressclasses").cluster_scoped()
        );
        assert_eq!(query.namespace, None);
    }

    #[test]
    fn network_policies_query_uses_networking_api_and_selected_namespace() {
        let query = resource_query_for_kind(
            ClusterId::new("local"),
            &ResourceLoadKind::NetworkPolicies {
                namespace: Some("production".to_owned()),
            },
        );

        assert_eq!(
            query.resource,
            ResourceRef::grouped("networking.k8s.io", "v1", "networkpolicies")
        );
        assert_eq!(query.namespace.as_deref(), Some("production"));
    }

    #[test]
    fn persistent_volume_claims_query_uses_core_api_without_namespace_by_default() {
        let query = resource_query_for_kind(
            ClusterId::new("local"),
            &ResourceLoadKind::PersistentVolumeClaims { namespace: None },
        );

        assert_eq!(
            query.resource,
            ResourceRef::core("v1", "persistentvolumeclaims")
        );
        assert_eq!(query.namespace, None);
    }

    #[test]
    fn persistent_volume_claims_query_uses_selected_namespace() {
        let query = resource_query_for_kind(
            ClusterId::new("local"),
            &ResourceLoadKind::PersistentVolumeClaims {
                namespace: Some("production".to_owned()),
            },
        );

        assert_eq!(
            query.resource,
            ResourceRef::core("v1", "persistentvolumeclaims")
        );
        assert_eq!(query.namespace.as_deref(), Some("production"));
    }

    #[test]
    fn persistent_volumes_query_uses_cluster_scoped_core_api() {
        let query = resource_query_for_kind(
            ClusterId::new("local"),
            &ResourceLoadKind::PersistentVolumes,
        );

        assert_eq!(
            query.resource,
            ResourceRef::core("v1", "persistentvolumes").cluster_scoped()
        );
        assert_eq!(query.namespace, None);
    }

    #[test]
    fn storage_classes_query_uses_cluster_scoped_storage_api() {
        let query =
            resource_query_for_kind(ClusterId::new("local"), &ResourceLoadKind::StorageClasses);

        assert_eq!(
            query.resource,
            ResourceRef::grouped("storage.k8s.io", "v1", "storageclasses").cluster_scoped()
        );
        assert_eq!(query.namespace, None);
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

    #[test]
    fn generic_apply_action_builds_resource_apply_request() {
        let request = ResourceActionRequest {
            request_id: 7,
            cluster_id: ClusterId::new("local"),
            kind: ResourceActionKind::ApplyResource {
                resource: ResourceRef::grouped("apps", "v1", "deployments"),
                namespace: Some("production".to_owned()),
                name: "api".to_owned(),
                manifest: serde_json::json!({
                    "apiVersion": "apps/v1",
                    "kind": "Deployment",
                    "metadata": {"name": "api", "namespace": "production"}
                }),
            },
        };

        let apply = request.apply_request().unwrap();

        assert_eq!(apply.cluster_id, ClusterId::new("local"));
        assert_eq!(
            apply.resource,
            ResourceRef::grouped("apps", "v1", "deployments")
        );
        assert_eq!(apply.namespace.as_deref(), Some("production"));
        assert_eq!(apply.name, "api");
        assert_eq!(apply.manifest["kind"], "Deployment");
    }

    #[test]
    fn generic_delete_action_builds_cluster_scoped_delete_request() {
        let request = ResourceActionRequest {
            request_id: 8,
            cluster_id: ClusterId::new("local"),
            kind: ResourceActionKind::DeleteResource {
                resource: ResourceRef::core("v1", "persistentvolumes").cluster_scoped(),
                namespace: None,
                name: "pv-fast".to_owned(),
            },
        };

        let delete = request.delete_request().unwrap();

        assert_eq!(
            delete.resource,
            ResourceRef::core("v1", "persistentvolumes").cluster_scoped()
        );
        assert_eq!(delete.namespace, None);
        assert_eq!(delete.name, "pv-fast");
    }

    #[test]
    fn generic_patch_action_builds_resource_patch_request() {
        let request = ResourceActionRequest {
            request_id: 12,
            cluster_id: ClusterId::new("local"),
            kind: ResourceActionKind::PatchResource {
                resource: ResourceRef::grouped("apps", "v1", "deployments"),
                namespace: Some("default".to_owned()),
                name: "api".to_owned(),
                patch: serde_json::json!({"spec": {"replicas": 4}}),
            },
        };

        let patch = request.patch_request().unwrap();

        assert_eq!(patch.cluster_id, ClusterId::new("local"));
        assert_eq!(
            patch.resource,
            ResourceRef::grouped("apps", "v1", "deployments")
        );
        assert_eq!(patch.namespace.as_deref(), Some("default"));
        assert_eq!(patch.name, "api");
        assert_eq!(patch.patch["spec"]["replicas"], 4);
    }

    #[test]
    fn generic_batch_delete_action_builds_delete_requests_for_targets() {
        let request = ResourceActionRequest {
            request_id: 9,
            cluster_id: ClusterId::new("local"),
            kind: ResourceActionKind::BatchDeleteResources {
                resource: ResourceRef::core("v1", "services"),
                targets: vec![
                    ResourceDeleteTarget {
                        namespace: Some("default".to_owned()),
                        name: "api".to_owned(),
                    },
                    ResourceDeleteTarget {
                        namespace: Some("kube-system".to_owned()),
                        name: "dns".to_owned(),
                    },
                ],
            },
        };

        let deletes = request.batch_delete_requests().unwrap();

        assert_eq!(deletes.len(), 2);
        assert_eq!(deletes[0].resource, ResourceRef::core("v1", "services"));
        assert_eq!(deletes[0].namespace.as_deref(), Some("default"));
        assert_eq!(deletes[0].name, "api");
        assert_eq!(deletes[1].namespace.as_deref(), Some("kube-system"));
        assert_eq!(deletes[1].name, "dns");
    }

    #[test]
    fn node_cordon_action_builds_node_cordon_request() {
        let request = ResourceActionRequest {
            request_id: 10,
            cluster_id: ClusterId::new("local"),
            kind: ResourceActionKind::CordonNode {
                name: "worker-1".to_owned(),
            },
        };

        let cordon = request.cordon_node_request().unwrap();

        assert_eq!(cordon.cluster_id, ClusterId::new("local"));
        assert_eq!(cordon.node, "worker-1");
    }

    #[test]
    fn node_drain_action_builds_node_drain_request() {
        let request = ResourceActionRequest {
            request_id: 11,
            cluster_id: ClusterId::new("local"),
            kind: ResourceActionKind::DrainNode {
                name: "worker-2".to_owned(),
            },
        };

        let drain = request.drain_node_request().unwrap();

        assert_eq!(drain.cluster_id, ClusterId::new("local"));
        assert_eq!(drain.node, "worker-2");
    }
}
