use async_trait::async_trait;
#[cfg(not(target_arch = "wasm32"))]
use futures::SinkExt;
use futures::StreamExt;
use futures::channel::mpsc;
#[cfg(target_arch = "wasm32")]
use futures::channel::oneshot;
use miku_api::{
    AgentConversation, AgentConversationStore, AgentConversationSummary, AgentPersistedMessage,
    AgentService, AgentTurnRequest, AgentTurnResponse, AppendAgentMessageRequest,
    ClusterConnectionInfo, ClusterInitializeRequest, ClusterInitializer, ClusterRegistry,
    ClusterStatusReader, ClusterStatusReport, ClusterStatusRequest, ClusterSummary,
    CreateAgentConversationRequest, CreateClusterRequest, KubernetesResourceReader,
    KubernetesResourceWriter, KubernetesWatchService, LlmProviderSettings, LlmSettingsStore,
    LocalPreferenceStore, LogLine, MikuServices, NodeCordonRequest, NodeDrainRequest,
    PodAttachInput, PodAttachOutput, PodAttachRequest, PodAttachService, PodAttachSession,
    PodEvictRequest, PodLogQuery, PodLogService, ResourceApplyRequest, ResourceDeleteRequest,
    ResourceEvent, ResourceList, ResourcePatchRequest, ResourceQuery, ResourceSummary,
};
#[cfg(target_arch = "wasm32")]
use std::cell::RefCell;
#[cfg(target_arch = "wasm32")]
use std::rc::Rc;
use url::Url;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::{JsCast, closure::Closure};

#[derive(Clone, Debug)]
pub struct HttpMikuClient {
    base_url: Url,
    client: reqwest::Client,
}

impl HttpMikuClient {
    #[tracing::instrument(name = "http_client.new")]
    pub fn new(base_url: &str) -> miku_core::Result<Self> {
        let base_url = Url::parse(base_url)
            .map_err(|error| miku_core::MikuError::Config(error.to_string()))?;
        tracing::debug!(%base_url, "created HTTP Miku client");
        Ok(Self {
            base_url,
            client: reqwest::Client::new(),
        })
    }

    pub fn endpoint(&self, path: &str) -> Url {
        self.base_url
            .join(path.trim_start_matches('/'))
            .expect("validated base URL should join relative API paths")
    }

    pub fn resource_watch_endpoint(&self, query: &ResourceQuery) -> Url {
        let mut endpoint = self.endpoint("/api/resources/watch");
        {
            let mut pairs = endpoint.query_pairs_mut();
            pairs.append_pair("cluster_id", query.cluster_id.as_str());
            if let Some(group) = &query.resource.group {
                pairs.append_pair("group", group);
            }
            pairs.append_pair("version", &query.resource.version);
            pairs.append_pair("plural", &query.resource.plural);
            if let Some(namespace) = &query.namespace {
                pairs.append_pair("namespace", namespace);
            }
            if let Some(label_selector) = &query.label_selector {
                pairs.append_pair("label_selector", label_selector);
            }
            if let Some(limit) = query.limit {
                pairs.append_pair("limit", &limit.to_string());
            }
        }
        endpoint
    }

