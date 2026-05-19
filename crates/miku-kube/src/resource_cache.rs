use std::collections::HashMap;
use std::sync::Arc;

use futures::StreamExt;
use kube::Api;
use kube::api::ApiResource;
use kube::core::DynamicObject;
use kube::runtime::reflector::{Store, store};
use kube::runtime::{reflector, watcher};
use miku_api::ResourceQuery;
use miku_core::{ResourceRef, ResourceScope};
use tokio::sync::Mutex;

use crate::api_resource;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ResourceCacheKey {
    group: Option<String>,
    version: String,
    plural: String,
    scope: ResourceCacheScope,
    label_selector: Option<String>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum ResourceCacheScope {
    All,
    Namespace(String),
}

impl ResourceCacheKey {
    pub(crate) fn from_query(query: &ResourceQuery) -> Self {
        let scope = resolved_scope(query);
        Self {
            group: query.resource.group.clone(),
            version: query.resource.version.clone(),
            plural: query.resource.plural.clone(),
            scope,
            label_selector: query.label_selector.clone(),
        }
    }

    fn resource_ref(&self) -> ResourceRef {
        ResourceRef {
            group: self.group.clone(),
            version: self.version.clone(),
            plural: self.plural.clone(),
            scope: ResourceScope::Cluster,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ResourceCacheRegistry {
    caches: Arc<Mutex<HashMap<ResourceCacheKey, Arc<ResourceCacheEntry>>>>,
}

impl ResourceCacheRegistry {
    pub(crate) fn new() -> Self {
        Self {
            caches: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub(crate) async fn get_or_start(
        &self,
        client: kube::Client,
        query: &ResourceQuery,
    ) -> miku_core::Result<Arc<ResourceCacheEntry>> {
        let key = ResourceCacheKey::from_query(query);
        let mut caches = self.caches.lock().await;
        if let Some(cache) = caches.get(&key) {
            return Ok(cache.clone());
        }

        let cache = Arc::new(ResourceCacheEntry::start(client, key.clone())?);
        caches.insert(key, cache.clone());
        Ok(cache)
    }
}

#[derive(Debug)]
pub(crate) struct ResourceCacheEntry {
    store: Store<DynamicObject>,
    _task: ReflectorTask,
}

impl ResourceCacheEntry {
    fn start(client: kube::Client, key: ResourceCacheKey) -> miku_core::Result<Self> {
        let api_resource = api_resource(&key.resource_ref());
        let api = api_for_cache_key(client, &api_resource, &key);
        let (store, writer) = dynamic_store(api_resource);
        let mut config = watcher::Config::default();
        if let Some(selector) = &key.label_selector {
            config = config.labels(selector);
        }

        let stream = watcher(api, config);
        let reflected = reflector(writer, stream);
        let task = tokio::spawn(async move {
            futures::pin_mut!(reflected);
            while let Some(event) = reflected.next().await {
                if let Err(error) = event {
                    tracing::warn!(%error, "resource cache reflector event failed");
                }
            }
        });

        Ok(Self {
            store,
            _task: ReflectorTask(task),
        })
    }

    pub(crate) async fn wait_until_ready(&self) -> miku_core::Result<()> {
        self.store
            .wait_until_ready()
            .await
            .map_err(|error| miku_core::MikuError::Kubernetes(error.to_string()))
    }

    pub(crate) fn snapshot(&self, limit: Option<u32>) -> Vec<DynamicObject> {
        let mut objects = self
            .store
            .state()
            .into_iter()
            .map(|object| object.as_ref().clone())
            .collect::<Vec<_>>();
        objects.sort_by(|left, right| {
            let left_namespace = left.metadata.namespace.as_deref().unwrap_or_default();
            let right_namespace = right.metadata.namespace.as_deref().unwrap_or_default();
            left_namespace
                .cmp(right_namespace)
                .then_with(|| left.metadata.name.cmp(&right.metadata.name))
        });
        if let Some(limit) = limit {
            objects.truncate(limit as usize);
        }
        objects
    }
}

#[derive(Debug)]
struct ReflectorTask(tokio::task::JoinHandle<()>);

impl Drop for ReflectorTask {
    fn drop(&mut self) {
        self.0.abort();
    }
}

fn dynamic_store(
    api_resource: ApiResource,
) -> (Store<DynamicObject>, store::Writer<DynamicObject>) {
    let writer = store::Writer::new(api_resource);
    let reader = writer.as_reader();
    (reader, writer)
}

fn api_for_cache_key(
    client: kube::Client,
    api_resource: &ApiResource,
    key: &ResourceCacheKey,
) -> Api<DynamicObject> {
    match &key.scope {
        ResourceCacheScope::All => Api::all_with(client, api_resource),
        ResourceCacheScope::Namespace(namespace) => {
            Api::namespaced_with(client, namespace, api_resource)
        }
    }
}

fn resolved_scope(query: &ResourceQuery) -> ResourceCacheScope {
    if let Some(namespace) = &query.namespace {
        return ResourceCacheScope::Namespace(namespace.clone());
    }

    match &query.resource.scope {
        ResourceScope::Cluster => ResourceCacheScope::All,
        ResourceScope::Namespaced(namespace) => ResourceCacheScope::Namespace(namespace.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kube::ResourceExt;

    #[test]
    fn cache_key_distinguishes_resource_scope_and_label_selector_but_not_limit() {
        let mut first = ResourceQuery::new(
            miku_core::ClusterId::new("local"),
            ResourceRef::core("v1", "pods"),
        );
        first.namespace = Some("default".to_owned());
        first.label_selector = Some("app=api".to_owned());
        first.limit = Some(10);

        let mut second = first.clone();
        second.limit = Some(250);

        let mut different_selector = first.clone();
        different_selector.label_selector = Some("app=worker".to_owned());

        let mut different_resource = first.clone();
        different_resource.resource = ResourceRef::core("v1", "services");

        assert_eq!(
            ResourceCacheKey::from_query(&first),
            ResourceCacheKey::from_query(&second)
        );
        assert_ne!(
            ResourceCacheKey::from_query(&first),
            ResourceCacheKey::from_query(&different_selector)
        );
        assert_ne!(
            ResourceCacheKey::from_query(&first),
            ResourceCacheKey::from_query(&different_resource)
        );
    }

    #[test]
    fn explicit_query_namespace_overrides_resource_scope() {
        let query = ResourceQuery::new(
            miku_core::ClusterId::new("local"),
            ResourceRef::core("v1", "pods").namespaced("default"),
        )
        .namespace("production");

        assert_eq!(
            ResourceCacheKey::from_query(&query).scope,
            ResourceCacheScope::Namespace("production".to_owned())
        );
    }

    #[tokio::test]
    async fn snapshot_applies_limit_after_reading_store_state() {
        let api_resource = api_resource(&ResourceRef::core("v1", "pods"));
        let (reader, mut writer) = dynamic_store(api_resource.clone());
        writer.apply_watcher_event(&watcher::Event::Init);
        writer.apply_watcher_event(&watcher::Event::InitApply(pod_object(
            &api_resource,
            "default",
            "api",
        )));
        writer.apply_watcher_event(&watcher::Event::InitApply(pod_object(
            &api_resource,
            "default",
            "worker",
        )));
        writer.apply_watcher_event(&watcher::Event::InitDone);
        let entry = ResourceCacheEntry {
            store: reader,
            _task: ReflectorTask(tokio::spawn(async {})),
        };

        let snapshot = entry.snapshot(Some(1));

        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].name_any(), "api");
    }

    fn pod_object(api_resource: &ApiResource, namespace: &str, name: &str) -> DynamicObject {
        let mut object = DynamicObject::new(name, api_resource);
        object.metadata.namespace = Some(namespace.to_owned());
        object
    }
}
