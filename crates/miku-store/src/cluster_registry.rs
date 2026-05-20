use async_trait::async_trait;
use miku_api::{ClusterConfigStore, ClusterRegistry, ClusterSummary, CreateClusterRequest};
use miku_core::MikuError;
use sea_orm::{EntityTrait, QueryOrder, Set};

use crate::clusters;
use crate::store::SqliteStore;
use crate::util::{to_storage_error, unix_timestamp};

#[async_trait]
impl ClusterRegistry for SqliteStore {
    #[tracing::instrument(name = "clusters.list", skip(self))]
    async fn list_clusters(&self) -> miku_core::Result<Vec<ClusterSummary>> {
        let clusters = clusters::Entity::find()
            .order_by_asc(clusters::Column::Name)
            .all(&self.database)
            .await
            .map_err(to_storage_error)?;

        Ok(clusters
            .into_iter()
            .map(|cluster| ClusterSummary {
                id: miku_core::ClusterId::new(cluster.id),
                name: cluster.name,
                context: cluster.kube_context,
                current: false,
            })
            .collect())
    }

    #[tracing::instrument(name = "clusters.create", skip(self, request), fields(context = %request.context))]
    async fn create_cluster(
        &self,
        request: CreateClusterRequest,
    ) -> miku_core::Result<ClusterSummary> {
        let context = request.context.trim();
        let config = request.config.trim();
        if context.is_empty() {
            return Err(MikuError::Config("cluster context is required".to_owned()));
        }
        if config.is_empty() {
            return Err(MikuError::Config("cluster config is required".to_owned()));
        }

        let timestamp = unix_timestamp();
        clusters::Entity::insert(clusters::ActiveModel {
            id: Set(context.to_owned()),
            name: Set(context.to_owned()),
            kube_context: Set(context.to_owned()),
            kubeconfig_path: Set(String::new()),
            config: Set(request.config),
            default_namespace: Set(None),
            last_used_at: Set(None),
            created_at: Set(timestamp),
            updated_at: Set(timestamp),
        })
        .exec(&self.database)
        .await
        .map_err(to_storage_error)?;

        tracing::info!(context, "created cluster");
        Ok(ClusterSummary {
            id: miku_core::ClusterId::new(context),
            name: context.to_owned(),
            context: context.to_owned(),
            current: false,
        })
    }
}

#[async_trait]
impl ClusterConfigStore for SqliteStore {
    #[tracing::instrument(name = "clusters.get_config", skip(self))]
    async fn get_cluster_config(
        &self,
        cluster_id: &miku_core::ClusterId,
    ) -> miku_core::Result<Option<String>> {
        let cluster = clusters::Entity::find_by_id(cluster_id.as_str().to_owned())
            .one(&self.database)
            .await
            .map_err(to_storage_error)?;

        Ok(cluster.map(|cluster| cluster.config))
    }
}