    pub fn pod_attach_endpoint(&self, request: &PodAttachRequest) -> miku_core::Result<Url> {
        let mut endpoint = self.endpoint("/api/pods/attach");
        endpoint
            .set_scheme(match self.base_url.scheme() {
                "https" | "wss" => "wss",
                _ => "ws",
            })
            .map_err(|_| {
                miku_core::MikuError::Config(format!(
                    "could not build websocket URL from {}",
                    self.base_url
                ))
            })?;
        {
            let mut pairs = endpoint.query_pairs_mut();
            pairs.append_pair("cluster_id", request.cluster_id.as_str());
            pairs.append_pair("namespace", &request.namespace);
            pairs.append_pair("pod", &request.pod);
            if let Some(container) = &request.container {
                pairs.append_pair("container", container);
            }
            pairs.append_pair("tty", if request.tty { "true" } else { "false" });
        }
        Ok(endpoint)
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl ClusterRegistry for HttpMikuClient {
    #[tracing::instrument(name = "http_client.list_clusters", skip(self))]
    async fn list_clusters(&self) -> miku_core::Result<Vec<ClusterSummary>> {
        let endpoint = self.endpoint("/api/clusters");
        tracing::debug!(url = %endpoint, "requesting clusters");
        self.client
            .get(endpoint)
            .send()
            .await
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))?
            .error_for_status()
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))?
            .json()
            .await
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))
    }

    #[tracing::instrument(name = "http_client.create_cluster", skip(self, request), fields(context = %request.context))]
    async fn create_cluster(
        &self,
        request: CreateClusterRequest,
    ) -> miku_core::Result<ClusterSummary> {
        let endpoint = self.endpoint("/api/clusters");
        self.client
            .post(endpoint)
            .json(&request)
            .send()
            .await
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))?
            .error_for_status()
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))?
            .json()
            .await
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl ClusterInitializer for HttpMikuClient {
    #[tracing::instrument(name = "http_client.initialize_cluster", skip(self), fields(cluster_id = %request.cluster_id))]
    async fn initialize_cluster(
        &self,
        request: ClusterInitializeRequest,
    ) -> miku_core::Result<ClusterConnectionInfo> {
        let endpoint = self.endpoint("/api/clusters/initialize");
        self.client
            .post(endpoint)
            .json(&request)
            .send()
            .await
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))?
            .error_for_status()
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))?
            .json()
            .await
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl ClusterStatusReader for HttpMikuClient {
    #[tracing::instrument(name = "http_client.get_cluster_status", skip(self), fields(cluster_id = %request.cluster_id))]
    async fn get_cluster_status(
        &self,
        request: ClusterStatusRequest,
    ) -> miku_core::Result<ClusterStatusReport> {
        let endpoint = self.endpoint("/api/clusters/status");
        self.client
            .post(endpoint)
            .json(&request)
            .send()
            .await
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))?
            .error_for_status()
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))?
            .json()
            .await
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl AgentService for HttpMikuClient {
    #[tracing::instrument(name = "http_client.run_agent_turn", skip(self, request), fields(session_id = %request.session_id))]
    async fn run_agent_turn(
        &self,
        request: AgentTurnRequest,
    ) -> miku_core::Result<AgentTurnResponse> {
        let endpoint = self.endpoint("/api/agent/turn");
        self.client
            .post(endpoint)
            .json(&request)
            .send()
            .await
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))?
            .error_for_status()
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))?
            .json()
            .await
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl AgentConversationStore for HttpMikuClient {
    #[tracing::instrument(name = "http_client.list_agent_conversations", skip(self))]
    async fn list_agent_conversations(&self) -> miku_core::Result<Vec<AgentConversationSummary>> {
        let endpoint = self.endpoint("/api/agent/conversations");
        self.client
            .get(endpoint)
            .send()
            .await
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))?
            .error_for_status()
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))?
            .json()
            .await
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))
    }

    #[tracing::instrument(name = "http_client.get_agent_conversation", skip(self), fields(conversation_id = %id))]
    async fn get_agent_conversation(
        &self,
        id: &str,
    ) -> miku_core::Result<Option<AgentConversation>> {
        let endpoint = self.endpoint(&format!("/api/agent/conversations/{id}"));
        let response = self
            .client
            .get(endpoint)
            .send()
            .await
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        response
            .error_for_status()
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))?
            .json()
            .await
            .map(Some)
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))
    }

    #[tracing::instrument(name = "http_client.create_agent_conversation", skip(self, request))]
    async fn create_agent_conversation(
        &self,
        request: CreateAgentConversationRequest,
    ) -> miku_core::Result<AgentConversationSummary> {
        let endpoint = self.endpoint("/api/agent/conversations");
        self.client
            .post(endpoint)
            .json(&request)
            .send()
            .await
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))?
            .error_for_status()
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))?
            .json()
            .await
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))
    }

    #[tracing::instrument(name = "http_client.append_agent_message", skip(self, request), fields(conversation_id = %request.conversation_id))]
    async fn append_agent_message(
        &self,
        request: AppendAgentMessageRequest,
    ) -> miku_core::Result<AgentPersistedMessage> {
        let endpoint = self.endpoint(&format!(
            "/api/agent/conversations/{}/messages",
            request.conversation_id
        ));
        self.client
            .post(endpoint)
            .json(&request)
            .send()
            .await
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))?
            .error_for_status()
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))?
            .json()
            .await
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))
    }

    #[tracing::instrument(name = "http_client.delete_agent_conversation", skip(self), fields(conversation_id = %id))]
    async fn delete_agent_conversation(&self, id: &str) -> miku_core::Result<()> {
        let endpoint = self.endpoint(&format!("/api/agent/conversations/{id}"));
        self.client
            .delete(endpoint)
            .send()
            .await
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))?
            .error_for_status()
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))?;
        Ok(())
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl KubernetesResourceReader for HttpMikuClient {
    #[tracing::instrument(name = "http_client.list_resources", skip(self, query), fields(resource = %query.resource.plural))]
    async fn list_resources(&self, query: ResourceQuery) -> miku_core::Result<ResourceList> {
        let endpoint = self.endpoint("/api/resources/list");
        self.client
            .post(endpoint)
            .json(&query)
            .send()
            .await
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))?
            .error_for_status()
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))?
            .json()
            .await
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl KubernetesResourceWriter for HttpMikuClient {
    #[tracing::instrument(name = "http_client.apply_resource", skip(self, request), fields(resource = %request.resource.plural, name = %request.name))]
    async fn apply_resource(
        &self,
        request: ResourceApplyRequest,
    ) -> miku_core::Result<ResourceSummary> {
        let endpoint = self.endpoint("/api/resources/apply");
        self.client
            .post(endpoint)
            .json(&request)
            .send()
            .await
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))?
            .error_for_status()
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))?
            .json()
            .await
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))
    }

    #[tracing::instrument(name = "http_client.delete_resource", skip(self, request), fields(resource = %request.resource.plural, name = %request.name))]
    async fn delete_resource(&self, request: ResourceDeleteRequest) -> miku_core::Result<()> {
        let endpoint = self.endpoint("/api/resources/delete");
        self.client
            .post(endpoint)
            .json(&request)
            .send()
            .await
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))?
            .error_for_status()
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))?;

        Ok(())
    }

    #[tracing::instrument(name = "http_client.patch_resource", skip(self, request), fields(resource = %request.resource.plural, name = %request.name))]
    async fn patch_resource(
        &self,
        request: ResourcePatchRequest,
    ) -> miku_core::Result<ResourceSummary> {
        let endpoint = self.endpoint("/api/resources/patch");
        self.client
            .post(endpoint)
            .json(&request)
            .send()
            .await
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))?
            .error_for_status()
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))?
            .json()
            .await
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))
    }

    #[tracing::instrument(name = "http_client.evict_pod", skip(self, request), fields(namespace = %request.namespace, pod = %request.pod))]
    async fn evict_pod(&self, request: PodEvictRequest) -> miku_core::Result<()> {
        let endpoint = self.endpoint("/api/pods/evict");
        self.client
            .post(endpoint)
            .json(&request)
            .send()
            .await
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))?
            .error_for_status()
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))?;

        Ok(())
    }

    #[tracing::instrument(name = "http_client.cordon_node", skip(self, request), fields(node = %request.node))]
    async fn cordon_node(&self, request: NodeCordonRequest) -> miku_core::Result<()> {
        let endpoint = self.endpoint("/api/nodes/cordon");
        self.client
            .post(endpoint)
            .json(&request)
            .send()
            .await
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))?
            .error_for_status()
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))?;

        Ok(())
    }

    #[tracing::instrument(name = "http_client.drain_node", skip(self, request), fields(node = %request.node))]
    async fn drain_node(&self, request: NodeDrainRequest) -> miku_core::Result<()> {
        let endpoint = self.endpoint("/api/nodes/drain");
        self.client
            .post(endpoint)
            .json(&request)
            .send()
            .await
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))?
            .error_for_status()
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))?;

        Ok(())
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl KubernetesWatchService for HttpMikuClient {
    #[tracing::instrument(name = "http_client.watch_resources", skip(self, query), fields(resource = %query.resource.plural))]
    async fn watch_resources(
        &self,
        query: ResourceQuery,
    ) -> miku_core::Result<miku_api::BoxEventStream<ResourceEvent>> {
        watch_resources(self, query).await
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl PodLogService for HttpMikuClient {
    #[tracing::instrument(name = "http_client.read_logs", skip(self, query), fields(namespace = %query.namespace, pod = %query.pod))]
    async fn read_logs(&self, query: PodLogQuery) -> miku_core::Result<Vec<LogLine>> {
        let endpoint = self.endpoint("/api/pods/logs");
        self.client
            .post(endpoint)
            .json(&query)
            .send()
            .await
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))?
            .error_for_status()
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))?
            .json()
            .await
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl PodAttachService for HttpMikuClient {
    async fn attach_pod(&self, request: PodAttachRequest) -> miku_core::Result<PodAttachSession> {
        attach_pod(self, request).await
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl LocalPreferenceStore for HttpMikuClient {
    async fn get_preference(&self, _key: &str) -> miku_core::Result<Option<serde_json::Value>> {
        Err(miku_core::MikuError::UnsupportedRuntime(
            "preferences are local to the native process".to_owned(),
        ))
    }

    async fn set_preference(&self, _key: &str, _value: serde_json::Value) -> miku_core::Result<()> {
        Err(miku_core::MikuError::UnsupportedRuntime(
            "preferences are local to the native process".to_owned(),
        ))
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl LlmSettingsStore for HttpMikuClient {
    #[tracing::instrument(name = "http_client.get_llm_settings", skip(self))]
    async fn get_llm_settings(&self) -> miku_core::Result<LlmProviderSettings> {
        let endpoint = self.endpoint("/api/settings/llm");
        self.client
            .get(endpoint)
            .send()
            .await
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))?
            .error_for_status()
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))?
            .json()
            .await
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))
    }

    #[tracing::instrument(name = "http_client.set_llm_settings", skip(self, settings))]
    async fn set_llm_settings(&self, settings: LlmProviderSettings) -> miku_core::Result<()> {
        let endpoint = self.endpoint("/api/settings/llm");
        self.client
            .put(endpoint)
            .json(&settings)
            .send()
            .await
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))?
            .error_for_status()
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))?;

        Ok(())
    }
}

