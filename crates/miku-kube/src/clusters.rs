use async_trait::async_trait;
use miku_api::{
    ClusterConfigStore, ClusterConnectionInfo, ClusterInitializeRequest, ClusterInitializer,
    ClusterRegistry, ClusterSummary, CreateClusterRequest, LocalPreferenceStore,
};
use miku_core::ResourceRef;

use crate::client::{
    KubeServices, in_cluster_cluster_summary, is_in_cluster_cluster_id, resolve_kubeconfig_context,
};

#[async_trait]
impl<S> ClusterRegistry for KubeServices<S>
where
    S: ClusterConfigStore + ClusterRegistry + LocalPreferenceStore + Clone + Send + Sync,
{
    #[tracing::instrument(name = "kube.list_clusters", skip(self))]
    async fn list_clusters(&self) -> miku_core::Result<Vec<ClusterSummary>> {
        let mut clusters = self.store.list_clusters().await?;
        if self.in_cluster_service_account.is_some() {
            if clusters
                .iter()
                .any(|cluster| is_in_cluster_cluster_id(cluster.id.as_str()))
            {
                tracing::warn!(
                    cluster_id = crate::client::IN_CLUSTER_CLUSTER_ID,
                    "stored cluster shadows built-in in-cluster service account"
                );
            } else {
                clusters.push(in_cluster_cluster_summary());
            }
        }
        tracing::debug!(count = clusters.len(), "listed clusters");
        Ok(clusters)
    }

    #[tracing::instrument(name = "kube.create_cluster", skip(self, request), fields(context = %request.context))]
    async fn create_cluster(
        &self,
        request: CreateClusterRequest,
    ) -> miku_core::Result<ClusterSummary> {
        if is_in_cluster_cluster_id(&request.context) {
            return Err(miku_core::MikuError::Config(format!(
                "cluster id {} is reserved for in-cluster service account",
                crate::client::IN_CLUSTER_CLUSTER_ID
            )));
        }
        let context = resolve_kubeconfig_context(&request.context, &request.config)?;
        if is_in_cluster_cluster_id(&context) {
            return Err(miku_core::MikuError::Config(format!(
                "cluster id {} is reserved for in-cluster service account",
                crate::client::IN_CLUSTER_CLUSTER_ID
            )));
        }
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
    use crate::client::{IN_CLUSTER_CLUSTER_ID, test_kube_client};
    use miku_api::KubernetesResourceReader;

    #[tokio::test]
    async fn list_clusters_includes_incluster_service_account_without_persisting_it() {
        let temp = tempfile::tempdir().unwrap();
        let store = miku_store::SqliteStore::initialize(miku_store::StorePaths::from_root(
            temp.path().join(".miku"),
        ))
        .await
        .unwrap();
        let services =
            KubeServices::new_with_incluster_client(store.clone(), test_kube_client().await);

        let clusters = services.list_clusters().await.unwrap();

        assert_eq!(clusters.len(), 1);
        assert_eq!(
            clusters[0].id,
            miku_core::ClusterId::new(IN_CLUSTER_CLUSTER_ID)
        );
        assert_eq!(clusters[0].name, "In-cluster");
        assert_eq!(clusters[0].context, "in-cluster");
        assert!(clusters[0].current);
        assert!(store.list_clusters().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn stored_cluster_shadows_incluster_service_account_summary() {
        let temp = tempfile::tempdir().unwrap();
        let store = miku_store::SqliteStore::initialize(miku_store::StorePaths::from_root(
            temp.path().join(".miku"),
        ))
        .await
        .unwrap();
        let stored = store
            .create_cluster(CreateClusterRequest {
                context: IN_CLUSTER_CLUSTER_ID.to_owned(),
                config: "apiVersion: v1".to_owned(),
            })
            .await
            .unwrap();
        let services = KubeServices::new_with_incluster_client(store, test_kube_client().await);

        let clusters = services.list_clusters().await.unwrap();

        assert_eq!(clusters, vec![stored]);
        assert!(!clusters[0].current);
    }

    #[tokio::test]
    async fn incluster_cluster_id_uses_inmemory_service_account_client() {
        let temp = tempfile::tempdir().unwrap();
        let store = miku_store::SqliteStore::initialize(miku_store::StorePaths::from_root(
            temp.path().join(".miku"),
        ))
        .await
        .unwrap();
        let services = KubeServices::new_with_incluster_client(store, test_kube_client().await);

        let client = services
            .client_for_cluster(&miku_core::ClusterId::new(IN_CLUSTER_CLUSTER_ID))
            .await
            .unwrap();

        assert_eq!(client.default_namespace(), "default");
    }

    #[tokio::test]
    async fn stored_cluster_shadowing_incluster_id_uses_stored_config_path() {
        let temp = tempfile::tempdir().unwrap();
        let store = miku_store::SqliteStore::initialize(miku_store::StorePaths::from_root(
            temp.path().join(".miku"),
        ))
        .await
        .unwrap();
        store
            .create_cluster(CreateClusterRequest {
                context: IN_CLUSTER_CLUSTER_ID.to_owned(),
                config: "not: [valid".to_owned(),
            })
            .await
            .unwrap();
        let services = KubeServices::new_with_incluster_client(store, test_kube_client().await);

        let result = services
            .client_for_cluster(&miku_core::ClusterId::new(IN_CLUSTER_CLUSTER_ID))
            .await;

        match result {
            Err(miku_core::MikuError::Kubernetes(_)) => {}
            Err(error) => panic!("expected Kubernetes error, got {error}"),
            Ok(_) => panic!("expected stored shadow config to fail"),
        }
    }

    #[tokio::test]
    async fn create_cluster_rejects_incluster_reserved_id() {
        let temp = tempfile::tempdir().unwrap();
        let store = miku_store::SqliteStore::initialize(miku_store::StorePaths::from_root(
            temp.path().join(".miku"),
        ))
        .await
        .unwrap();
        let services = KubeServices::new_offline(store);

        let error = services
            .create_cluster(CreateClusterRequest {
                context: IN_CLUSTER_CLUSTER_ID.to_owned(),
                config: "apiVersion: v1".to_owned(),
            })
            .await
            .unwrap_err();

        assert!(error.to_string().contains("reserved"));
    }

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
