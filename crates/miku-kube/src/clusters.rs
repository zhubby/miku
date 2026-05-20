use async_trait::async_trait;
use miku_api::{
    ClusterConfigStore, ClusterConnectionInfo, ClusterInitializeRequest, ClusterInitializer,
    ClusterRegistry, ClusterSummary, CreateClusterRequest, LocalPreferenceStore,
};
use miku_core::ResourceRef;

use crate::client::{KubeServices, resolve_kubeconfig_context};

#[async_trait]
impl<S> ClusterRegistry for KubeServices<S>
where
    S: ClusterConfigStore + ClusterRegistry + LocalPreferenceStore + Clone + Send + Sync,
{
    #[tracing::instrument(name = "kube.list_clusters", skip(self))]
    async fn list_clusters(&self) -> miku_core::Result<Vec<ClusterSummary>> {
        let clusters = self.store.list_clusters().await?;
        tracing::debug!(count = clusters.len(), "listed clusters");
        Ok(clusters)
    }

    #[tracing::instrument(name = "kube.create_cluster", skip(self, request), fields(context = %request.context))]
    async fn create_cluster(
        &self,
        request: CreateClusterRequest,
    ) -> miku_core::Result<ClusterSummary> {
        let context = resolve_kubeconfig_context(&request.context, &request.config)?;
        let cluster = self
            .store
            .create_cluster(CreateClusterRequest {
                context,
                config: request.config,
            })
            .await?;
        self.invalidate_cluster_cache(&cluster.id).await;
        Ok(cluster)
    }
}

#[async_trait]
impl<S> ClusterInitializer for KubeServices<S>
where
    S: ClusterConfigStore + ClusterRegistry + LocalPreferenceStore + Clone + Send + Sync,
{
    #[tracing::instrument(name = "kube.initialize_cluster", skip(self), fields(cluster_id = %request.cluster_id))]
    async fn initialize_cluster(
        &self,
        request: ClusterInitializeRequest,
    ) -> miku_core::Result<ClusterConnectionInfo> {
        let cluster_id = request.cluster_id;
        let client = self.client_for_cluster(&cluster_id).await?;
        let version = client
            .apiserver_version()
            .await
            .map_err(|error| miku_core::MikuError::Kubernetes(error.to_string()))?;
        let namespaces =
            miku_api::ResourceQuery::new(cluster_id.clone(), ResourceRef::core("v1", "namespaces"));
        let cache = self
            .resource_cache
            .get_or_start(client, &namespaces)
            .await?;
        cache.wait_until_ready().await?;

        Ok(ClusterConnectionInfo {
            version: version.git_version,
            platform: (!version.platform.is_empty()).then_some(version.platform),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use miku_api::KubernetesResourceReader;

    #[tokio::test]
    async fn list_resources_rejects_unknown_cluster() {
        let temp = tempfile::tempdir().unwrap();
        let store = miku_store::SqliteStore::initialize(miku_store::StorePaths::from_root(
            temp.path().join(".miku"),
        ))
        .await
        .unwrap();
        let services = KubeServices::new_offline(store);

        let error = services
            .list_resources(miku_api::ResourceQuery::new(
                miku_core::ClusterId::new("local"),
                miku_core::ResourceRef::core("v1", "pods"),
            ))
            .await
            .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("cluster local is not configured")
        );
    }
}