impl MikuServices for HttpMikuClient {}

#[cfg(not(target_arch = "wasm32"))]
async fn attach_pod(
    client: &HttpMikuClient,
    request: PodAttachRequest,
) -> miku_core::Result<PodAttachSession> {
    let endpoint = client.pod_attach_endpoint(&request)?;
    let (socket, _) = tokio_tungstenite::connect_async(endpoint.as_str())
        .await
        .map_err(|error| miku_core::MikuError::Transport(error.to_string()))?;
    let (mut socket_tx, mut socket_rx) = socket.split();
    let (input_tx, mut input_rx) = mpsc::unbounded();
    let (output_tx, output_rx) = mpsc::unbounded();

    tokio::spawn(async move {
        while let Some(input) = input_rx.next().await {
            let close = matches!(input, PodAttachInput::Close);
            let message = match input {
                PodAttachInput::Bytes(bytes) => {
                    tokio_tungstenite::tungstenite::Message::Binary(bytes.into())
                }
                PodAttachInput::Resize { .. } | PodAttachInput::Close => {
                    let Ok(text) = serde_json::to_string(&input) else {
                        break;
                    };
                    tokio_tungstenite::tungstenite::Message::Text(text.into())
                }
            };
            if socket_tx.send(message).await.is_err() {
                break;
            }
            if close {
                break;
            }
        }
    });

    tokio::spawn(async move {
        while let Some(message) = socket_rx.next().await {
            let output = match message {
                Ok(tokio_tungstenite::tungstenite::Message::Binary(bytes)) => {
                    Ok(PodAttachOutput::Stdout(bytes.to_vec()))
                }
                Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => {
                    serde_json::from_str::<PodAttachOutput>(&text)
                        .map_err(|error| miku_core::MikuError::Transport(error.to_string()))
                }
                Ok(tokio_tungstenite::tungstenite::Message::Close(_)) => {
                    Ok(PodAttachOutput::Closed)
                }
                Ok(_) => continue,
                Err(error) => Err(miku_core::MikuError::Transport(error.to_string())),
            };
            let close = matches!(output, Ok(PodAttachOutput::Closed));
            if output_tx.unbounded_send(output).is_err() || close {
                break;
            }
        }
        let _ = output_tx.unbounded_send(Ok(PodAttachOutput::Closed));
    });

    Ok(PodAttachSession {
        input: input_tx,
        output: output_rx.boxed(),
    })
}

