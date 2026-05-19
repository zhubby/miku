use async_trait::async_trait;
use kube::api::{ApiResource, LogParams};
use kube::core::{DynamicObject, GroupVersionKind};
use kube::{Api, ResourceExt};
use miku_api::{
    ClusterRegistry, ClusterSummary, CreateClusterRequest, KubernetesResourceReader,
    KubernetesWatchService, LocalPreferenceStore, LogLine, MikuServices, PodLogQuery,
    PodLogService, ResourceDetail, ResourceList, ResourceQuery, ResourceSummary,
};
use miku_core::{ResourceRef, ResourceScope};

mod resource_cache;

use resource_cache::ResourceCacheRegistry;

#[derive(Clone)]
pub struct KubeServices<S> {
    store: S,
    client: Option<kube::Client>,
    resource_cache: ResourceCacheRegistry,
}

impl<S> KubeServices<S> {
    pub fn new_offline(store: S) -> Self {
        tracing::info!("created offline Kubernetes services");
        Self {
            store,
            client: None,
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
            client: Some(client),
            resource_cache: ResourceCacheRegistry::new(),
        })
    }

    pub fn has_live_client(&self) -> bool {
        self.client.is_some()
    }

    fn live_client(&self) -> miku_core::Result<kube::Client> {
        self.client.clone().ok_or_else(|| {
            miku_core::MikuError::Kubernetes("no live Kubernetes client is configured".to_owned())
        })
    }
}

pub fn resource_query_path(query: &ResourceQuery) -> String {
    query.resource.api_path()
}

pub fn api_resource(resource: &ResourceRef) -> ApiResource {
    let group = resource.group.as_deref().unwrap_or("");
    let kind = kind_for_plural(&resource.plural);
    let gvk = GroupVersionKind::gvk(group, &resource.version, &kind);
    ApiResource::from_gvk_with_plural(&gvk, &resource.plural)
}

fn kind_for_plural(plural: &str) -> String {
    match plural {
        "pods" => "Pod".to_owned(),
        "services" => "Service".to_owned(),
        "deployments" => "Deployment".to_owned(),
        "namespaces" => "Namespace".to_owned(),
        "configmaps" => "ConfigMap".to_owned(),
        "secrets" => "Secret".to_owned(),
        value => value
            .trim_end_matches('s')
            .split(['-', '_'])
            .filter(|part| !part.is_empty())
            .map(|part| {
                let mut chars = part.chars();
                match chars.next() {
                    Some(first) => first.to_uppercase().chain(chars).collect::<String>(),
                    None => String::new(),
                }
            })
            .collect::<String>(),
    }
}

#[async_trait]
impl<S> ClusterRegistry for KubeServices<S>
where
    S: ClusterRegistry + LocalPreferenceStore + Clone + Send + Sync,
{
    #[tracing::instrument(name = "kube.list_clusters", skip(self))]
    async fn list_clusters(&self) -> miku_core::Result<Vec<ClusterSummary>> {
        let mut clusters = self.store.list_clusters().await?;
        if self.client.is_some() {
            tracing::debug!("returning default live kubeconfig context");
            clusters.push(ClusterSummary {
                id: miku_core::ClusterId::new("default"),
                name: "Default kubeconfig context".to_owned(),
                context: "default".to_owned(),
                current: true,
            });
        }

        tracing::debug!(count = clusters.len(), "listed clusters");
        Ok(clusters)
    }

    #[tracing::instrument(name = "kube.create_cluster", skip(self, request), fields(context = %request.context))]
    async fn create_cluster(
        &self,
        request: CreateClusterRequest,
    ) -> miku_core::Result<ClusterSummary> {
        self.store.create_cluster(request).await
    }
}

#[async_trait]
impl<S> KubernetesResourceReader for KubeServices<S>
where
    S: LocalPreferenceStore + Clone + Send + Sync,
{
    #[tracing::instrument(name = "kube.list_resources", skip(self), fields(path = %resource_query_path(&query)))]
    async fn list_resources(&self, query: ResourceQuery) -> miku_core::Result<ResourceList> {
        let Some(client) = self.client.clone() else {
            return Err(miku_core::MikuError::Kubernetes(format!(
                "no live Kubernetes client is configured for {}",
                resource_query_path(&query)
            )));
        };

        let cache = self.resource_cache.get_or_start(client, &query).await?;
        cache.wait_until_ready().await?;

        Ok(ResourceList {
            items: cache
                .snapshot(query.limit)
                .into_iter()
                .map(resource_summary)
                .collect(),
            continue_token: None,
        })
    }

    #[tracing::instrument(name = "kube.get_resource", skip(self), fields(path = %resource_query_path(&query), name = %name))]
    async fn get_resource(
        &self,
        query: ResourceQuery,
        name: &str,
    ) -> miku_core::Result<ResourceDetail> {
        let client = self.live_client()?;
        let api_resource = api_resource(&query.resource);
        let api: Api<DynamicObject> = match query.namespace.as_deref() {
            Some(namespace) => Api::namespaced_with(client, namespace, &api_resource),
            None => match &query.resource.scope {
                ResourceScope::Namespaced(namespace) => {
                    Api::namespaced_with(client, namespace, &api_resource)
                }
                ResourceScope::Cluster => Api::all_with(client, &api_resource),
            },
        };
        let object = api
            .get(name)
            .await
            .map_err(|error| miku_core::MikuError::Kubernetes(error.to_string()))?;

        Ok(ResourceDetail {
            summary: resource_summary(object.clone()),
            raw: serde_json::to_value(&object).unwrap_or(serde_json::Value::Null),
        })
    }
}

