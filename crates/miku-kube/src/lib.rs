mod client;
mod clusters;
mod pods;
mod resource_cache;
mod resources;
mod status;

pub use client::KubeServices;
pub use resources::{api_resource, resource_query_path};

use async_trait::async_trait;
use miku_api::{ClusterConfigStore, ClusterRegistry, LocalPreferenceStore, MikuServices};

#[async_trait]
impl<S> LocalPreferenceStore for KubeServices<S>
where
    S: LocalPreferenceStore + Clone + Send + Sync,
{
    async fn get_preference(&self, key: &str) -> miku_core::Result<Option<serde_json::Value>> {
        self.store.get_preference(key).await
    }

    async fn set_preference(&self, key: &str, value: serde_json::Value) -> miku_core::Result<()> {
        self.store.set_preference(key, value).await
    }
}

impl<S> MikuServices for KubeServices<S> where
    S: ClusterConfigStore + ClusterRegistry + LocalPreferenceStore + Clone + Send + Sync
{
}
