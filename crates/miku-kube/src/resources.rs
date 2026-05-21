use async_trait::async_trait;
use futures::StreamExt;
use kube::api::{ApiResource, DeleteParams, EvictParams, Patch, PatchParams};
use kube::core::{DynamicObject, GroupVersionKind};
use kube::{Api, ResourceExt};
use miku_api::{
    ClusterConfigStore, ClusterRegistry, KubernetesResourceReader, KubernetesResourceWriter,
    KubernetesWatchService, LocalPreferenceStore, PodEvictRequest, ResourceApplyRequest,
    ResourceDeleteRequest, ResourceDetail, ResourceEvent, ResourceList, ResourceQuery,
    ResourceSummary,
};
use miku_core::{ResourceRef, ResourceScope};

use crate::client::KubeServices;

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
        "cronjobs" => "CronJob".to_owned(),
        "daemonsets" => "DaemonSet".to_owned(),
        "deployments" => "Deployment".to_owned(),
        "jobs" => "Job".to_owned(),
        "replicasets" => "ReplicaSet".to_owned(),
        "statefulsets" => "StatefulSet".to_owned(),
        "namespaces" => "Namespace".to_owned(),
        "configmaps" => "ConfigMap".to_owned(),
        "limitranges" => "LimitRange".to_owned(),
        "resourcequotas" => "ResourceQuota".to_owned(),
        "secrets" => "Secret".to_owned(),
        "customresourcedefinitions" => "CustomResourceDefinition".to_owned(),
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

pub(crate) fn dynamic_api(
    client: kube::Client,
    resource: &ResourceRef,
    namespace: Option<&str>,
) -> Api<DynamicObject> {
    let api_resource = api_resource(resource);
    match namespace {
        Some(namespace) => Api::namespaced_with(client, namespace, &api_resource),
        None => match &resource.scope {
            ResourceScope::Namespaced(namespace) => {
                Api::namespaced_with(client, namespace, &api_resource)
            }
            ResourceScope::Cluster => Api::all_with(client, &api_resource),
        },
    }
}