fn resource_summary(object: DynamicObject) -> ResourceSummary {
    let raw = serde_json::to_value(&object).unwrap_or(serde_json::Value::Null);
    let kind = object
        .types
        .as_ref()
        .map(|type_meta| type_meta.kind.clone())
        .unwrap_or_else(|| "Unknown".to_owned());
    let status = object
        .data
        .get("status")
        .and_then(|status| status.get("phase").or_else(|| status.get("status")))
        .and_then(serde_json::Value::as_str)
        .map(ToOwned::to_owned);

    ResourceSummary {
        name: object.name_any(),
        namespace: object.namespace(),
        kind,
        status,
        raw,
    }
}

#[async_trait]
impl<S> KubernetesWatchService for KubeServices<S> where
    S: LocalPreferenceStore + Clone + Send + Sync
{
}

#[async_trait]
impl<S> PodLogService for KubeServices<S>
where
    S: LocalPreferenceStore + Clone + Send + Sync,
{
    #[tracing::instrument(name = "kube.read_logs", skip(self), fields(namespace = %query.namespace, pod = %query.pod))]
    async fn read_logs(&self, query: PodLogQuery) -> miku_core::Result<Vec<LogLine>> {
        let client = self.live_client()?;
        let pods: Api<k8s_openapi::api::core::v1::Pod> = Api::namespaced(client, &query.namespace);
        let params = log_params(&query);
        let logs = pods
            .logs(&query.pod, &params)
            .await
            .map_err(|error| miku_core::MikuError::Kubernetes(error.to_string()))?;

        Ok(logs
            .lines()
            .map(|line| LogLine {
                text: line.to_owned(),
            })
            .collect())
    }
}

fn log_params(query: &PodLogQuery) -> LogParams {
    LogParams {
        container: query.container.clone(),
        tail_lines: query.tail_lines.map(i64::from),
        ..LogParams::default()
    }
}

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
    S: ClusterRegistry + LocalPreferenceStore + Clone + Send + Sync
{
}

#[cfg(test)]
mod tests {
    use super::*;
    use kube::core::TypeMeta;
    use miku_api::KubernetesResourceReader;

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
    fn maps_resource_queries_to_kubernetes_api_paths() {
        let query = miku_api::ResourceQuery::new(
            miku_core::ClusterId::new("local"),
            miku_core::ResourceRef::core("v1", "pods").namespaced("default"),
        );

        assert_eq!(
            resource_query_path(&query),
            "/api/v1/namespaces/default/pods"
        );
    }

    #[test]
    fn api_resource_uses_known_kind_for_common_plural() {
        let resource = miku_core::ResourceRef::grouped("apps", "v1", "deployments");

        let api_resource = api_resource(&resource);

        assert_eq!(api_resource.kind, "Deployment");
        assert_eq!(api_resource.plural, "deployments");
    }

    #[test]
    fn log_params_map_container_and_tail_lines() {
        let query = miku_api::PodLogQuery {
            cluster_id: miku_core::ClusterId::new("local"),
            namespace: "default".to_owned(),
            pod: "api".to_owned(),
            container: Some("server".to_owned()),
            tail_lines: Some(100),
        };

        let params = log_params(&query);

        assert_eq!(params.container.as_deref(), Some("server"));
        assert_eq!(params.tail_lines, Some(100));
    }

    #[tokio::test]
    async fn offline_list_resources_returns_no_client_error() {
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

        assert!(error.to_string().contains("no live Kubernetes client"));
    }

    #[test]
    fn resource_summary_maps_dynamic_object_metadata_and_status() {
        let api_resource = api_resource(&miku_core::ResourceRef::core("v1", "pods"));
        let mut object = DynamicObject::new("api", &api_resource);
        object.metadata.namespace = Some("default".to_owned());
        object.types = Some(TypeMeta {
            api_version: "v1".to_owned(),
            kind: "Pod".to_owned(),
        });
        object.data = serde_json::json!({
            "status": {
                "phase": "Running"
            }
        });

        let summary = resource_summary(object);

        assert_eq!(summary.name, "api");
        assert_eq!(summary.namespace.as_deref(), Some("default"));
        assert_eq!(summary.kind, "Pod");
        assert_eq!(summary.status.as_deref(), Some("Running"));
        assert_eq!(summary.raw["metadata"]["name"], "api");
    }
}
