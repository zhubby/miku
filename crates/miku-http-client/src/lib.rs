use async_trait::async_trait;
use miku_api::{
    ClusterRegistry, ClusterSummary, CreateClusterRequest, KubernetesResourceReader,
    KubernetesWatchService, LocalPreferenceStore, MikuServices, PodLogService, ResourceList,
    ResourceQuery,
};
use url::Url;

#[derive(Clone, Debug)]
pub struct HttpMikuClient {
    base_url: Url,
    client: reqwest::Client,
}

impl HttpMikuClient {
    #[tracing::instrument(name = "http_client.new")]
    pub fn new(base_url: &str) -> miku_core::Result<Self> {
        let base_url = Url::parse(base_url)
            .map_err(|error| miku_core::MikuError::Config(error.to_string()))?;
        tracing::debug!(%base_url, "created HTTP Miku client");
        Ok(Self {
            base_url,
            client: reqwest::Client::new(),
        })
    }

    pub fn endpoint(&self, path: &str) -> Url {
        self.base_url
            .join(path.trim_start_matches('/'))
            .expect("validated base URL should join relative API paths")
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl ClusterRegistry for HttpMikuClient {
    #[tracing::instrument(name = "http_client.list_clusters", skip(self))]
    async fn list_clusters(&self) -> miku_core::Result<Vec<ClusterSummary>> {
        let endpoint = self.endpoint("/api/clusters");
        tracing::debug!(url = %endpoint, "requesting clusters");
        self.client
            .get(endpoint)
            .send()
            .await
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))?
            .error_for_status()
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))?
            .json()
            .await
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))
    }

    #[tracing::instrument(name = "http_client.create_cluster", skip(self, request), fields(context = %request.context))]
    async fn create_cluster(
        &self,
        request: CreateClusterRequest,
    ) -> miku_core::Result<ClusterSummary> {
        let endpoint = self.endpoint("/api/clusters");
        self.client
            .post(endpoint)
            .json(&request)
            .send()
            .await
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))?
            .error_for_status()
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))?
            .json()
            .await
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl KubernetesResourceReader for HttpMikuClient {
    #[tracing::instrument(name = "http_client.list_resources", skip(self, query), fields(resource = %query.resource.plural))]
    async fn list_resources(&self, query: ResourceQuery) -> miku_core::Result<ResourceList> {
        let endpoint = self.endpoint("/api/resources/list");
        self.client
            .post(endpoint)
            .json(&query)
            .send()
            .await
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))?
            .error_for_status()
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))?
            .json()
            .await
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl KubernetesWatchService for HttpMikuClient {}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl PodLogService for HttpMikuClient {}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl LocalPreferenceStore for HttpMikuClient {
    async fn get_preference(&self, _key: &str) -> miku_core::Result<Option<serde_json::Value>> {
        Err(miku_core::MikuError::UnsupportedRuntime(
            "preferences are local to the native process".to_owned(),
        ))
    }

    async fn set_preference(&self, _key: &str, _value: serde_json::Value) -> miku_core::Result<()> {
        Err(miku_core::MikuError::UnsupportedRuntime(
            "preferences are local to the native process".to_owned(),
        ))
    }
}

impl MikuServices for HttpMikuClient {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_endpoint_urls_from_base_url() {
        let client = HttpMikuClient::new("http://127.0.0.1:5174").unwrap();

        assert_eq!(
            client.endpoint("/api/clusters").as_str(),
            "http://127.0.0.1:5174/api/clusters"
        );
    }

    #[test]
    fn create_cluster_uses_cluster_collection_endpoint() {
        let client = HttpMikuClient::new("http://127.0.0.1:5174").unwrap();

        assert_eq!(
            client.endpoint("/api/clusters").as_str(),
            "http://127.0.0.1:5174/api/clusters"
        );
    }

    #[test]
    fn list_resources_uses_resource_list_endpoint() {
        let client = HttpMikuClient::new("http://127.0.0.1:5174").unwrap();

        assert_eq!(
            client.endpoint("/api/resources/list").as_str(),
            "http://127.0.0.1:5174/api/resources/list"
        );
    }
}
