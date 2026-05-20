use async_trait::async_trait;
use futures::channel::mpsc;
use futures::{AsyncBufReadExt, SinkExt, StreamExt, TryStreamExt};
use kube::api::{
    ApiResource, AttachParams, DeleteParams, EvictParams, LogParams, Patch, PatchParams,
    TerminalSize,
};
use kube::config::{KubeConfigOptions, Kubeconfig};
use kube::core::{DynamicObject, GroupVersionKind};
use kube::{Api, Config, ResourceExt};
use miku_api::{
    ClusterConfigStore, ClusterConnectionInfo, ClusterInitializeRequest, ClusterInitializer,
    ClusterRegistry, ClusterStatusCondition, ClusterStatusEventSummary, ClusterStatusOverview,
    ClusterStatusReader, ClusterStatusReport, ClusterStatusRequest, ClusterStatusSeverity,
    ClusterStatusWorkloadSummary, ClusterSummary, CreateClusterRequest, KubernetesResourceReader,
    KubernetesResourceWriter, KubernetesWatchService, LocalPreferenceStore, LogLine, MikuServices,
    PodAttachInput, PodAttachOutput, PodAttachRequest, PodAttachService, PodAttachSession,
    PodEvictRequest, PodLogQuery, PodLogService, ResourceApplyRequest, ResourceDeleteRequest,
    ResourceDetail, ResourceEvent, ResourceList, ResourceQuery, ResourceSummary,
};
use miku_core::{ClusterId, ResourceRef, ResourceScope};
use std::collections::HashMap;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::Mutex;

mod resource_cache;

use resource_cache::ResourceCacheRegistry;

#[derive(Clone)]
pub struct KubeServices<S> {
    store: S,
    default_client: Option<kube::Client>,
    clients: std::sync::Arc<Mutex<HashMap<ClusterId, kube::Client>>>,
    resource_cache: ResourceCacheRegistry,
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

    async fn invalidate_cluster_cache(&self, cluster_id: &ClusterId) {
        self.resource_cache.invalidate_cluster(cluster_id).await;
    }

    async fn client_for_cluster(&self, cluster_id: &ClusterId) -> miku_core::Result<kube::Client>
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

