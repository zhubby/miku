use std::pin::Pin;

use async_trait::async_trait;
use futures::Stream;
use miku_core::{ClusterId, ResourceRef};
use serde::{Deserialize, Serialize};

#[cfg(not(target_arch = "wasm32"))]
pub type BoxEventStream<T> = Pin<Box<dyn Stream<Item = miku_core::Result<T>> + Send>>;

#[cfg(target_arch = "wasm32")]
pub type BoxEventStream<T> = Pin<Box<dyn Stream<Item = miku_core::Result<T>>>>;

#[cfg(not(target_arch = "wasm32"))]
pub trait ServiceBounds: Send + Sync {}

#[cfg(not(target_arch = "wasm32"))]
impl<T: Send + Sync> ServiceBounds for T {}

#[cfg(target_arch = "wasm32")]
pub trait ServiceBounds {}

#[cfg(target_arch = "wasm32")]
impl<T> ServiceBounds for T {}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ClusterSummary {
    pub id: ClusterId,
    pub name: String,
    pub context: String,
    pub current: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResourceQuery {
    pub cluster_id: ClusterId,
    pub resource: ResourceRef,
    pub namespace: Option<String>,
    pub label_selector: Option<String>,
    pub limit: Option<u32>,
}

impl ResourceQuery {
    pub fn new(cluster_id: ClusterId, resource: ResourceRef) -> Self {
        Self {
            cluster_id,
            resource,
            namespace: None,
            label_selector: None,
            limit: Some(250),
        }
    }

    pub fn namespace(mut self, namespace: impl Into<String>) -> Self {
        self.namespace = Some(namespace.into());
        self
    }

    pub fn label_selector(mut self, selector: impl Into<String>) -> Self {
        self.label_selector = Some(selector.into());
        self
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ResourceList {
    pub items: Vec<ResourceSummary>,
    pub continue_token: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResourceSummary {
    pub name: String,
    pub namespace: Option<String>,
    pub kind: String,
    pub status: Option<String>,
    pub raw: serde_json::Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResourceDetail {
    pub summary: ResourceSummary,
    pub raw: serde_json::Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ResourceEvent {
    Applied(ResourceSummary),
    Deleted {
        name: String,
        namespace: Option<String>,
    },
    Restarted,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PodLogQuery {
    pub cluster_id: ClusterId,
    pub namespace: String,
    pub pod: String,
    pub container: Option<String>,
    pub tail_lines: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LogLine {
    pub text: String,
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait ClusterRegistry: ServiceBounds {
    async fn list_clusters(&self) -> miku_core::Result<Vec<ClusterSummary>>;
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait KubernetesResourceReader: ServiceBounds {
    async fn list_resources(&self, query: ResourceQuery) -> miku_core::Result<ResourceList>;

    async fn get_resource(
        &self,
        _query: ResourceQuery,
        name: &str,
    ) -> miku_core::Result<ResourceDetail> {
        Err(miku_core::MikuError::UnsupportedRuntime(format!(
            "resource detail is not implemented for {name} in this service"
        )))
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait KubernetesWatchService: ServiceBounds {
    async fn watch_resources(
        &self,
        query: ResourceQuery,
    ) -> miku_core::Result<BoxEventStream<ResourceEvent>> {
        let _ = query;
        Err(miku_core::MikuError::UnsupportedRuntime(
            "resource watch is not implemented in this service".to_owned(),
        ))
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait PodLogService: ServiceBounds {
    async fn read_logs(&self, query: PodLogQuery) -> miku_core::Result<Vec<LogLine>> {
        let _ = query;
        Err(miku_core::MikuError::UnsupportedRuntime(
            "pod logs are not implemented in this service".to_owned(),
        ))
    }

    async fn stream_logs(&self, query: PodLogQuery) -> miku_core::Result<BoxEventStream<LogLine>> {
        let _ = query;
        Err(miku_core::MikuError::UnsupportedRuntime(
            "pod log streaming is not implemented in this service".to_owned(),
        ))
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait LocalPreferenceStore: ServiceBounds {
    async fn get_preference(&self, key: &str) -> miku_core::Result<Option<serde_json::Value>>;

    async fn set_preference(&self, key: &str, value: serde_json::Value) -> miku_core::Result<()>;
}

pub trait MikuServices:
    ClusterRegistry
    + KubernetesResourceReader
    + KubernetesWatchService
    + PodLogService
    + LocalPreferenceStore
    + ServiceBounds
{
}

#[cfg(test)]
mod tests {
    use super::*;
    use miku_core::{ClusterId, ResourceRef};

    #[test]
    fn resource_query_defaults_to_no_namespace_or_selector() {
        let query = ResourceQuery::new(ClusterId::new("local"), ResourceRef::core("v1", "pods"));

        assert_eq!(query.cluster_id.as_str(), "local");
        assert!(query.namespace.is_none());
        assert!(query.label_selector.is_none());
        assert_eq!(query.limit, Some(250));
    }

    #[test]
    fn service_bundle_can_be_type_checked() {
        fn accepts_services<T: MikuServices + ?Sized>(_services: &T) {}

        struct Dummy;

        #[async_trait::async_trait]
        impl ClusterRegistry for Dummy {
            async fn list_clusters(&self) -> miku_core::Result<Vec<ClusterSummary>> {
                Ok(Vec::new())
            }
        }

        #[async_trait::async_trait]
        impl KubernetesResourceReader for Dummy {
            async fn list_resources(
                &self,
                _query: ResourceQuery,
            ) -> miku_core::Result<ResourceList> {
                Ok(ResourceList::default())
            }
        }

        #[async_trait::async_trait]
        impl KubernetesWatchService for Dummy {}

        #[async_trait::async_trait]
        impl PodLogService for Dummy {}

        #[async_trait::async_trait]
        impl LocalPreferenceStore for Dummy {
            async fn get_preference(
                &self,
                _key: &str,
            ) -> miku_core::Result<Option<serde_json::Value>> {
                Ok(None)
            }

            async fn set_preference(
                &self,
                _key: &str,
                _value: serde_json::Value,
            ) -> miku_core::Result<()> {
                Ok(())
            }
        }

        impl MikuServices for Dummy {}

        accepts_services(&Dummy);
    }
}
