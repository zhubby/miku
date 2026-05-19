use miku_api::ResourceList;
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

#[derive(Clone, Debug)]
pub(crate) enum ResourceUiEvent {
    ResourcesLoaded {
        request: ResourceLoadRequest,
        result: Result<ResourceList, String>,
    },
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