    async fn cached_snapshot(
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

fn kubeconfig_options_for_context(kubeconfig: &Kubeconfig, context: &str) -> KubeConfigOptions {
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

fn resolve_kubeconfig_context(
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

fn dynamic_api(
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
            ResourceQuery::new(cluster_id.clone(), ResourceRef::core("v1", "namespaces"));
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

#[async_trait]
impl<S> ClusterStatusReader for KubeServices<S>
where
    S: ClusterConfigStore + ClusterRegistry + LocalPreferenceStore + Clone + Send + Sync,
{
    #[tracing::instrument(name = "kube.get_cluster_status", skip(self), fields(cluster_id = %request.cluster_id))]
    async fn get_cluster_status(
        &self,
        request: ClusterStatusRequest,
    ) -> miku_core::Result<ClusterStatusReport> {
        let cluster_id = request.cluster_id;
        let client = self.client_for_cluster(&cluster_id).await?;

        let version_client = client.clone();
        let version = async move {
            version_client
                .apiserver_version()
                .await
                .map_err(|error| miku_core::MikuError::Kubernetes(error.to_string()))
        };
        let namespaces = self.cached_snapshot(
            client.clone(),
            ResourceQuery::new(cluster_id.clone(), ResourceRef::core("v1", "namespaces")),
        );
        let nodes = self.cached_snapshot(
            client.clone(),
            ResourceQuery::new(cluster_id.clone(), ResourceRef::core("v1", "nodes")),
        );
        let pods = self.cached_snapshot(
            client.clone(),
            ResourceQuery::new(cluster_id.clone(), ResourceRef::core("v1", "pods")),
        );
        let deployments = self.cached_snapshot(
            client.clone(),
            ResourceQuery::new(
                cluster_id.clone(),
                ResourceRef::grouped("apps", "v1", "deployments"),
            ),
        );
        let services = self.cached_snapshot(
            client.clone(),
            ResourceQuery::new(cluster_id.clone(), ResourceRef::core("v1", "services")),
        );
        let config_maps = self.cached_snapshot(
            client.clone(),
            ResourceQuery::new(cluster_id.clone(), ResourceRef::core("v1", "configmaps")),
        );
        let secrets = self.cached_snapshot(
            client.clone(),
            ResourceQuery::new(cluster_id.clone(), ResourceRef::core("v1", "secrets")),
        );
        let events = self.cached_snapshot(
            client,
            ResourceQuery::new(cluster_id, ResourceRef::core("v1", "events")),
        );

        let (version, namespaces, nodes, pods, deployments, services, config_maps, secrets, events) =
            tokio::try_join!(
                version,
                namespaces,
                nodes,
                pods,
                deployments,
                services,
                config_maps,
                secrets,
                events
            )?;

        Ok(build_cluster_status_report(
            version.git_version,
            (!version.platform.is_empty()).then_some(version.platform),
            ClusterStatusSnapshots {
                namespaces: &namespaces,
                nodes: &nodes,
                pods: &pods,
                deployments: &deployments,
                services: &services,
                config_maps: &config_maps,
                secrets: &secrets,
                events: &events,
            },
        ))
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

fn resource_list_from_snapshot(objects: Vec<DynamicObject>) -> ResourceList {
    ResourceList {
        items: objects.into_iter().map(resource_summary).collect(),
        continue_token: None,
    }
}

#[async_trait]
impl<S> PodLogService for KubeServices<S>
where
    S: ClusterConfigStore + ClusterRegistry + LocalPreferenceStore + Clone + Send + Sync,
{
    #[tracing::instrument(name = "kube.read_logs", skip(self), fields(namespace = %query.namespace, pod = %query.pod))]
    async fn read_logs(&self, query: PodLogQuery) -> miku_core::Result<Vec<LogLine>> {
        let client = self.client_for_cluster(&query.cluster_id).await?;
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

    #[tracing::instrument(name = "kube.stream_logs", skip(self), fields(namespace = %query.namespace, pod = %query.pod))]
    async fn stream_logs(
        &self,
        query: PodLogQuery,
    ) -> miku_core::Result<miku_api::BoxEventStream<LogLine>> {
        let client = self.client_for_cluster(&query.cluster_id).await?;
        let pods: Api<k8s_openapi::api::core::v1::Pod> = Api::namespaced(client, &query.namespace);
        let mut params = log_params(&query);
        params.follow = true;
        let lines = pods
            .log_stream(&query.pod, &params)
            .await
            .map_err(|error| miku_core::MikuError::Kubernetes(error.to_string()))?
            .lines();

        Ok(futures::stream::try_unfold(lines, |mut lines| async move {
            let line = lines
                .try_next()
                .await
                .map_err(|error| miku_core::MikuError::Kubernetes(error.to_string()))?;
            Ok(line.map(|text| (LogLine { text }, lines)))
        })
        .boxed())
    }
}

#[async_trait]
impl<S> PodAttachService for KubeServices<S>
where
    S: ClusterConfigStore + ClusterRegistry + LocalPreferenceStore + Clone + Send + Sync,
{
    #[tracing::instrument(name = "kube.attach_pod", skip(self), fields(namespace = %request.namespace, pod = %request.pod))]
    async fn attach_pod(&self, request: PodAttachRequest) -> miku_core::Result<PodAttachSession> {
        let client = self.client_for_cluster(&request.cluster_id).await?;
        let pods: Api<k8s_openapi::api::core::v1::Pod> =
            Api::namespaced(client, &request.namespace);
        let params = attach_params(&request);
        let mut attached = pods
            .attach(&request.pod, &params)
            .await
            .map_err(|error| miku_core::MikuError::Kubernetes(error.to_string()))?;

        let stdin = attached.stdin();
        let stdout = attached.stdout();
        let stderr = attached.stderr();
        let terminal_size = attached.terminal_size();
        let (input_tx, input_rx) = mpsc::unbounded();
        let (output_tx, output_rx) = mpsc::unbounded();

        tokio::spawn(run_attach_input(input_rx, stdin, terminal_size));
        if let Some(stdout) = stdout {
            tokio::spawn(read_attach_output(
                stdout,
                output_tx.clone(),
                PodAttachOutputKind::Stdout,
            ));
        }
        if let Some(stderr) = stderr {
            tokio::spawn(read_attach_output(
                stderr,
                output_tx.clone(),
                PodAttachOutputKind::Stderr,
            ));
        }
        tokio::spawn(async move {
            let result = attached
                .join()
                .await
                .map_err(|error| miku_core::MikuError::Kubernetes(error.to_string()));
            if let Err(error) = result {
                let _ = output_tx.unbounded_send(Err(error));
            }
            let _ = output_tx.unbounded_send(Ok(PodAttachOutput::Closed));
        });

        Ok(PodAttachSession {
            input: input_tx,
            output: output_rx.boxed(),
        })
    }
}

enum PodAttachOutputKind {
    Stdout,
    Stderr,
}

async fn run_attach_input<W>(
    mut input: mpsc::UnboundedReceiver<PodAttachInput>,
    mut stdin: Option<W>,
    mut terminal_size: Option<impl futures::Sink<TerminalSize> + Unpin>,
) where
    W: AsyncWrite + Unpin,
{
    while let Some(message) = input.next().await {
        match message {
            PodAttachInput::Bytes(bytes) => {
                if let Some(stdin) = stdin.as_mut()
                    && stdin.write_all(&bytes).await.is_err()
                {
                    break;
                }
            }
            PodAttachInput::Resize { cols, rows } => {
                if let Some(terminal_size) = terminal_size.as_mut() {
                    let _ = terminal_size
                        .send(TerminalSize {
                            width: cols,
                            height: rows,
                        })
                        .await;
                }
            }
            PodAttachInput::Close => break,
        }
    }
}

async fn read_attach_output<R>(
    mut reader: R,
    output: mpsc::UnboundedSender<miku_core::Result<PodAttachOutput>>,
    kind: PodAttachOutputKind,
) where
    R: AsyncRead + Unpin,
{
    let mut buffer = [0_u8; 4096];
    loop {
        match reader.read(&mut buffer).await {
            Ok(0) => break,
            Ok(count) => {
                let bytes = buffer[..count].to_vec();
                let message = match kind {
                    PodAttachOutputKind::Stdout => PodAttachOutput::Stdout(bytes),
                    PodAttachOutputKind::Stderr => PodAttachOutput::Stderr(bytes),
                };
                if output.unbounded_send(Ok(message)).is_err() {
                    break;
                }
            }
            Err(error) => {
                let _ =
                    output.unbounded_send(Err(miku_core::MikuError::Kubernetes(error.to_string())));
                break;
            }
        }
    }
}

fn attach_params(request: &PodAttachRequest) -> AttachParams {
    let mut params = AttachParams::interactive_tty()
        .tty(request.tty)
        .stderr(!request.tty);
    if let Some(container) = request.container.as_deref() {
        params = params.container(container);
    }
    params
}

fn log_params(query: &PodLogQuery) -> LogParams {
    LogParams {
        container: query.container.clone(),
        tail_lines: query.tail_lines.map(i64::from),
        ..LogParams::default()
    }
}

struct ClusterStatusSnapshots<'a> {
    namespaces: &'a [DynamicObject],
    nodes: &'a [DynamicObject],
    pods: &'a [DynamicObject],
    deployments: &'a [DynamicObject],
    services: &'a [DynamicObject],
    config_maps: &'a [DynamicObject],
    secrets: &'a [DynamicObject],
    events: &'a [DynamicObject],
}

fn build_cluster_status_report(
    version: String,
    platform: Option<String>,
    snapshots: ClusterStatusSnapshots<'_>,
) -> ClusterStatusReport {
    let ready_nodes = count_ready_nodes(snapshots.nodes);
    let unhealthy_pods = count_unhealthy_pods(snapshots.pods);
    let warning_events = snapshots
        .events
        .iter()
        .filter(|event| json_str(&event.data, "/type") == Some("Warning"))
        .count();

    ClusterStatusReport {
        overview: ClusterStatusOverview {
            version,
            platform,
            namespaces: snapshots.namespaces.len(),
            nodes: snapshots.nodes.len(),
            pods: snapshots.pods.len(),
            ready_nodes,
            unhealthy_pods,
        },
        conditions: cluster_status_conditions(
            snapshots.nodes.len(),
            ready_nodes,
            snapshots.pods.len(),
            unhealthy_pods,
            warning_events,
        ),
        workloads: ClusterStatusWorkloadSummary {
            pods: snapshots.pods.len(),
            deployments: snapshots.deployments.len(),
            services: snapshots.services.len(),
            config_maps: snapshots.config_maps.len(),
            secrets: snapshots.secrets.len(),
        },
        recent_events: recent_event_summaries(snapshots.events),
    }
}

fn count_ready_nodes(nodes: &[DynamicObject]) -> usize {
    nodes.iter().filter(|node| node_is_ready(node)).count()
}

fn node_is_ready(node: &DynamicObject) -> bool {
    node.data
        .pointer("/status/conditions")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|conditions| {
            conditions.iter().any(|condition| {
                json_str(condition, "/type") == Some("Ready")
                    && json_str(condition, "/status") == Some("True")
            })
        })
}

fn count_unhealthy_pods(pods: &[DynamicObject]) -> usize {
    pods.iter().filter(|pod| pod_is_unhealthy(pod)).count()
}

fn pod_is_unhealthy(pod: &DynamicObject) -> bool {
    let phase = json_str(&pod.data, "/status/phase").unwrap_or_default();
    if !matches!(phase, "Running" | "Succeeded") {
        return true;
    }

    pod.data
        .pointer("/status/containerStatuses")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|statuses| {
            statuses.iter().any(|status| {
                status
                    .pointer("/ready")
                    .and_then(serde_json::Value::as_bool)
                    == Some(false)
            })
        })
}

fn cluster_status_conditions(
    nodes: usize,
    ready_nodes: usize,
    pods: usize,
    unhealthy_pods: usize,
    warning_events: usize,
) -> Vec<ClusterStatusCondition> {
    vec![
        ClusterStatusCondition {
            name: "Nodes".to_owned(),
            status: format!("{ready_nodes}/{nodes} ready"),
            severity: if nodes == ready_nodes {
                ClusterStatusSeverity::Ok
            } else {
                ClusterStatusSeverity::Critical
            },
            message: if nodes == ready_nodes {
                "All nodes are ready".to_owned()
            } else {
                format!(
                    "{} node(s) are not ready",
                    nodes.saturating_sub(ready_nodes)
                )
            },
        },
        ClusterStatusCondition {
            name: "Pods".to_owned(),
            status: format!("{} unhealthy / {pods} total", unhealthy_pods),
            severity: if unhealthy_pods == 0 {
                ClusterStatusSeverity::Ok
            } else {
                ClusterStatusSeverity::Warning
            },
            message: if unhealthy_pods == 0 {
                "No unhealthy pods detected".to_owned()
            } else {
                format!("{unhealthy_pods} pod(s) need attention")
            },
        },
        ClusterStatusCondition {
            name: "Events".to_owned(),
            status: format!("{warning_events} warning"),
            severity: if warning_events == 0 {
                ClusterStatusSeverity::Ok
            } else {
                ClusterStatusSeverity::Warning
            },
            message: if warning_events == 0 {
                "No warning events in the recent event cache".to_owned()
            } else {
                format!("{warning_events} warning event(s) were found")
            },
        },
    ]
}

fn recent_event_summaries(events: &[DynamicObject]) -> Vec<ClusterStatusEventSummary> {
    let mut events = events.iter().collect::<Vec<_>>();
    events.sort_by_key(|event| std::cmp::Reverse(event_timestamp(event)));
    events.into_iter().take(10).map(event_summary).collect()
}

fn event_summary(event: &DynamicObject) -> ClusterStatusEventSummary {
    ClusterStatusEventSummary {
        namespace: event.namespace(),
        involved_object: involved_object_name(event),
        reason: json_str(&event.data, "/reason")
            .unwrap_or("Unknown")
            .to_owned(),
        message: json_str(&event.data, "/message").unwrap_or("").to_owned(),
        event_type: json_str(&event.data, "/type")
            .unwrap_or("Unknown")
            .to_owned(),
    }
}

fn involved_object_name(event: &DynamicObject) -> String {
    let kind = json_str(&event.data, "/involvedObject/kind")
        .or_else(|| json_str(&event.data, "/regarding/kind"))
        .unwrap_or("Object");
    let name = json_str(&event.data, "/involvedObject/name")
        .or_else(|| json_str(&event.data, "/regarding/name"))
        .or(event.metadata.name.as_deref())
        .unwrap_or("unknown");
    format!("{kind}/{name}")
}

fn event_timestamp(event: &DynamicObject) -> String {
    json_str(&event.data, "/lastTimestamp")
        .or_else(|| json_str(&event.data, "/eventTime"))
        .or_else(|| json_str(&event.data, "/firstTimestamp"))
        .map(ToOwned::to_owned)
        .or_else(|| {
            event
                .metadata
                .creation_timestamp
                .as_ref()
                .map(|timestamp| format!("{timestamp:?}"))
        })
        .unwrap_or_default()
}

fn json_str<'a>(value: &'a serde_json::Value, pointer: &str) -> Option<&'a str> {
    value.pointer(pointer).and_then(serde_json::Value::as_str)
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
    S: ClusterConfigStore + ClusterRegistry + LocalPreferenceStore + Clone + Send + Sync
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

    #[test]
    fn attach_params_use_tty_without_stderr_and_container() {
        let request = miku_api::PodAttachRequest {
            cluster_id: miku_core::ClusterId::new("local"),
            namespace: "default".to_owned(),
            pod: "api".to_owned(),
            container: Some("server".to_owned()),
            tty: true,
        };

        let params = attach_params(&request);

        assert!(params.stdin);
        assert!(params.stdout);
        assert!(params.tty);
        assert!(!params.stderr);
        assert_eq!(params.container.as_deref(), Some("server"));
    }

    #[test]
    fn attach_params_enable_stderr_when_tty_is_disabled() {
        let request = miku_api::PodAttachRequest {
            cluster_id: miku_core::ClusterId::new("local"),
            namespace: "default".to_owned(),
            pod: "api".to_owned(),
            container: None,
            tty: false,
        };

        let params = attach_params(&request);

        assert!(params.stdin);
        assert!(params.stdout);
        assert!(!params.tty);
        assert!(params.stderr);
        assert!(params.container.is_none());
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

    #[test]
    fn ready_nodes_are_counted_from_ready_condition() {
        let ready = dynamic_object(
            "ready",
            "nodes",
            serde_json::json!({
                "status": {
                    "conditions": [{"type": "Ready", "status": "True"}]
                }
            }),
        );
        let not_ready = dynamic_object(
            "not-ready",
            "nodes",
            serde_json::json!({
                "status": {
                    "conditions": [{"type": "Ready", "status": "False"}]
                }
            }),
        );

        assert_eq!(count_ready_nodes(&[ready, not_ready]), 1);
    }

    #[test]
    fn unhealthy_pods_include_non_running_and_unready_containers() {
        let running = dynamic_object(
            "running",
            "pods",
            serde_json::json!({
                "status": {
                    "phase": "Running",
                    "containerStatuses": [{"ready": true}]
                }
            }),
        );
        let pending = dynamic_object(
            "pending",
            "pods",
            serde_json::json!({
                "status": {"phase": "Pending"}
            }),
        );
        let unready = dynamic_object(
            "unready",
            "pods",
            serde_json::json!({
                "status": {
                    "phase": "Running",
                    "containerStatuses": [{"ready": false}]
                }
            }),
        );

        assert_eq!(count_unhealthy_pods(&[running, pending, unready]), 2);
    }

    #[test]
    fn status_report_summarizes_workloads_and_conditions() {
        let namespace = dynamic_object("default", "namespaces", serde_json::json!({}));
        let node = dynamic_object(
            "node",
            "nodes",
            serde_json::json!({
                "status": {"conditions": [{"type": "Ready", "status": "True"}]}
            }),
        );
        let pod = dynamic_object(
            "api",
            "pods",
            serde_json::json!({
                "status": {"phase": "Running", "containerStatuses": [{"ready": true}]}
            }),
        );
        let namespaces = vec![namespace];
        let nodes = vec![node];
        let pods = vec![pod];
        let deployments = vec![dynamic_object("api", "deployments", serde_json::json!({}))];
        let services = vec![dynamic_object("api", "services", serde_json::json!({}))];
        let config_maps = vec![dynamic_object("api", "configmaps", serde_json::json!({}))];
        let secrets = vec![dynamic_object("api", "secrets", serde_json::json!({}))];
        let events = Vec::<DynamicObject>::new();

        let report = build_cluster_status_report(
            "v1.35.0".to_owned(),
            Some("darwin/arm64".to_owned()),
            ClusterStatusSnapshots {
                namespaces: &namespaces,
                nodes: &nodes,
                pods: &pods,
                deployments: &deployments,
                services: &services,
                config_maps: &config_maps,
                secrets: &secrets,
                events: &events,
            },
        );

        assert_eq!(report.overview.version, "v1.35.0");
        assert_eq!(report.overview.ready_nodes, 1);
        assert_eq!(report.overview.unhealthy_pods, 0);
        assert_eq!(report.workloads.deployments, 1);
        assert_eq!(report.conditions[0].severity, ClusterStatusSeverity::Ok);
    }

    #[test]
    fn recent_events_are_sorted_and_summarized() {
        let older = event_object(
            "older",
            "default",
            "2026-01-01T00:00:00Z",
            "Normal",
            "Scheduled",
        );
        let newer = event_object(
            "newer",
            "default",
            "2026-01-02T00:00:00Z",
            "Warning",
            "BackOff",
        );

        let events = recent_event_summaries(&[older, newer]);

        assert_eq!(events[0].reason, "BackOff");
        assert_eq!(events[0].namespace.as_deref(), Some("default"));
        assert_eq!(events[0].involved_object, "Pod/api");
        assert_eq!(events[0].event_type, "Warning");
    }

    fn dynamic_object(name: &str, plural: &str, data: serde_json::Value) -> DynamicObject {
        let api_resource = api_resource(&miku_core::ResourceRef::core("v1", plural));
        let mut object = DynamicObject::new(name, &api_resource);
        object.data = data;
        object
    }

    fn event_object(
        name: &str,
        namespace: &str,
        last_timestamp: &str,
        event_type: &str,
        reason: &str,
    ) -> DynamicObject {
        let mut event = dynamic_object(
            name,
            "events",
            serde_json::json!({
                "lastTimestamp": last_timestamp,
                "type": event_type,
                "reason": reason,
                "message": "event message",
                "involvedObject": {
                    "kind": "Pod",
                    "name": "api"
                }
            }),
        );
        event.metadata.namespace = Some(namespace.to_owned());
        event
    }
}