#[cfg(target_arch = "wasm32")]
async fn attach_pod(
    client: &HttpMikuClient,
    request: PodAttachRequest,
) -> miku_core::Result<PodAttachSession> {
    let endpoint = client.pod_attach_endpoint(&request)?;
    let socket = web_sys::WebSocket::new(endpoint.as_str()).map_err(|error| {
        miku_core::MikuError::Transport(format!(
            "failed to open pod attach websocket: {}",
            js_value_message(error)
        ))
    })?;
    socket.set_binary_type(web_sys::BinaryType::Arraybuffer);

    let (input_tx, mut input_rx) = mpsc::unbounded();
    let (output_tx, output_rx) = mpsc::unbounded();
    let (open_tx, open_rx) = oneshot::channel::<miku_core::Result<()>>();
    let open_sender = Rc::new(RefCell::new(Some(open_tx)));

    let open_sender_for_open = Rc::clone(&open_sender);
    let onopen = Closure::<dyn FnMut(web_sys::Event)>::new(move |_event| {
        if let Some(sender) = open_sender_for_open.borrow_mut().take() {
            let _ = sender.send(Ok(()));
        }
    });
    socket.set_onopen(Some(onopen.as_ref().unchecked_ref()));

    let open_sender_for_error = Rc::clone(&open_sender);
    let output_sender_for_error = output_tx.clone();
    let onerror = Closure::<dyn FnMut(web_sys::Event)>::new(move |_event| {
        let error = miku_core::MikuError::Transport("pod attach websocket failed".to_owned());
        if let Some(sender) = open_sender_for_error.borrow_mut().take() {
            let _ = sender.send(Err(error));
        } else {
            let _ = output_sender_for_error.unbounded_send(Err(error));
        }
    });
    socket.set_onerror(Some(onerror.as_ref().unchecked_ref()));

    let open_sender_for_close = Rc::clone(&open_sender);
    let output_sender_for_close = output_tx.clone();
    let onclose = Closure::<dyn FnMut(web_sys::Event)>::new(move |_event| {
        if let Some(sender) = open_sender_for_close.borrow_mut().take() {
            let _ = sender.send(Err(miku_core::MikuError::Transport(
                "pod attach websocket closed before it opened".to_owned(),
            )));
        } else {
            let _ = output_sender_for_close.unbounded_send(Ok(PodAttachOutput::Closed));
        }
    });
    socket.set_onclose(Some(onclose.as_ref().unchecked_ref()));

    let output_sender_for_message = output_tx.clone();
    let onmessage =
        Closure::<dyn FnMut(web_sys::MessageEvent)>::new(move |event: web_sys::MessageEvent| {
            let output = pod_attach_output_from_message(event.data());
            let _ = output_sender_for_message.unbounded_send(output);
        });
    socket.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));

    match open_rx.await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            let _ = socket.close();
            return Err(error);
        }
        Err(_) => {
            let _ = socket.close();
            return Err(miku_core::MikuError::Transport(
                "pod attach websocket open callback was dropped".to_owned(),
            ));
        }
    }
    socket.set_onopen(None);

    let socket_for_input = socket.clone();
    let output_sender_for_input = output_tx.clone();
    wasm_bindgen_futures::spawn_local(async move {
        while let Some(input) = input_rx.next().await {
            let close = matches!(input, PodAttachInput::Close);
            if let Err(error) = send_pod_attach_input(&socket_for_input, input) {
                let _ = output_sender_for_input.unbounded_send(Err(error));
                break;
            }
            if close {
                let _ = socket_for_input.close();
                break;
            }
        }
    });

    Ok(PodAttachSession {
        input: input_tx,
        output: Box::pin(WebSocketAttachStream {
            receiver: output_rx,
            socket,
            _onmessage: onmessage,
            _onerror: onerror,
            _onclose: onclose,
        }),
    })
}

