use std::pin::Pin;

use async_trait::async_trait;
use futures::Stream;
use futures::channel::mpsc;
use miku_core::{ClusterId, ResourceRef};
use serde::{Deserialize, Serialize};

#[cfg(not(target_arch = "wasm32"))]
pub type BoxEventStream<T> = Pin<Box<dyn Stream<Item = miku_core::Result<T>> + Send>>;

#[cfg(target_arch = "wasm32")]
pub type BoxEventStream<T> = Pin<Box<dyn Stream<Item = miku_core::Result<T>>>>;

#[cfg(not(target_arch = "wasm32"))]
pub trait ServiceBounds: Send + Sync {}

#[cfg(not(target_arch = "wasm32"))]
impl<T: Send + Sync> ServiceBounds for T {}

#[cfg(target_arch = "wasm32")]
pub trait ServiceBounds {}

#[cfg(target_arch = "wasm32")]
impl<T> ServiceBounds for T {}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ClusterSummary {
    pub id: ClusterId,
    pub name: String,
    pub context: String,
    pub current: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CreateClusterRequest {
    pub context: String,
    pub config: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LlmProviderSettings {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    #[serde(default = "default_llm_stream")]
    pub stream: bool,
}

impl Default for LlmProviderSettings {
    fn default() -> Self {
        Self {
            base_url: String::new(),
            api_key: String::new(),
            model: String::new(),
            stream: true,
        }
    }
}

fn default_llm_stream() -> bool {
    true
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ClusterInitializeRequest {
    pub cluster_id: ClusterId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ClusterConnectionInfo {
    pub version: String,
    pub platform: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ClusterStatusRequest {
    pub cluster_id: ClusterId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ClusterStatusOverview {
    pub version: String,
    pub platform: Option<String>,
    pub namespaces: usize,
    pub nodes: usize,
    pub pods: usize,
    pub ready_nodes: usize,
    pub unhealthy_pods: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ClusterStatusSeverity {
    Ok,
    Warning,
    Critical,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ClusterStatusCondition {
    pub name: String,
    pub status: String,
    pub severity: ClusterStatusSeverity,
    pub message: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ClusterStatusWorkloadSummary {
    pub pods: usize,
    pub deployments: usize,
    pub services: usize,
    pub config_maps: usize,
    pub secrets: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ClusterStatusEventSummary {
    pub namespace: Option<String>,
    pub involved_object: String,
    pub reason: String,
    pub message: String,
    pub event_type: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ClusterStatusReport {
    pub overview: ClusterStatusOverview,
    pub conditions: Vec<ClusterStatusCondition>,
    pub workloads: ClusterStatusWorkloadSummary,
    pub recent_events: Vec<ClusterStatusEventSummary>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResourceQuery {
    pub cluster_id: ClusterId,
    pub resource: ResourceRef,
    pub namespace: Option<String>,
    pub label_selector: Option<String>,
    pub limit: Option<u32>,
}

impl ResourceQuery {
    pub fn new(cluster_id: ClusterId, resource: ResourceRef) -> Self {
        Self {
            cluster_id,
            resource,
            namespace: None,
            label_selector: None,
            limit: Some(250),
        }
    }

    pub fn namespace(mut self, namespace: impl Into<String>) -> Self {
        self.namespace = Some(namespace.into());
        self
    }

    pub fn label_selector(mut self, selector: impl Into<String>) -> Self {
        self.label_selector = Some(selector.into());
        self
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ResourceList {
    pub items: Vec<ResourceSummary>,
    pub continue_token: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResourceSummary {
    pub name: String,
    pub namespace: Option<String>,
    pub kind: String,
    pub status: Option<String>,
    pub raw: serde_json::Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResourceDetail {
    pub summary: ResourceSummary,
    pub raw: serde_json::Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResourceApplyRequest {
    pub cluster_id: ClusterId,
    pub resource: ResourceRef,
    pub namespace: Option<String>,
    pub name: String,
    pub manifest: serde_json::Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResourcePatchRequest {
    pub cluster_id: ClusterId,
    pub resource: ResourceRef,
    pub namespace: Option<String>,
    pub name: String,
    pub patch: serde_json::Value,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResourceDeleteRequest {
    pub cluster_id: ClusterId,
    pub resource: ResourceRef,
    pub namespace: Option<String>,
    pub name: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PodEvictRequest {
    pub cluster_id: ClusterId,
    pub namespace: String,
    pub pod: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct NodeCordonRequest {
    pub cluster_id: ClusterId,
    pub node: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct NodeDrainRequest {
    pub cluster_id: ClusterId,
    pub node: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ResourceEvent {
    Snapshot(ResourceList),
    Applied(ResourceSummary),
    Deleted {
        name: String,
        namespace: Option<String>,
    },
    Restarted,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PodLogQuery {
    pub cluster_id: ClusterId,
    pub namespace: String,
    pub pod: String,
    pub container: Option<String>,
    pub tail_lines: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PodAttachRequest {
    pub cluster_id: ClusterId,
    pub namespace: String,
    pub pod: String,
    pub container: Option<String>,
    pub tty: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PodExecRequest {
    pub cluster_id: ClusterId,
    pub namespace: String,
    pub pod: String,
    pub container: Option<String>,
    #[serde(default = "default_pod_exec_command")]
    pub command: Vec<String>,
    pub tty: bool,
}

pub fn default_pod_exec_command() -> Vec<String> {
    vec!["/bin/sh".to_owned()]
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum PodAttachInput {
    Bytes(Vec<u8>),
    Resize { cols: u16, rows: u16 },
    Close,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum PodAttachOutput {
    Stdout(Vec<u8>),
    Stderr(Vec<u8>),
    Closed,
}

pub struct PodAttachSession {
    pub input: mpsc::UnboundedSender<PodAttachInput>,
    pub output: BoxEventStream<PodAttachOutput>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LogLine {
    pub text: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum AgentRole {
    User,
    Assistant,
    Tool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AgentMessage {
    pub role: AgentRole,
    pub content: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AgentContext {
    pub cluster_id: Option<ClusterId>,
    pub cluster_name: Option<String>,
    pub selected_resource: Option<String>,
    pub namespace: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AgentTurnRequest {
    pub session_id: String,
    pub message: String,
    pub context: AgentContext,
    pub history: Vec<AgentMessage>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum AgentTurnStatus {
    Completed,
    Partial,
    Blocked,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AgentToolCallSummary {
    pub name: String,
    pub arguments: serde_json::Value,
    pub result: Option<String>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum AgentEvent {
    ToolStarted {
        name: String,
        arguments: serde_json::Value,
    },
    ToolFinished {
        name: String,
        result: String,
    },
    ToolFailed {
        name: String,
        error: String,
    },
    Completed {
        status: AgentTurnStatus,
        summary: String,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AgentTurnResponse {
    pub session_id: String,
    pub message: AgentMessage,
    pub status: AgentTurnStatus,
    pub tool_calls: Vec<AgentToolCallSummary>,
    pub events: Vec<AgentEvent>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AgentConversationSummary {
    pub id: String,
    pub title: String,
    pub context: AgentContext,
    pub created_at: i64,
    pub updated_at: i64,
    pub last_message_at: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AgentConversation {
    pub summary: AgentConversationSummary,
    pub messages: Vec<AgentPersistedMessage>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AgentPersistedMessage {
    pub id: String,
    pub conversation_id: String,
    pub role: AgentRole,
    pub content: String,
    pub created_at: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CreateAgentConversationRequest {
    pub title: Option<String>,
    pub context: AgentContext,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AppendAgentMessageRequest {
    pub conversation_id: String,
    pub role: AgentRole,
    pub content: String,
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait ClusterRegistry: ServiceBounds {
    async fn list_clusters(&self) -> miku_core::Result<Vec<ClusterSummary>>;

    async fn create_cluster(
        &self,
        request: CreateClusterRequest,
    ) -> miku_core::Result<ClusterSummary>;
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait ClusterConfigStore: ServiceBounds {
    async fn get_cluster_config(&self, cluster_id: &ClusterId)
    -> miku_core::Result<Option<String>>;
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait ClusterInitializer: ServiceBounds {
    async fn initialize_cluster(
        &self,
        request: ClusterInitializeRequest,
    ) -> miku_core::Result<ClusterConnectionInfo>;
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait ClusterStatusReader: ServiceBounds {
    async fn get_cluster_status(
        &self,
        request: ClusterStatusRequest,
    ) -> miku_core::Result<ClusterStatusReport>;
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait KubernetesResourceReader: ServiceBounds {
    async fn list_resources(&self, query: ResourceQuery) -> miku_core::Result<ResourceList>;

    async fn get_resource(
        &self,
        _query: ResourceQuery,
        name: &str,
    ) -> miku_core::Result<ResourceDetail> {
        Err(miku_core::MikuError::UnsupportedRuntime(format!(
            "resource detail is not implemented for {name} in this service"
        )))
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait KubernetesResourceWriter: ServiceBounds {
    async fn apply_resource(
        &self,
        _request: ResourceApplyRequest,
    ) -> miku_core::Result<ResourceSummary> {
        Err(miku_core::MikuError::UnsupportedRuntime(
            "resource apply is not implemented in this service".to_owned(),
        ))
    }

    async fn patch_resource(
        &self,
        request: ResourcePatchRequest,
    ) -> miku_core::Result<ResourceSummary> {
        Err(miku_core::MikuError::UnsupportedRuntime(format!(
            "resource patch is not implemented for {}",
            request.name
        )))
    }

    async fn delete_resource(&self, request: ResourceDeleteRequest) -> miku_core::Result<()> {
        Err(miku_core::MikuError::UnsupportedRuntime(format!(
            "resource delete is not implemented for {}",
            request.name
        )))
    }

    async fn evict_pod(&self, request: PodEvictRequest) -> miku_core::Result<()> {
        Err(miku_core::MikuError::UnsupportedRuntime(format!(
            "pod eviction is not implemented for {}",
            request.pod
        )))
    }

    async fn cordon_node(&self, request: NodeCordonRequest) -> miku_core::Result<()> {
        Err(miku_core::MikuError::UnsupportedRuntime(format!(
            "node cordon is not implemented for {}",
            request.node
        )))
    }

    async fn drain_node(&self, request: NodeDrainRequest) -> miku_core::Result<()> {
        Err(miku_core::MikuError::UnsupportedRuntime(format!(
            "node drain is not implemented for {}",
            request.node
        )))
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait KubernetesWatchService: ServiceBounds {
    async fn watch_resources(
        &self,
        query: ResourceQuery,
    ) -> miku_core::Result<BoxEventStream<ResourceEvent>> {
        let _ = query;
        Err(miku_core::MikuError::UnsupportedRuntime(
            "resource watch is not implemented in this service".to_owned(),
        ))
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait PodLogService: ServiceBounds {
    async fn read_logs(&self, query: PodLogQuery) -> miku_core::Result<Vec<LogLine>> {
        let _ = query;
        Err(miku_core::MikuError::UnsupportedRuntime(
            "pod logs are not implemented in this service".to_owned(),
        ))
    }

    async fn stream_logs(&self, query: PodLogQuery) -> miku_core::Result<BoxEventStream<LogLine>> {
        let _ = query;
        Err(miku_core::MikuError::UnsupportedRuntime(
            "pod log streaming is not implemented in this service".to_owned(),
        ))
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait PodAttachService: ServiceBounds {
    async fn attach_pod(&self, request: PodAttachRequest) -> miku_core::Result<PodAttachSession> {
        Err(miku_core::MikuError::UnsupportedRuntime(format!(
            "pod attach is not implemented for {}",
            request.pod
        )))
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait PodExecService: ServiceBounds {
    async fn exec_pod(&self, request: PodExecRequest) -> miku_core::Result<PodAttachSession> {
        Err(miku_core::MikuError::UnsupportedRuntime(format!(
            "pod exec is not implemented for {}",
            request.pod
        )))
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait LocalPreferenceStore: ServiceBounds {
    async fn get_preference(&self, key: &str) -> miku_core::Result<Option<serde_json::Value>>;

    async fn set_preference(&self, key: &str, value: serde_json::Value) -> miku_core::Result<()>;
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait LlmSettingsStore: ServiceBounds {
    async fn get_llm_settings(&self) -> miku_core::Result<LlmProviderSettings>;

    async fn set_llm_settings(&self, settings: LlmProviderSettings) -> miku_core::Result<()>;
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait AgentService: ServiceBounds {
    async fn run_agent_turn(
        &self,
        request: AgentTurnRequest,
    ) -> miku_core::Result<AgentTurnResponse> {
        let _ = request;
        Err(miku_core::MikuError::UnsupportedRuntime(
            "agent service is not implemented in this runtime".to_owned(),
        ))
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait AgentConversationStore: ServiceBounds {
    async fn list_agent_conversations(&self) -> miku_core::Result<Vec<AgentConversationSummary>> {
        Err(miku_core::MikuError::UnsupportedRuntime(
            "agent conversation store is not implemented in this runtime".to_owned(),
        ))
    }

    async fn get_agent_conversation(
        &self,
        id: &str,
    ) -> miku_core::Result<Option<AgentConversation>> {
        let _ = id;
        Err(miku_core::MikuError::UnsupportedRuntime(
            "agent conversation store is not implemented in this runtime".to_owned(),
        ))
    }

    async fn create_agent_conversation(
        &self,
        request: CreateAgentConversationRequest,
    ) -> miku_core::Result<AgentConversationSummary> {
        let _ = request;
        Err(miku_core::MikuError::UnsupportedRuntime(
            "agent conversation store is not implemented in this runtime".to_owned(),
        ))
    }

    async fn append_agent_message(
        &self,
        request: AppendAgentMessageRequest,
    ) -> miku_core::Result<AgentPersistedMessage> {
        let _ = request;
        Err(miku_core::MikuError::UnsupportedRuntime(
            "agent conversation store is not implemented in this runtime".to_owned(),
        ))
    }

    async fn delete_agent_conversation(&self, id: &str) -> miku_core::Result<()> {
        let _ = id;
        Err(miku_core::MikuError::UnsupportedRuntime(
            "agent conversation store is not implemented in this runtime".to_owned(),
        ))
    }
}

pub trait MikuServices:
    ClusterRegistry
    + ClusterInitializer
    + ClusterStatusReader
    + KubernetesResourceReader
    + KubernetesResourceWriter
    + KubernetesWatchService
    + PodLogService
    + PodAttachService
    + PodExecService
    + LocalPreferenceStore
    + LlmSettingsStore
    + AgentService
    + AgentConversationStore
    + ServiceBounds
{
}

#[cfg(test)]
mod tests {
    use super::*;
    use miku_core::{ClusterId, ResourceRef};

    #[test]
    fn create_cluster_request_round_trips_as_json() {
        let request = CreateClusterRequest {
            context: "kind-miku".to_owned(),
            config: "apiVersion: v1".to_owned(),
        };

        let serialized = serde_json::to_string(&request).unwrap();
        let deserialized = serde_json::from_str::<CreateClusterRequest>(&serialized).unwrap();

        assert_eq!(deserialized, request);
    }

    #[test]
    fn cluster_initialize_request_round_trips_as_json() {
        let request = ClusterInitializeRequest {
            cluster_id: ClusterId::new("kind-miku"),
        };

        let serialized = serde_json::to_string(&request).unwrap();
        let deserialized = serde_json::from_str::<ClusterInitializeRequest>(&serialized).unwrap();

        assert_eq!(deserialized, request);
    }

    #[test]
    fn cluster_connection_info_round_trips_as_json() {
        let info = ClusterConnectionInfo {
            version: "v1.35.0".to_owned(),
            platform: Some("darwin/arm64".to_owned()),
        };

        let serialized = serde_json::to_string(&info).unwrap();
        let deserialized = serde_json::from_str::<ClusterConnectionInfo>(&serialized).unwrap();

        assert_eq!(deserialized, info);
    }

    #[test]
    fn cluster_status_report_round_trips_as_json() {
        let report = ClusterStatusReport {
            overview: ClusterStatusOverview {
                version: "v1.35.0".to_owned(),
                platform: Some("darwin/arm64".to_owned()),
                namespaces: 4,
                nodes: 3,
                pods: 12,
                ready_nodes: 2,
                unhealthy_pods: 1,
            },
            conditions: vec![ClusterStatusCondition {
                name: "Nodes".to_owned(),
                status: "2/3 ready".to_owned(),
                severity: ClusterStatusSeverity::Warning,
                message: "1 node is not ready".to_owned(),
            }],
            workloads: ClusterStatusWorkloadSummary {
                pods: 12,
                deployments: 3,
                services: 5,
                config_maps: 7,
                secrets: 2,
            },
            recent_events: vec![ClusterStatusEventSummary {
                namespace: Some("default".to_owned()),
                involved_object: "Pod/api".to_owned(),
                reason: "Started".to_owned(),
                message: "Started container api".to_owned(),
                event_type: "Normal".to_owned(),
            }],
        };

        let serialized = serde_json::to_string(&report).unwrap();
        let deserialized = serde_json::from_str::<ClusterStatusReport>(&serialized).unwrap();

        assert_eq!(deserialized, report);
    }

    #[test]
    fn pod_attach_contract_round_trips_as_json() {
        let request = PodAttachRequest {
            cluster_id: ClusterId::new("local"),
            namespace: "default".to_owned(),
            pod: "api".to_owned(),
            container: Some("server".to_owned()),
            tty: true,
        };
        let input = PodAttachInput::Resize {
            cols: 120,
            rows: 32,
        };
        let output = PodAttachOutput::Stdout(b"ready\n".to_vec());

        let request_json = serde_json::to_string(&request).unwrap();
        let input_json = serde_json::to_string(&input).unwrap();
        let output_json = serde_json::to_string(&output).unwrap();

        assert_eq!(
            serde_json::from_str::<PodAttachRequest>(&request_json).unwrap(),
            request
        );
        assert_eq!(
            serde_json::from_str::<PodAttachInput>(&input_json).unwrap(),
            input
        );
        assert_eq!(
            serde_json::from_str::<PodAttachOutput>(&output_json).unwrap(),
            output
        );
    }

    #[test]
    fn pod_exec_contract_round_trips_as_json() {
        let request = PodExecRequest {
            cluster_id: ClusterId::new("local"),
            namespace: "default".to_owned(),
            pod: "api".to_owned(),
            container: Some("server".to_owned()),
            command: default_pod_exec_command(),
            tty: true,
        };

        let request_json = serde_json::to_string(&request).unwrap();

        assert_eq!(
            serde_json::from_str::<PodExecRequest>(&request_json).unwrap(),
            request
        );
    }

    #[test]
    fn pod_exec_contract_defaults_to_shell_command() {
        let request = serde_json::from_value::<PodExecRequest>(serde_json::json!({
            "cluster_id": "local",
            "namespace": "default",
            "pod": "api",
            "container": "server",
            "tty": true
        }))
        .unwrap();

        assert_eq!(request.command, default_pod_exec_command());
    }

    #[test]
    fn resource_query_defaults_to_no_namespace_or_selector() {
        let query = ResourceQuery::new(ClusterId::new("local"), ResourceRef::core("v1", "pods"));

        assert_eq!(query.cluster_id.as_str(), "local");
        assert!(query.namespace.is_none());
        assert!(query.label_selector.is_none());
        assert_eq!(query.limit, Some(250));
    }

    #[test]
    fn resource_patch_request_round_trips_as_json() {
        let request = ResourcePatchRequest {
            cluster_id: ClusterId::new("local"),
            resource: ResourceRef::grouped("apps", "v1", "deployments"),
            namespace: Some("default".to_owned()),
            name: "api".to_owned(),
            patch: serde_json::json!({"spec": {"replicas": 3}}),
        };

        let serialized = serde_json::to_string(&request).unwrap();
        let deserialized = serde_json::from_str::<ResourcePatchRequest>(&serialized).unwrap();

        assert_eq!(deserialized, request);
    }

    #[test]
    fn service_bundle_can_be_type_checked() {
        fn accepts_services<T: MikuServices + ?Sized>(_services: &T) {}

        struct Dummy;

        #[async_trait::async_trait]
        impl ClusterRegistry for Dummy {
            async fn list_clusters(&self) -> miku_core::Result<Vec<ClusterSummary>> {
                Ok(Vec::new())
            }

            async fn create_cluster(
                &self,
                request: CreateClusterRequest,
            ) -> miku_core::Result<ClusterSummary> {
                Ok(ClusterSummary {
                    id: ClusterId::new(request.context.clone()),
                    name: request.context.clone(),
                    context: request.context,
                    current: false,
                })
            }
        }

        #[async_trait::async_trait]
        impl KubernetesResourceReader for Dummy {
            async fn list_resources(
                &self,
                _query: ResourceQuery,
            ) -> miku_core::Result<ResourceList> {
                Ok(ResourceList::default())
            }
        }

        #[async_trait::async_trait]
        impl ClusterInitializer for Dummy {
            async fn initialize_cluster(
                &self,
                request: ClusterInitializeRequest,
            ) -> miku_core::Result<ClusterConnectionInfo> {
                Ok(ClusterConnectionInfo {
                    version: format!("{}-ready", request.cluster_id.as_str()),
                    platform: None,
                })
            }
        }

        #[async_trait::async_trait]
        impl ClusterStatusReader for Dummy {
            async fn get_cluster_status(
                &self,
                request: ClusterStatusRequest,
            ) -> miku_core::Result<ClusterStatusReport> {
                Ok(ClusterStatusReport {
                    overview: ClusterStatusOverview {
                        version: format!("{}-ready", request.cluster_id.as_str()),
                        platform: None,
                        namespaces: 0,
                        nodes: 0,
                        pods: 0,
                        ready_nodes: 0,
                        unhealthy_pods: 0,
                    },
                    conditions: Vec::new(),
                    workloads: ClusterStatusWorkloadSummary {
                        pods: 0,
                        deployments: 0,
                        services: 0,
                        config_maps: 0,
                        secrets: 0,
                    },
                    recent_events: Vec::new(),
                })
            }
        }

        #[async_trait::async_trait]
        impl KubernetesResourceWriter for Dummy {}

        #[async_trait::async_trait]
        impl KubernetesWatchService for Dummy {}

        #[async_trait::async_trait]
        impl PodLogService for Dummy {}

        #[async_trait::async_trait]
        impl PodAttachService for Dummy {}

        #[async_trait::async_trait]
        impl PodExecService for Dummy {}

        #[async_trait::async_trait]
        impl LocalPreferenceStore for Dummy {
            async fn get_preference(
                &self,
                _key: &str,
            ) -> miku_core::Result<Option<serde_json::Value>> {
                Ok(None)
            }

            async fn set_preference(
                &self,
                _key: &str,
                _value: serde_json::Value,
            ) -> miku_core::Result<()> {
                Ok(())
            }
        }

        #[async_trait::async_trait]
        impl LlmSettingsStore for Dummy {
            async fn get_llm_settings(&self) -> miku_core::Result<LlmProviderSettings> {
                Ok(LlmProviderSettings::default())
            }

            async fn set_llm_settings(
                &self,
                _settings: LlmProviderSettings,
            ) -> miku_core::Result<()> {
                Ok(())
            }
        }

        #[async_trait::async_trait]
        impl AgentService for Dummy {}

        #[async_trait::async_trait]
        impl AgentConversationStore for Dummy {}

        impl MikuServices for Dummy {}

        accepts_services(&Dummy);
    }

    #[test]
    fn agent_turn_contract_round_trips_as_json() {
        let request = AgentTurnRequest {
            session_id: "agent-1".to_owned(),
            message: "Summarize this cluster".to_owned(),
            context: AgentContext {
                cluster_id: Some(ClusterId::new("local")),
                cluster_name: Some("kind-miku".to_owned()),
                selected_resource: Some("Pods".to_owned()),
                namespace: Some("default".to_owned()),
            },
            history: vec![AgentMessage {
                role: AgentRole::User,
                content: "What is unhealthy?".to_owned(),
            }],
        };
        let response = AgentTurnResponse {
            session_id: request.session_id.clone(),
            message: AgentMessage {
                role: AgentRole::Assistant,
                content: "No unhealthy pods found.".to_owned(),
            },
            status: AgentTurnStatus::Completed,
            tool_calls: vec![AgentToolCallSummary {
                name: "get_cluster_status".to_owned(),
                arguments: serde_json::json!({"cluster_id": "local"}),
                result: Some("ok".to_owned()),
                error: None,
            }],
            events: vec![AgentEvent::Completed {
                status: AgentTurnStatus::Completed,
                summary: "Checked cluster status".to_owned(),
            }],
        };

        let request_json = serde_json::to_string(&request).unwrap();
        let response_json = serde_json::to_string(&response).unwrap();

        assert_eq!(
            serde_json::from_str::<AgentTurnRequest>(&request_json).unwrap(),
            request
        );
        assert_eq!(
            serde_json::from_str::<AgentTurnResponse>(&response_json).unwrap(),
            response
        );
    }

    #[test]
    fn agent_conversation_contract_round_trips_as_json() {
        let context = AgentContext {
            cluster_id: Some(ClusterId::new("local")),
            cluster_name: Some("kind-miku".to_owned()),
            selected_resource: Some("Pods".to_owned()),
            namespace: Some("default".to_owned()),
        };
        let summary = AgentConversationSummary {
            id: "conversation-1".to_owned(),
            title: "Inspect pods".to_owned(),
            context: context.clone(),
            created_at: 10,
            updated_at: 12,
            last_message_at: Some(12),
        };
        let message = AgentPersistedMessage {
            id: "message-1".to_owned(),
            conversation_id: summary.id.clone(),
            role: AgentRole::User,
            content: "What is unhealthy?".to_owned(),
            created_at: 11,
        };
        let conversation = AgentConversation {
            summary: summary.clone(),
            messages: vec![message.clone()],
        };
        let create_request = CreateAgentConversationRequest {
            title: Some(summary.title.clone()),
            context,
        };
        let append_request = AppendAgentMessageRequest {
            conversation_id: summary.id.clone(),
            role: AgentRole::Assistant,
            content: "No unhealthy pods found.".to_owned(),
        };

        let summary_json = serde_json::to_string(&summary).unwrap();
        let conversation_json = serde_json::to_string(&conversation).unwrap();
        let message_json = serde_json::to_string(&message).unwrap();
        let create_json = serde_json::to_string(&create_request).unwrap();
        let append_json = serde_json::to_string(&append_request).unwrap();

        assert_eq!(
            serde_json::from_str::<AgentConversationSummary>(&summary_json).unwrap(),
            summary
        );
        assert_eq!(
            serde_json::from_str::<AgentConversation>(&conversation_json).unwrap(),
            conversation
        );
        assert_eq!(
            serde_json::from_str::<AgentPersistedMessage>(&message_json).unwrap(),
            message
        );
        assert_eq!(
            serde_json::from_str::<CreateAgentConversationRequest>(&create_json).unwrap(),
            create_request
        );
        assert_eq!(
            serde_json::from_str::<AppendAgentMessageRequest>(&append_json).unwrap(),
            append_request
        );
    }
}
