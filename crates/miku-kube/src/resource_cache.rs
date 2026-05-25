use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::StreamExt;
use kube::Api;
use kube::api::ApiResource;
use kube::core::DynamicObject;
use kube::runtime::WatchStreamExt;
use kube::runtime::reflector::{Store, store};
use kube::runtime::{reflector, watcher};
use miku_api::ResourceQuery;
use miku_core::{ClusterId, ResourceRef, ResourceScope};
use tokio::sync::{Mutex, broadcast, mpsc};
use tokio::time::timeout;

use crate::api_resource;

const CACHE_READY_TIMEOUT: Duration = Duration::from_secs(15);
const DEFAULT_CACHE_IDLE_TTL: Duration = Duration::from_secs(5 * 60);
const DEFAULT_CACHE_SWEEP_INTERVAL: Duration = Duration::from_secs(60);
const DEFAULT_CACHE_MAX_ENTRIES: usize = 128;
const RESOURCE_CHANGE_DEBOUNCE: Duration = Duration::from_millis(150);

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ResourceCacheKey {
    cluster_id: ClusterId,
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
            cluster_id: query.cluster_id.clone(),
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
    inner: Arc<Mutex<ResourceCacheInner>>,
}

impl ResourceCacheRegistry {
    pub(crate) fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(ResourceCacheInner::new(CachePolicy::default()))),
        }
    }

    pub(crate) async fn get_or_start(
        &self,
        client: kube::Client,
        query: &ResourceQuery,
    ) -> miku_core::Result<Arc<ResourceCacheEntry>> {
        let key = ResourceCacheKey::from_query(query);
        self.get_or_start_with(key.clone(), || ResourceCacheEntry::start(client, key))
            .await
    }

    async fn get_or_start_with(
        &self,
        key: ResourceCacheKey,
        start: impl FnOnce() -> miku_core::Result<ResourceCacheEntry>,
    ) -> miku_core::Result<Arc<ResourceCacheEntry>> {
        let mut inner = self.inner.lock().await;
        let now = Instant::now();
        if let Some(slot) = inner.caches.get_mut(&key) {
            if slot.entry.is_finished() {
                tracing::debug!(?key, "discarding finished resource cache reflector");
                inner.caches.remove(&key);
            } else {
                slot.last_used = now;
                let cache = slot.entry.clone();
                inner.sweep_after_access(now);
                tracing::debug!(?key, "reusing resource cache");
                return Ok(cache);
            }
        }

        tracing::debug!(?key, "starting resource cache reflector");
        let cache = Arc::new(start()?);
        inner.caches.insert(
            key,
            CacheSlot {
                entry: cache.clone(),
                last_used: now,
            },
        );
        inner.sweep_after_access(now);
        Ok(cache)
    }

    pub(crate) async fn invalidate_cluster(&self, cluster_id: &ClusterId) {
        let mut inner = self.inner.lock().await;
        inner
            .caches
            .retain(|key, _slot| &key.cluster_id != cluster_id);
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) async fn invalidate_all(&self) {
        self.inner.lock().await.caches.clear();
    }

    #[cfg(test)]
    fn with_policy(policy: CachePolicy) -> Self {
        Self {
            inner: Arc::new(Mutex::new(ResourceCacheInner::new(policy))),
        }
    }

    #[cfg(test)]
    async fn insert_test_entry(
        &self,
        key: ResourceCacheKey,
        entry: Arc<ResourceCacheEntry>,
        last_used: Instant,
    ) {
        self.inner
            .lock()
            .await
            .caches
            .insert(key, CacheSlot { entry, last_used });
    }

    #[cfg(test)]
    async fn sweep_idle_for_tests(&self, now: Instant) {
        self.inner.lock().await.sweep_idle(now);
    }

    #[cfg(test)]
    async fn cache_count(&self) -> usize {
        self.inner.lock().await.caches.len()
    }

    #[cfg(test)]
    async fn contains_key(&self, key: &ResourceCacheKey) -> bool {
        self.inner.lock().await.caches.contains_key(key)
    }

    #[cfg(test)]
    async fn entry_for_key(&self, key: &ResourceCacheKey) -> Option<Arc<ResourceCacheEntry>> {
        self.inner
            .lock()
            .await
            .caches
            .get(key)
            .map(|slot| slot.entry.clone())
    }

    #[cfg(test)]
    async fn last_used_for_key(&self, key: &ResourceCacheKey) -> Option<Instant> {
        self.inner
            .lock()
            .await
            .caches
            .get(key)
            .map(|slot| slot.last_used)
    }
}

#[derive(Debug)]
struct ResourceCacheInner {
    caches: HashMap<ResourceCacheKey, CacheSlot>,
    policy: CachePolicy,
    last_sweep: Instant,
}