#[cfg(target_arch = "wasm32")]
fn send_pod_attach_input(
    socket: &web_sys::WebSocket,
    input: PodAttachInput,
) -> miku_core::Result<()> {
    match input {
        PodAttachInput::Bytes(bytes) => socket.send_with_u8_array(&bytes).map_err(|error| {
            miku_core::MikuError::Transport(format!(
                "failed to send pod attach stdin: {}",
                js_value_message(error)
            ))
        }),
        PodAttachInput::Resize { .. } | PodAttachInput::Close => {
            let text = serde_json::to_string(&input)
                .map_err(|error| miku_core::MikuError::Transport(error.to_string()))?;
            socket.send_with_str(&text).map_err(|error| {
                miku_core::MikuError::Transport(format!(
                    "failed to send pod attach control message: {}",
                    js_value_message(error)
                ))
            })
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn pod_attach_output_from_message(
    data: wasm_bindgen::JsValue,
) -> miku_core::Result<PodAttachOutput> {
    if let Some(buffer) = data.dyn_ref::<js_sys::ArrayBuffer>() {
        return Ok(PodAttachOutput::Stdout(
            js_sys::Uint8Array::new(buffer).to_vec(),
        ));
    }

    if let Some(text) = data.as_string() {
        return serde_json::from_str::<PodAttachOutput>(&text)
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()));
    }

    Err(miku_core::MikuError::Transport(
        "pod attach websocket message had unsupported payload type".to_owned(),
    ))
}

#[cfg(target_arch = "wasm32")]
fn js_value_message(value: wasm_bindgen::JsValue) -> String {
    if let Some(text) = value.as_string() {
        return text;
    }
    if let Some(error) = value.dyn_ref::<js_sys::Error>() {
        return String::from(error.message());
    }
    format!("{value:?}")
}

#[cfg(target_arch = "wasm32")]
struct WebSocketAttachStream {
    receiver: mpsc::UnboundedReceiver<miku_core::Result<PodAttachOutput>>,
    socket: web_sys::WebSocket,
    _onmessage: Closure<dyn FnMut(web_sys::MessageEvent)>,
    _onerror: Closure<dyn FnMut(web_sys::Event)>,
    _onclose: Closure<dyn FnMut(web_sys::Event)>,
}

#[cfg(target_arch = "wasm32")]
impl futures::Stream for WebSocketAttachStream {
    type Item = miku_core::Result<PodAttachOutput>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        context: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.receiver.poll_next_unpin(context)
    }
}

