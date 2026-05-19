use miku_api::{ResourceApplyRequest, ResourceDeleteRequest, ResourceList, ResourceSummary};
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
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ResourcePanelRequests {
    pub(crate) loads: Vec<ResourceLoadRequest>,
    pub(crate) actions: Vec<ResourceActionRequest>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ResourceLoadKind {
    Namespaces,
    Pods { namespace: Option<String> },
}

impl ResourceLoadRequest {
    pub(crate) fn query(&self) -> miku_api::ResourceQuery {
        match &self.kind {
            ResourceLoadKind::Namespaces => miku_api::ResourceQuery::new(
                self.cluster_id.clone(),
                ResourceRef::core("v1", "namespaces").cluster_scoped(),
            ),
            ResourceLoadKind::Pods { namespace } => {
                let mut query = miku_api::ResourceQuery::new(
                    self.cluster_id.clone(),
                    ResourceRef::core("v1", "pods"),
                );
                if let Some(namespace) = namespace {
                    query = query.namespace(namespace.clone());
                }
                query
            }
        }
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
            ResourceActionKind::DeletePod { .. } => None,
        }
    }

    pub(crate) fn delete_request(&self) -> Option<ResourceDeleteRequest> {
        match &self.kind {
            ResourceActionKind::ApplyPod { .. } => None,
            ResourceActionKind::DeletePod { namespace, name } => Some(ResourceDeleteRequest {
                cluster_id: self.cluster_id.clone(),
                resource: ResourceRef::core("v1", "pods"),
                namespace: namespace.clone(),
                name: name.clone(),
            }),
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
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ResourceActionOutcome {
    Applied(ResourceSummary),
    Deleted,
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