#[async_trait]
impl<S> KubernetesResourceReader for KubeServices<S>
where
    S: ClusterConfigStore + ClusterRegistry + LocalPreferenceStore + Clone + Send + Sync,
{
    #[tracing::instrument(name = "kube.list_resources", skip(self), fields(path = %resource_query_path(&query)))]
    async fn list_resources(&self, query: ResourceQuery) -> miku_core::Result<ResourceList> {
        let client = self.client_for_cluster(&query.cluster_id).await?;
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
        let client = self.client_for_cluster(&query.cluster_id).await?;
        let api = dynamic_api(client, &query.resource, query.namespace.as_deref());
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

#[async_trait]
impl<S> KubernetesResourceWriter for KubeServices<S>
where
    S: ClusterConfigStore + ClusterRegistry + LocalPreferenceStore + Clone + Send + Sync,
{
    #[tracing::instrument(name = "kube.apply_resource", skip(self, request), fields(name = %request.name))]
    async fn apply_resource(
        &self,
        request: ResourceApplyRequest,
    ) -> miku_core::Result<ResourceSummary> {
        let client = self.client_for_cluster(&request.cluster_id).await?;
        let api = dynamic_api(client, &request.resource, request.namespace.as_deref());
        let params = PatchParams::apply("miku").force();
        let object = api
            .patch(&request.name, &params, &Patch::Apply(&request.manifest))
            .await
            .map_err(|error| miku_core::MikuError::Kubernetes(error.to_string()))?;

        Ok(resource_summary(object))
    }

    #[tracing::instrument(name = "kube.delete_resource", skip(self, request), fields(name = %request.name))]
    async fn delete_resource(&self, request: ResourceDeleteRequest) -> miku_core::Result<()> {
        let client = self.client_for_cluster(&request.cluster_id).await?;
        let api = dynamic_api(client, &request.resource, request.namespace.as_deref());
        api.delete(&request.name, &DeleteParams::default())
            .await
            .map_err(|error| miku_core::MikuError::Kubernetes(error.to_string()))?;

        Ok(())
    }

    #[tracing::instrument(name = "kube.evict_pod", skip(self, request), fields(namespace = %request.namespace, pod = %request.pod))]
    async fn evict_pod(&self, request: PodEvictRequest) -> miku_core::Result<()> {
        let client = self.client_for_cluster(&request.cluster_id).await?;
        let pods: Api<k8s_openapi::api::core::v1::Pod> =
            Api::namespaced(client, &request.namespace);
        pods.evict(&request.pod, &EvictParams::default())
            .await
            .map_err(|error| miku_core::MikuError::Kubernetes(error.to_string()))?;

        Ok(())
    }
}

#[async_trait]
impl<S> KubernetesWatchService for KubeServices<S>
where
    S: ClusterConfigStore + ClusterRegistry + LocalPreferenceStore + Clone + Send + Sync,
{
    #[tracing::instrument(name = "kube.watch_resources", skip(self), fields(path = %resource_query_path(&query)))]
    async fn watch_resources(
        &self,
        query: ResourceQuery,
    ) -> miku_core::Result<miku_api::BoxEventStream<ResourceEvent>> {
        let client = self.client_for_cluster(&query.cluster_id).await?;
        let cache = self.resource_cache.get_or_start(client, &query).await?;
        cache.wait_until_ready().await?;

        let changes = cache.subscribe();
        let cache = cache.clone();
        let limit = query.limit;
        let initial = Some(Ok(ResourceEvent::Snapshot(resource_list_from_snapshot(
            cache.snapshot(limit),
        ))));

        Ok(futures::stream::unfold(
            (initial, cache, limit, changes),
            |(mut initial, cache, limit, mut changes)| async move {
                if let Some(event) = initial.take() {
                    return Some((event, (initial, cache, limit, changes)));
                }

                match changes.recv().await {
                    Ok(()) => {
                        let event = ResourceEvent::Snapshot(resource_list_from_snapshot(
                            cache.snapshot(limit),
                        ));
                        Some((Ok(event), (initial, cache, limit, changes)))
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                        tracing::warn!(skipped, "resource watch receiver lagged");
                        let event = ResourceEvent::Snapshot(resource_list_from_snapshot(
                            cache.snapshot(limit),
                        ));
                        Some((Ok(event), (initial, cache, limit, changes)))
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => None,
                }
            },
        )
        .boxed())
    }
}

pub(crate) fn resource_summary(object: DynamicObject) -> ResourceSummary {
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

fn resource_list_from_snapshot(objects: Vec<DynamicObject>) -> ResourceList {
    ResourceList {
        items: objects.into_iter().map(resource_summary).collect(),
        continue_token: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kube::core::TypeMeta;

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
    fn api_resource_uses_known_kind_for_daemon_sets() {
        let resource = miku_core::ResourceRef::grouped("apps", "v1", "daemonsets");

        let api_resource = api_resource(&resource);

        assert_eq!(api_resource.kind, "DaemonSet");
        assert_eq!(api_resource.plural, "daemonsets");
    }

    #[test]
    fn api_resource_uses_known_kind_for_cron_jobs() {
        let resource = miku_core::ResourceRef::grouped("batch", "v1", "cronjobs");

        let api_resource = api_resource(&resource);

        assert_eq!(api_resource.kind, "CronJob");
        assert_eq!(api_resource.plural, "cronjobs");
    }

    #[test]
    fn api_resource_uses_known_kind_for_jobs() {
        let resource = miku_core::ResourceRef::grouped("batch", "v1", "jobs");

        let api_resource = api_resource(&resource);

        assert_eq!(api_resource.kind, "Job");
        assert_eq!(api_resource.plural, "jobs");
    }

    #[test]
    fn api_resource_uses_known_kind_for_resource_quotas() {
        let resource = miku_core::ResourceRef::core("v1", "resourcequotas");

        let api_resource = api_resource(&resource);

        assert_eq!(api_resource.kind, "ResourceQuota");
        assert_eq!(api_resource.plural, "resourcequotas");
    }

    #[test]
    fn api_resource_uses_known_kind_for_limit_ranges() {
        let resource = miku_core::ResourceRef::core("v1", "limitranges");

        let api_resource = api_resource(&resource);

        assert_eq!(api_resource.kind, "LimitRange");
        assert_eq!(api_resource.plural, "limitranges");
    }

    #[test]
    fn api_resource_uses_known_kind_for_replica_sets() {
        let resource = miku_core::ResourceRef::grouped("apps", "v1", "replicasets");

        let api_resource = api_resource(&resource);

        assert_eq!(api_resource.kind, "ReplicaSet");
        assert_eq!(api_resource.plural, "replicasets");
    }

    #[test]
    fn api_resource_uses_known_kind_for_stateful_sets() {
        let resource = miku_core::ResourceRef::grouped("apps", "v1", "statefulsets");

        let api_resource = api_resource(&resource);

        assert_eq!(api_resource.kind, "StatefulSet");
        assert_eq!(api_resource.plural, "statefulsets");
    }

    #[test]
    fn api_resource_uses_known_kind_for_custom_resource_definitions() {
        let resource = miku_core::ResourceRef::grouped(
            "apiextensions.k8s.io",
            "v1",
            "customresourcedefinitions",
        );

        let api_resource = api_resource(&resource);

        assert_eq!(api_resource.kind, "CustomResourceDefinition");
        assert_eq!(api_resource.plural, "customresourcedefinitions");
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