#[cfg(target_arch = "wasm32")]
impl Drop for WebSocketAttachStream {
    fn drop(&mut self) {
        let _ = self.socket.close();
    }
}

#[cfg(not(target_arch = "wasm32"))]
async fn watch_resources(
    client: &HttpMikuClient,
    query: ResourceQuery,
) -> miku_core::Result<miku_api::BoxEventStream<ResourceEvent>> {
    let endpoint = client.resource_watch_endpoint(&query);
    let bytes = client
        .client
        .get(endpoint)
        .send()
        .await
        .map_err(|error| miku_core::MikuError::Transport(error.to_string()))?
        .error_for_status()
        .map_err(|error| miku_core::MikuError::Transport(error.to_string()))?
        .bytes_stream();

    Ok(futures::stream::unfold(
        (
            bytes,
            String::new(),
            Vec::<miku_core::Result<ResourceEvent>>::new(),
        ),
        |(mut bytes, mut buffer, mut pending)| async move {
            loop {
                if let Some(event) = pending.pop() {
                    return Some((event, (bytes, buffer, pending)));
                }

                match bytes.next().await {
                    Some(Ok(chunk)) => {
                        buffer.push_str(&String::from_utf8_lossy(&chunk));
                        pending = parse_sse_events(&mut buffer);
                        pending.reverse();
                    }
                    Some(Err(error)) => {
                        return Some((
                            Err(miku_core::MikuError::Transport(error.to_string())),
                            (bytes, buffer, pending),
                        ));
                    }
                    None => return None,
                }
            }
        },
    )
    .boxed())
}

