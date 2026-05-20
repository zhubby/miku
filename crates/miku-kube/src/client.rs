use kube::Config;
use kube::config::{KubeConfigOptions, Kubeconfig};
use kube::core::DynamicObject;
use miku_api::{ClusterConfigStore, ClusterRegistry, ResourceQuery};
use miku_core::ClusterId;
use std::collections::HashMap;
use tokio::sync::Mutex;

use crate::resource_cache::ResourceCacheRegistry;

#[derive(Clone)]
pub struct KubeServices<S> {
    pub(crate) store: S,
    pub(crate) default_client: Option<kube::Client>,
    pub(crate) clients: std::sync::Arc<Mutex<HashMap<ClusterId, kube::Client>>>,
    pub(crate) resource_cache: ResourceCacheRegistry,
}

impl<S> KubeServices<S> {
    pub fn new_offline(store: S) -> Self {
        tracing::info!("created offline Kubernetes services");
        Self {
            store,
            default_client: None,
            clients: std::sync::Arc::new(Mutex::new(HashMap::new())),
            resource_cache: ResourceCacheRegistry::new(),
        }
    }

    #[tracing::instrument(name = "kube.try_default_client", skip(store))]
    pub async fn try_with_default_client(store: S) -> miku_core::Result<Self> {
        let client = kube::Client::try_default()
            .await
            .map_err(|error| miku_core::MikuError::Kubernetes(error.to_string()))?;
        tracing::info!("configured default Kubernetes client");
        Ok(Self {
            store,
            default_client: Some(client),
            clients: std::sync::Arc::new(Mutex::new(HashMap::new())),
            resource_cache: ResourceCacheRegistry::new(),
        })
    }

    pub fn has_live_client(&self) -> bool {
        self.default_client.is_some()
    }

    pub(crate) async fn invalidate_cluster_cache(&self, cluster_id: &ClusterId) {
        self.resource_cache.invalidate_cluster(cluster_id).await;
    }

    pub(crate) async fn client_for_cluster(
        &self,
        cluster_id: &ClusterId,
    ) -> miku_core::Result<kube::Client>
    where
        S: ClusterConfigStore + ClusterRegistry + Send + Sync,
    {
        if let Some(client) = self.clients.lock().await.get(cluster_id).cloned() {
            return Ok(client);
        }

        let cluster = self
            .store
            .list_clusters()
            .await?
            .into_iter()
            .find(|cluster| &cluster.id == cluster_id)
            .ok_or_else(|| {
                miku_core::MikuError::Kubernetes(format!("cluster {cluster_id} is not configured"))
            })?;
        let config = self.store.get_cluster_config(cluster_id).await?;
        let client = client_for_cluster_config(&cluster.context, config.as_deref()).await?;

        self.clients
            .lock()
            .await
            .insert(cluster_id.clone(), client.clone());
        Ok(client)
    }

    pub(crate) async fn cached_snapshot(
        &self,
        client: kube::Client,
        query: ResourceQuery,
    ) -> miku_core::Result<Vec<DynamicObject>> {
        let cache = self.resource_cache.get_or_start(client, &query).await?;
        cache.wait_until_ready().await?;
        Ok(cache.snapshot(None))
    }
}

async fn client_for_cluster_config(
    context: &str,
    kubeconfig_yaml: Option<&str>,
) -> miku_core::Result<kube::Client> {
    let config = match kubeconfig_yaml.filter(|config| !config.trim().is_empty()) {
        Some(config) => {
            let kubeconfig = Kubeconfig::from_yaml(config)
                .map_err(|error| miku_core::MikuError::Kubernetes(error.to_string()))?;
            let options = kubeconfig_options_for_context(&kubeconfig, context);
            Config::from_custom_kubeconfig(kubeconfig, &options)
                .await
                .map_err(|error| miku_core::MikuError::Kubernetes(error.to_string()))?
        }
        None => {
            let options = KubeConfigOptions {
                context: Some(context.to_owned()),
                ..KubeConfigOptions::default()
            };
            Config::from_kubeconfig(&options)
                .await
                .map_err(|error| miku_core::MikuError::Kubernetes(error.to_string()))?
        }
    };
    kube::Client::try_from(config)
        .map_err(|error| miku_core::MikuError::Kubernetes(error.to_string()))
}

