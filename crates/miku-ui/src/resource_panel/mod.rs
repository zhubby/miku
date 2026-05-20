use miku_api::{
    LogLine, PodEvictRequest, PodLogQuery, ResourceApplyRequest, ResourceDeleteRequest,
    ResourceEvent, ResourceList, ResourceSummary,
};
use miku_core::{ClusterId, ResourceRef};

mod components;
mod pod;

pub(crate) use pod::PodResourcePanel;

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
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum ResourceLoadKind {
    Namespaces,
    Pods { namespace: Option<String> },
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
            ResourceLoadKind::Pods { .. } => ResourceLoadKind::Pods { namespace: None },
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
        ResourceLoadKind::Pods { namespace } => {
            let mut query =
                miku_api::ResourceQuery::new(cluster_id, ResourceRef::core("v1", "pods"));
            if let Some(namespace) = namespace {
                query = query.namespace(namespace.clone());
            }
            query
        }
    }
}