#[cfg(target_arch = "wasm32")]
async fn watch_resources(
    client: &HttpMikuClient,
    query: ResourceQuery,
) -> miku_core::Result<miku_api::BoxEventStream<ResourceEvent>> {
    let endpoint = client.resource_watch_endpoint(&query);
    let event_source = web_sys::EventSource::new(endpoint.as_str())
        .map_err(|error| miku_core::MikuError::Transport(format!("{error:?}")))?;
    let (sender, receiver) =
        futures::channel::mpsc::unbounded::<miku_core::Result<ResourceEvent>>();

    let message_sender = sender.clone();
    let onmessage =
        Closure::<dyn FnMut(web_sys::MessageEvent)>::new(move |event: web_sys::MessageEvent| {
            let Some(data) = event.data().as_string() else {
                let _ = message_sender.unbounded_send(Err(miku_core::MikuError::Transport(
                    "resource watch event did not contain text data".to_owned(),
                )));
                return;
            };
            let result = serde_json::from_str::<ResourceEvent>(&data)
                .map_err(|error| miku_core::MikuError::Transport(error.to_string()));
            let _ = message_sender.unbounded_send(result);
        });
    event_source.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));

    let error_sender = sender;
    let onerror = Closure::<dyn FnMut(web_sys::Event)>::new(move |_event: web_sys::Event| {
        let _ = error_sender.unbounded_send(Err(miku_core::MikuError::Transport(
            "resource watch stream failed".to_owned(),
        )));
    });
    event_source.set_onerror(Some(onerror.as_ref().unchecked_ref()));

    Ok(Box::pin(EventSourceStream {
        receiver,
        _event_source: event_source,
        _onmessage: onmessage,
        _onerror: onerror,
    }))
}

#[cfg(target_arch = "wasm32")]
struct EventSourceStream {
    receiver: futures::channel::mpsc::UnboundedReceiver<miku_core::Result<ResourceEvent>>,
    _event_source: web_sys::EventSource,
    _onmessage: Closure<dyn FnMut(web_sys::MessageEvent)>,
    _onerror: Closure<dyn FnMut(web_sys::Event)>,
}

#[cfg(target_arch = "wasm32")]
impl futures::Stream for EventSourceStream {
    type Item = miku_core::Result<ResourceEvent>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        context: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.receiver.poll_next_unpin(context)
    }
}