pub(crate) fn kubeconfig_options_for_context(
    kubeconfig: &Kubeconfig,
    context: &str,
) -> KubeConfigOptions {
    let requested_context = context.trim();
    if kubeconfig
        .contexts
        .iter()
        .any(|named_context| named_context.name == requested_context)
    {
        return KubeConfigOptions {
            context: Some(requested_context.to_owned()),
            ..KubeConfigOptions::default()
        };
    }

    KubeConfigOptions {
        context: kubeconfig.current_context.clone(),
        ..KubeConfigOptions::default()
    }
}

pub(crate) fn resolve_kubeconfig_context(
    requested_context: &str,
    kubeconfig_yaml: &str,
) -> miku_core::Result<String> {
    let requested_context = requested_context.trim();
    let kubeconfig = Kubeconfig::from_yaml(kubeconfig_yaml)
        .map_err(|error| miku_core::MikuError::Config(error.to_string()))?;

    if kubeconfig
        .contexts
        .iter()
        .any(|named_context| named_context.name == requested_context)
    {
        return Ok(requested_context.to_owned());
    }

    kubeconfig.current_context.ok_or_else(|| {
        miku_core::MikuError::Config(format!(
            "context {requested_context} was not found and kubeconfig has no current-context"
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn service_can_be_constructed_without_touching_a_cluster() {
        let temp = tempfile::tempdir().unwrap();
        let store = miku_store::SqliteStore::initialize(miku_store::StorePaths::from_root(
            temp.path().join(".miku"),
        ))
        .await
        .unwrap();

        let services = KubeServices::new_offline(store);

        assert!(!services.has_live_client());
    }

    #[test]
    fn saved_kubeconfig_uses_current_context_when_record_context_is_alias() {
        let kubeconfig = Kubeconfig::from_yaml(
            r#"
apiVersion: v1
kind: Config
current-context: real-context
contexts:
  - name: real-context
    context:
      cluster: real-cluster
      user: real-user
clusters:
  - name: real-cluster
    cluster:
      server: https://127.0.0.1:6443
users:
  - name: real-user
    user: {}
"#,
        )
        .unwrap();

        let options = kubeconfig_options_for_context(&kubeconfig, "local");

        assert_eq!(options.context.as_deref(), Some("real-context"));
    }

    #[test]
    fn saved_kubeconfig_uses_record_context_when_it_exists() {
        let kubeconfig = Kubeconfig::from_yaml(
            r#"
apiVersion: v1
kind: Config
current-context: first-context
contexts:
  - name: first-context
    context:
      cluster: first-cluster
      user: first-user
  - name: second-context
    context:
      cluster: second-cluster
      user: second-user
clusters:
  - name: first-cluster
    cluster:
      server: https://127.0.0.1:6443
  - name: second-cluster
    cluster:
      server: https://127.0.0.2:6443
users:
  - name: first-user
    user: {}
  - name: second-user
    user: {}
"#,
        )
        .unwrap();

        let options = kubeconfig_options_for_context(&kubeconfig, "second-context");

        assert_eq!(options.context.as_deref(), Some("second-context"));
    }

    #[test]
    fn imported_cluster_stores_requested_context_when_it_exists() {
        let context = resolve_kubeconfig_context(
            "second-context",
            r#"
apiVersion: v1
kind: Config
current-context: first-context
contexts:
  - name: first-context
    context:
      cluster: first-cluster
      user: first-user
  - name: second-context
    context:
      cluster: second-cluster
      user: second-user
clusters:
  - name: first-cluster
    cluster:
      server: https://127.0.0.1:6443
  - name: second-cluster
    cluster:
      server: https://127.0.0.2:6443
users:
  - name: first-user
    user: {}
  - name: second-user
    user: {}
"#,
        )
        .unwrap();

        assert_eq!(context, "second-context");
    }

    #[test]
    fn imported_cluster_stores_current_context_when_requested_context_is_alias() {
        let context = resolve_kubeconfig_context(
            "local",
            r#"
apiVersion: v1
kind: Config
current-context: real-context
contexts:
  - name: real-context
    context:
      cluster: real-cluster
      user: real-user
clusters:
  - name: real-cluster
    cluster:
      server: https://127.0.0.1:6443
users:
  - name: real-user
    user: {}
"#,
        )
        .unwrap();

        assert_eq!(context, "real-context");
    }
}