impl ResourceCacheInner {
    fn new(policy: CachePolicy) -> Self {
        Self {
            caches: HashMap::new(),
            policy,
            last_sweep: Instant::now(),
        }
    }

    fn sweep_after_access(&mut self, now: Instant) {
        if now.duration_since(self.last_sweep) >= self.policy.sweep_interval {
            self.sweep_idle(now);
            self.last_sweep = now;
        } else {
            self.enforce_max_entries();
        }
    }

    fn sweep_idle(&mut self, now: Instant) {
        let idle_ttl = self.policy.idle_ttl;
        self.caches.retain(|_key, slot| {
            Arc::strong_count(&slot.entry) > 1 || now.duration_since(slot.last_used) < idle_ttl
        });
        self.enforce_max_entries();
    }

    fn enforce_max_entries(&mut self) {
        if self.caches.len() <= self.policy.max_entries {
            return;
        }

        let mut idle_keys = self
            .caches
            .iter()
            .filter(|(_key, slot)| Arc::strong_count(&slot.entry) == 1)
            .map(|(key, slot)| (key.clone(), slot.last_used))
            .collect::<Vec<_>>();
        idle_keys.sort_by_key(|(_key, last_used)| *last_used);

        let excess = self.caches.len().saturating_sub(self.policy.max_entries);
        for (key, _last_used) in idle_keys.into_iter().take(excess) {
            self.caches.remove(&key);
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct CachePolicy {
    idle_ttl: Duration,
    sweep_interval: Duration,
    max_entries: usize,
}

impl Default for CachePolicy {
    fn default() -> Self {
        Self {
            idle_ttl: DEFAULT_CACHE_IDLE_TTL,
            sweep_interval: DEFAULT_CACHE_SWEEP_INTERVAL,
            max_entries: DEFAULT_CACHE_MAX_ENTRIES,
        }
    }
}

#[derive(Debug)]
struct CacheSlot {
    entry: Arc<ResourceCacheEntry>,
    last_used: Instant,
}

#[derive(Debug)]
pub(crate) struct ResourceCacheEntry {
    store: Store<DynamicObject>,
    changes: broadcast::Sender<()>,
    _task: ReflectorTask,
    _debounce_task: ReflectorTask,
}

impl ResourceCacheEntry {
    fn start(client: kube::Client, key: ResourceCacheKey) -> miku_core::Result<Self> {
        let api_resource = api_resource(&key.resource_ref());
        let api = api_for_cache_key(client, &api_resource, &key);
        let (store, writer) = dynamic_store(api_resource);
        let config = watcher_config(&key);
        let (changes, _) = broadcast::channel(64);
        let change_sender = changes.clone();
        let (change_request_sender, mut change_request_receiver) = mpsc::channel(1);

        let debounce_task = tokio::spawn(async move {
            while change_request_receiver.recv().await.is_some() {
                tokio::time::sleep(RESOURCE_CHANGE_DEBOUNCE).await;
                while change_request_receiver.try_recv().is_ok() {}
                let _ = change_sender.send(());
            }
        });

        let stream = watcher(api, config).default_backoff();
        let reflected = reflector(writer, stream);
        let task = tokio::spawn(async move {
            futures::pin_mut!(reflected);
            while let Some(event) = reflected.next().await {
                match event {
                    Ok(event) => {
                        if watcher_event_updates_snapshot(&event) {
                            let _ = change_request_sender.try_send(());
                        }
                    }
                    Err(error) => {
                        tracing::warn!(%error, "resource cache reflector event failed");
                    }
                }
            }
        });

        Ok(Self {
            store,
            changes,
            _task: ReflectorTask(task),
            _debounce_task: ReflectorTask(debounce_task),
        })
    }

    pub(crate) async fn wait_until_ready(&self) -> miku_core::Result<()> {
        timeout(CACHE_READY_TIMEOUT, self.store.wait_until_ready())
            .await
            .map_err(|_| {
                miku_core::MikuError::Kubernetes(format!(
                    "resource cache did not become ready within {} seconds",
                    CACHE_READY_TIMEOUT.as_secs()
                ))
            })?
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

    pub(crate) fn subscribe(&self) -> broadcast::Receiver<()> {
        self.changes.subscribe()
    }

    fn is_finished(&self) -> bool {
        self._task.is_finished()
    }
}

fn watcher_event_updates_snapshot(event: &watcher::Event<DynamicObject>) -> bool {
    matches!(
        event,
        watcher::Event::Apply(_) | watcher::Event::Delete(_) | watcher::Event::InitDone
    )
}

fn watcher_config(key: &ResourceCacheKey) -> watcher::Config {
    let mut config = watcher::Config {
        page_size: None,
        ..watcher::Config::default()
    };
    if let Some(selector) = &key.label_selector {
        config = config.labels(selector);
    }
    config
}

#[derive(Debug)]
struct ReflectorTask(tokio::task::JoinHandle<()>);

impl ReflectorTask {
    fn is_finished(&self) -> bool {
        self.0.is_finished()
    }
}

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
    use tokio::sync::oneshot;

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
        let mut different_cluster = first.clone();
        different_cluster.cluster_id = miku_core::ClusterId::new("remote");

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
        assert_ne!(
            ResourceCacheKey::from_query(&first),
            ResourceCacheKey::from_query(&different_cluster)
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

    #[test]
    fn watcher_config_uses_single_initial_list_without_pagination() {
        let query = ResourceQuery::new(
            miku_core::ClusterId::new("local"),
            ResourceRef::core("v1", "pods"),
        )
        .label_selector("app=api");
        let key = ResourceCacheKey::from_query(&query);

        let config = watcher_config(&key);

        assert_eq!(config.page_size, None);
        assert_eq!(config.label_selector.as_deref(), Some("app=api"));
    }

    #[test]
    fn watcher_events_identify_snapshot_updates() {
        let api_resource = api_resource(&ResourceRef::core("v1", "pods"));
        let pod = pod_object(&api_resource, "default", "api");

        assert!(watcher_event_updates_snapshot(&watcher::Event::Apply(
            pod.clone()
        )));
        assert!(watcher_event_updates_snapshot(&watcher::Event::Delete(pod)));
        assert!(watcher_event_updates_snapshot(&watcher::Event::InitDone));
        assert!(!watcher_event_updates_snapshot(&watcher::Event::Init));
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
            changes: broadcast::channel(1).0,
            _task: ReflectorTask(tokio::spawn(async {})),
            _debounce_task: ReflectorTask(tokio::spawn(async {})),
        };

        let snapshot = entry.snapshot(Some(1));

        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].name_any(), "api");
    }

    #[tokio::test]
    async fn get_or_start_reuses_live_cache_and_updates_last_used() {
        let registry = ResourceCacheRegistry::with_policy(test_policy(10, 60, 128));
        let key = cache_key("local", "default");
        let old_last_used = Instant::now() - Duration::from_secs(30);
        let (entry, _abort_rx) = test_entry();
        registry
            .insert_test_entry(key.clone(), entry.clone(), old_last_used)
            .await;

        let reused = registry
            .get_or_start_with(key.clone(), || {
                panic!("live cache should be reused instead of restarted")
            })
            .await
            .unwrap();

        assert!(Arc::ptr_eq(&entry, &reused));
        assert!(registry.last_used_for_key(&key).await.unwrap() > old_last_used);
    }

    #[tokio::test]
    async fn sweep_idle_removes_unreferenced_entries_and_aborts_task() {
        let registry = ResourceCacheRegistry::with_policy(test_policy(5, 60, 128));
        let key = cache_key("local", "default");
        let (entry, abort_rx) = test_entry();
        registry
            .insert_test_entry(key.clone(), entry, Instant::now() - Duration::from_secs(10))
            .await;
        tokio::task::yield_now().await;

        registry.sweep_idle_for_tests(Instant::now()).await;

        assert!(!registry.contains_key(&key).await);
        abort_rx.await.unwrap();
    }

    #[tokio::test]
    async fn sweep_does_not_remove_entry_with_external_arc() {
        let registry = ResourceCacheRegistry::with_policy(test_policy(5, 60, 128));
        let key = cache_key("local", "default");
        let (entry, _abort_rx) = test_entry();
        let active = entry.clone();
        registry
            .insert_test_entry(key.clone(), entry, Instant::now() - Duration::from_secs(10))
            .await;

        registry.sweep_idle_for_tests(Instant::now()).await;

        assert!(registry.contains_key(&key).await);
        drop(active);
        registry.sweep_idle_for_tests(Instant::now()).await;
        assert!(!registry.contains_key(&key).await);
    }

    #[tokio::test]
    async fn finished_reflector_is_restarted_on_next_get_or_start() {
        let registry = ResourceCacheRegistry::with_policy(test_policy(10, 60, 128));
        let key = cache_key("local", "default");
        let finished = Arc::new(finished_entry().await);
        registry
            .insert_test_entry(key.clone(), finished.clone(), Instant::now())
            .await;

        let (replacement_entry, _abort_rx) = test_entry_value();
        let restarted = registry
            .get_or_start_with(key.clone(), || Ok(replacement_entry))
            .await
            .unwrap();

        assert!(!Arc::ptr_eq(&finished, &restarted));
        assert!(Arc::ptr_eq(
            &restarted,
            &registry.entry_for_key(&key).await.unwrap()
        ));
    }

    #[tokio::test]
    async fn invalidate_cluster_removes_only_matching_cluster_entries() {
        let registry = ResourceCacheRegistry::with_policy(test_policy(10, 60, 128));
        let local = cache_key("local", "default");
        let remote = cache_key("remote", "default");
        registry
            .insert_test_entry(local.clone(), test_entry().0, Instant::now())
            .await;
        registry
            .insert_test_entry(remote.clone(), test_entry().0, Instant::now())
            .await;

        registry.invalidate_cluster(&ClusterId::new("local")).await;

        assert!(!registry.contains_key(&local).await);
        assert!(registry.contains_key(&remote).await);
    }

    #[tokio::test]
    async fn max_entries_evicts_oldest_idle_entries_first() {
        let registry = ResourceCacheRegistry::with_policy(test_policy(60, 60, 2));
        let oldest = cache_key("local", "oldest");
        let middle = cache_key("local", "middle");
        let newest = cache_key("local", "newest");
        let now = Instant::now();
        registry
            .insert_test_entry(
                oldest.clone(),
                test_entry().0,
                now - Duration::from_secs(30),
            )
            .await;
        registry
            .insert_test_entry(
                middle.clone(),
                test_entry().0,
                now - Duration::from_secs(20),
            )
            .await;

        registry
            .get_or_start_with(newest.clone(), || Ok(test_entry_value().0))
            .await
            .unwrap();

        assert_eq!(registry.cache_count().await, 2);
        assert!(!registry.contains_key(&oldest).await);
        assert!(registry.contains_key(&middle).await);
        assert!(registry.contains_key(&newest).await);
    }

    #[tokio::test]
    async fn invalidate_all_removes_every_cache_entry() {
        let registry = ResourceCacheRegistry::with_policy(test_policy(10, 60, 128));
        registry
            .insert_test_entry(
                cache_key("local", "default"),
                test_entry().0,
                Instant::now(),
            )
            .await;
        registry
            .insert_test_entry(
                cache_key("remote", "default"),
                test_entry().0,
                Instant::now(),
            )
            .await;

        registry.invalidate_all().await;

        assert_eq!(registry.cache_count().await, 0);
    }

    fn test_policy(
        idle_ttl_secs: u64,
        sweep_interval_secs: u64,
        max_entries: usize,
    ) -> CachePolicy {
        CachePolicy {
            idle_ttl: Duration::from_secs(idle_ttl_secs),
            sweep_interval: Duration::from_secs(sweep_interval_secs),
            max_entries,
        }
    }

    fn cache_key(cluster_id: &str, namespace: &str) -> ResourceCacheKey {
        let mut query =
            ResourceQuery::new(ClusterId::new(cluster_id), ResourceRef::core("v1", "pods"));
        query.namespace = Some(namespace.to_owned());
        ResourceCacheKey::from_query(&query)
    }

    fn test_entry() -> (Arc<ResourceCacheEntry>, oneshot::Receiver<()>) {
        let (entry, abort_rx) = test_entry_value();
        (Arc::new(entry), abort_rx)
    }

    fn test_entry_value() -> (ResourceCacheEntry, oneshot::Receiver<()>) {
        let api_resource = api_resource(&ResourceRef::core("v1", "pods"));
        let (reader, _writer) = dynamic_store(api_resource);
        let (abort_tx, abort_rx) = oneshot::channel();
        let task = tokio::spawn(async move {
            let _signal = AbortSignal(Some(abort_tx));
            futures::future::pending::<()>().await;
        });
        (
            ResourceCacheEntry {
                store: reader,
                changes: broadcast::channel(1).0,
                _task: ReflectorTask(task),
                _debounce_task: ReflectorTask(tokio::spawn(async {})),
            },
            abort_rx,
        )
    }

    async fn finished_entry() -> ResourceCacheEntry {
        let api_resource = api_resource(&ResourceRef::core("v1", "pods"));
        let (reader, _writer) = dynamic_store(api_resource);
        let task = tokio::spawn(async {});
        while !task.is_finished() {
            tokio::task::yield_now().await;
        }
        ResourceCacheEntry {
            store: reader,
            changes: broadcast::channel(1).0,
            _task: ReflectorTask(task),
            _debounce_task: ReflectorTask(tokio::spawn(async {})),
        }
    }

    struct AbortSignal(Option<oneshot::Sender<()>>);

    impl Drop for AbortSignal {
        fn drop(&mut self) {
            if let Some(sender) = self.0.take() {
                let _ = sender.send(());
            }
        }
    }

    fn pod_object(api_resource: &ApiResource, namespace: &str, name: &str) -> DynamicObject {
        let mut object = DynamicObject::new(name, api_resource);
        object.metadata.namespace = Some(namespace.to_owned());
        object
    }
}