#[cfg(target_arch = "wasm32")]
impl Drop for EventSourceStream {
    fn drop(&mut self) {
        self._event_source.close();
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn parse_sse_events(buffer: &mut String) -> Vec<miku_core::Result<ResourceEvent>> {
    let mut events = Vec::new();
    while let Some(index) = buffer.find("\n\n") {
        let frame = buffer[..index].to_owned();
        buffer.drain(..index + 2);
        let mut event_name = None;
        let mut data = Vec::new();
        for line in frame.lines() {
            if let Some(value) = line.strip_prefix("event:") {
                event_name = Some(value.trim().to_owned());
            } else if let Some(value) = line.strip_prefix("data:") {
                data.push(value.trim_start().to_owned());
            }
        }
        if event_name.as_deref() == Some("error") {
            events.push(Err(miku_core::MikuError::Transport(data.join("\n"))));
        } else if !data.is_empty() {
            events.push(
                serde_json::from_str::<ResourceEvent>(&data.join("\n"))
                    .map_err(|error| miku_core::MikuError::Transport(error.to_string())),
            );
        }
    }
    events
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_endpoint_urls_from_base_url() {
        let client = HttpMikuClient::new("http://127.0.0.1:5174").unwrap();

        assert_eq!(
            client.endpoint("/api/clusters").as_str(),
            "http://127.0.0.1:5174/api/clusters"
        );
    }

    #[test]
    fn create_cluster_uses_cluster_collection_endpoint() {
        let client = HttpMikuClient::new("http://127.0.0.1:5174").unwrap();

        assert_eq!(
            client.endpoint("/api/clusters").as_str(),
            "http://127.0.0.1:5174/api/clusters"
        );
    }

    #[test]
    fn list_resources_uses_resource_list_endpoint() {
        let client = HttpMikuClient::new("http://127.0.0.1:5174").unwrap();

        assert_eq!(
            client.endpoint("/api/resources/list").as_str(),
            "http://127.0.0.1:5174/api/resources/list"
        );
    }

    #[test]
    fn watch_resources_encodes_resource_query() {
        let client = HttpMikuClient::new("http://127.0.0.1:5174").unwrap();
        let mut query = ResourceQuery::new(
            miku_core::ClusterId::new("local"),
            miku_core::ResourceRef::grouped("apps", "v1", "deployments"),
        )
        .namespace("production")
        .label_selector("app=api");
        query.limit = Some(50);

        let endpoint = client.resource_watch_endpoint(&query);

        assert_eq!(
            endpoint.as_str(),
            "http://127.0.0.1:5174/api/resources/watch?cluster_id=local&group=apps&version=v1&plural=deployments&namespace=production&label_selector=app%3Dapi&limit=50"
        );
    }

    #[test]
    fn pod_attach_endpoint_uses_websocket_scheme_and_query() {
        let client = HttpMikuClient::new("http://127.0.0.1:5174").unwrap();
        let request = PodAttachRequest {
            cluster_id: miku_core::ClusterId::new("local"),
            namespace: "default".to_owned(),
            pod: "api-7f9c".to_owned(),
            container: Some("server".to_owned()),
            tty: true,
        };

        assert_eq!(
            client.pod_attach_endpoint(&request).unwrap().as_str(),
            "ws://127.0.0.1:5174/api/pods/attach?cluster_id=local&namespace=default&pod=api-7f9c&container=server&tty=true"
        );

        let secure_client = HttpMikuClient::new("https://miku.example").unwrap();
        assert_eq!(
            secure_client
                .pod_attach_endpoint(&request)
                .unwrap()
                .scheme(),
            "wss"
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn parses_sse_resource_events() {
        let mut buffer =
            "data: {\"Snapshot\":{\"items\":[],\"continue_token\":null}}\n\n".to_owned();

        let events = parse_sse_events(&mut buffer);

        assert!(buffer.is_empty());
        assert!(matches!(
            events.as_slice(),
            [Ok(ResourceEvent::Snapshot(ResourceList { items, .. }))] if items.is_empty()
        ));
    }

    #[test]
    fn initialize_cluster_uses_cluster_initialize_endpoint() {
        let client = HttpMikuClient::new("http://127.0.0.1:5174").unwrap();

        assert_eq!(
            client.endpoint("/api/clusters/initialize").as_str(),
            "http://127.0.0.1:5174/api/clusters/initialize"
        );
    }

    #[test]
    fn cluster_status_uses_cluster_status_endpoint() {
        let client = HttpMikuClient::new("http://127.0.0.1:5174").unwrap();

        assert_eq!(
            client.endpoint("/api/clusters/status").as_str(),
            "http://127.0.0.1:5174/api/clusters/status"
        );
    }

    #[test]
    fn apply_resource_uses_resource_apply_endpoint() {
        let client = HttpMikuClient::new("http://127.0.0.1:5174").unwrap();

        assert_eq!(
            client.endpoint("/api/resources/apply").as_str(),
            "http://127.0.0.1:5174/api/resources/apply"
        );
    }

    #[test]
    fn delete_resource_uses_resource_delete_endpoint() {
        let client = HttpMikuClient::new("http://127.0.0.1:5174").unwrap();

        assert_eq!(
            client.endpoint("/api/resources/delete").as_str(),
            "http://127.0.0.1:5174/api/resources/delete"
        );
    }

    #[test]
    fn patch_resource_uses_resource_patch_endpoint() {
        let client = HttpMikuClient::new("http://127.0.0.1:5174").unwrap();

        assert_eq!(
            client.endpoint("/api/resources/patch").as_str(),
            "http://127.0.0.1:5174/api/resources/patch"
        );
    }

    #[test]
    fn pod_logs_use_pod_logs_endpoint() {
        let client = HttpMikuClient::new("http://127.0.0.1:5174").unwrap();

        assert_eq!(
            client.endpoint("/api/pods/logs").as_str(),
            "http://127.0.0.1:5174/api/pods/logs"
        );
    }

    #[test]
    fn pod_evict_uses_pod_evict_endpoint() {
        let client = HttpMikuClient::new("http://127.0.0.1:5174").unwrap();

        assert_eq!(
            client.endpoint("/api/pods/evict").as_str(),
            "http://127.0.0.1:5174/api/pods/evict"
        );
    }
}
