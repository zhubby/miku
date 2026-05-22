use std::sync::Arc;
use std::time::Duration;

use miku_api::{
    AgentEvent, AgentMessage, AgentRole, AgentToolCallSummary, AgentTurnRequest, AgentTurnResponse,
    AgentTurnStatus,
};
use serde_json::{Value, json};
use tokio::time::timeout;

use crate::AgentToolServices;
use crate::provider::{ChatMessage, LlmProvider, ProviderChatRequest};
use crate::tools::{ToolExecutionRecord, ToolRegistry};

#[derive(Clone, Debug)]
pub struct RunLimits {
    pub max_tool_iterations: u32,
    pub max_tool_calls: u32,
    pub provider_timeout: Duration,
    pub tool_timeout: Duration,
}

impl Default for RunLimits {
    fn default() -> Self {
        Self {
            max_tool_iterations: 8,
            max_tool_calls: 24,
            provider_timeout: Duration::from_secs(60),
            tool_timeout: Duration::from_secs(20),
        }
    }
}

#[derive(Clone)]
pub struct AgentRuntime {
    provider: Arc<dyn LlmProvider>,
    tools: ToolRegistry,
    limits: RunLimits,
}

impl AgentRuntime {
    pub fn new(provider: Arc<dyn LlmProvider>) -> Self {
        Self {
            provider,
            tools: ToolRegistry::miku_readonly(),
            limits: RunLimits::default(),
        }
    }

    pub fn with_limits(mut self, limits: RunLimits) -> Self {
        self.limits = limits;
        self
    }

    #[tracing::instrument(name = "agent.run_turn", skip_all, fields(session_id = %request.session_id))]
    pub async fn run_turn<S>(
        &self,
        request: AgentTurnRequest,
        services: Arc<S>,
    ) -> miku_core::Result<AgentTurnResponse>
    where
        S: AgentToolServices + 'static,
    {
        let session_id = request.session_id.clone();
        let mut messages = self.build_messages(&request);
        let mut events = Vec::new();
        let mut tool_calls = Vec::new();
        let mut tool_call_count = 0_u32;
        let services: Arc<dyn AgentToolServices> = services;

        for _ in 0..self.limits.max_tool_iterations {
            let response = timeout(
                self.limits.provider_timeout,
                self.provider.chat(ProviderChatRequest {
                    messages: messages.clone(),
                    tools: self.tools.definitions(),
                }),
            )
            .await
            .map_err(|_| {
                miku_core::MikuError::Transport("agent provider timed out".to_owned())
            })??;

            let assistant_message = response.message.clone();
            messages.push(assistant_message.clone());

            if assistant_message.tool_calls.is_empty() {
                let content = assistant_message.content.unwrap_or_default();
                return Ok(AgentTurnResponse {
                    session_id,
                    message: AgentMessage {
                        role: AgentRole::Assistant,
                        content,
                    },
                    status: AgentTurnStatus::Completed,
                    tool_calls,
                    events,
                });
            }

            for tool_call in assistant_message.tool_calls {
                tool_call_count += 1;
                if tool_call_count > self.limits.max_tool_calls {
                    return Ok(limit_response(
                        session_id,
                        "Agent stopped because it reached the tool call limit.",
                        tool_calls,
                        events,
                    ));
                }

                let arguments =
                    parse_tool_arguments(&tool_call.function.arguments).unwrap_or_else(|error| {
                        json!({
                            "parse_error": error,
                            "raw_arguments": tool_call.function.arguments,
                        })
                    });
                let name = tool_call.function.name;
                events.push(AgentEvent::ToolStarted {
                    name: name.clone(),
                    arguments: arguments.clone(),
                });

                let record = self
                    .execute_tool(name.clone(), arguments.clone(), services.clone())
                    .await;
                events.push(record.event.clone());
                tool_calls.push(record.summary.clone());

                match record.output {
                    Some(output) => {
                        messages.push(ChatMessage::tool(tool_call.id, output.content.clone()));
                        if let Some(completion) = output.completion {
                            events.push(AgentEvent::Completed {
                                status: completion.status.clone(),
                                summary: completion.summary.clone(),
                            });
                            return Ok(AgentTurnResponse {
                                session_id,
                                message: AgentMessage {
                                    role: AgentRole::Assistant,
                                    content: completion.summary,
                                },
                                status: completion.status,
                                tool_calls,
                                events,
                            });
                        }
                    }
                    None => {
                        let error = record
                            .summary
                            .error
                            .unwrap_or_else(|| "tool failed".to_owned());
                        messages.push(ChatMessage::tool(tool_call.id, error));
                    }
                }
            }
        }

        Ok(limit_response(
            session_id,
            "Agent stopped because it reached the tool iteration limit.",
            tool_calls,
            events,
        ))
    }

    fn build_messages(&self, request: &AgentTurnRequest) -> Vec<ChatMessage> {
        let mut messages = vec![ChatMessage::system(system_prompt(request))];
        messages.extend(request.history.iter().filter_map(history_message));
        messages.push(ChatMessage::user(request.message.clone()));
        messages
    }

    async fn execute_tool(
        &self,
        name: String,
        arguments: Value,
        services: Arc<dyn AgentToolServices>,
    ) -> ToolExecutionRecord {
        let Some(tool) = self.tools.get(&name) else {
            return ToolExecutionRecord::failure(
                name,
                arguments,
                "tool is not registered".to_owned(),
            );
        };

        match timeout(
            self.limits.tool_timeout,
            tool.execute(arguments.clone(), services),
        )
        .await
        {
            Ok(Ok(output)) => {
                let result = output.content.clone();
                ToolExecutionRecord::success(name, arguments, result, output)
            }
            Ok(Err(error)) => ToolExecutionRecord::failure(name, arguments, error.to_string()),
            Err(_) => ToolExecutionRecord::failure(name, arguments, "tool timed out".to_owned()),
        }
    }
}

fn history_message(message: &AgentMessage) -> Option<ChatMessage> {
    match message.role {
        AgentRole::User => Some(ChatMessage::user(message.content.clone())),
        AgentRole::Assistant => Some(ChatMessage::assistant(message.content.clone())),
        AgentRole::Tool => None,
    }
}

fn system_prompt(request: &AgentTurnRequest) -> String {
    let context = &request.context;
    let cluster = context
        .cluster_name
        .as_deref()
        .or_else(|| {
            context
                .cluster_id
                .as_ref()
                .map(miku_core::ClusterId::as_str)
        })
        .unwrap_or("none selected");
    let selected_resource = context.selected_resource.as_deref().unwrap_or("none");
    let namespace = context.namespace.as_deref().unwrap_or("not specified");

    format!(
        r#"You are Miku's Kubernetes operations agent.

Help the user understand and operate their Kubernetes clusters from inside Miku.
Use tools to inspect real cluster state before making factual claims.

Current UI context:
- Selected cluster: {cluster}
- Selected resource: {selected_resource}
- Namespace: {namespace}

Available behavior:
- Prefer read-only Kubernetes inspection tools.
- Do not claim you changed cluster state; mutating tools are not available yet.
- When you have completed the user's request, call complete_task with a concise summary.
- If you are blocked, call complete_task with status "blocked" and explain what is missing.
"#
    )
}

fn parse_tool_arguments(arguments: &str) -> Result<Value, String> {
    if arguments.trim().is_empty() {
        return Ok(Value::Object(Default::default()));
    }
    serde_json::from_str(arguments).map_err(|error| error.to_string())
}

fn limit_response(
    session_id: String,
    content: &str,
    tool_calls: Vec<AgentToolCallSummary>,
    mut events: Vec<AgentEvent>,
) -> AgentTurnResponse {
    events.push(AgentEvent::Completed {
        status: AgentTurnStatus::Partial,
        summary: content.to_owned(),
    });
    AgentTurnResponse {
        session_id,
        message: AgentMessage {
            role: AgentRole::Assistant,
            content: content.to_owned(),
        },
        status: AgentTurnStatus::Partial,
        tool_calls,
        events,
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use miku_api::{
        AgentContext, ClusterConnectionInfo, ClusterInitializeRequest, ClusterInitializer,
        ClusterStatusOverview, ClusterStatusReader, ClusterStatusReport, ClusterStatusRequest,
        ClusterStatusWorkloadSummary, ClusterSummary, CreateClusterRequest,
        KubernetesResourceWriter, KubernetesWatchService, LlmProviderSettings, LlmSettingsStore,
        LocalPreferenceStore, LogLine, PodAttachService, PodEvictRequest, PodLogQuery,
        PodLogService, ResourceList, ResourceQuery,
    };
    use miku_core::ClusterId;
    use tokio::sync::Mutex;

    use super::*;
    use crate::provider::{
        ChatMessage, ProviderChatResponse, ToolCall, ToolCallFunction, ToolDefinition,
    };

    struct MockProvider {
        responses: Mutex<Vec<ChatMessage>>,
    }

    #[async_trait]
    impl LlmProvider for MockProvider {
        async fn chat(
            &self,
            _request: ProviderChatRequest,
        ) -> miku_core::Result<ProviderChatResponse> {
            Ok(ProviderChatResponse {
                message: self.responses.lock().await.remove(0),
            })
        }
    }

    #[derive(Clone)]
    struct DummyServices;

    #[async_trait]
    impl miku_api::ClusterRegistry for DummyServices {
        async fn list_clusters(&self) -> miku_core::Result<Vec<ClusterSummary>> {
            Ok(vec![ClusterSummary {
                id: ClusterId::new("local"),
                name: "local".to_owned(),
                context: "local".to_owned(),
                current: true,
            }])
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

    #[async_trait]
    impl ClusterStatusReader for DummyServices {
        async fn get_cluster_status(
            &self,
            _request: ClusterStatusRequest,
        ) -> miku_core::Result<ClusterStatusReport> {
            Ok(ClusterStatusReport {
                overview: ClusterStatusOverview {
                    version: "v1.35.0".to_owned(),
                    platform: None,
                    namespaces: 1,
                    nodes: 1,
                    pods: 2,
                    ready_nodes: 1,
                    unhealthy_pods: 0,
                },
                conditions: Vec::new(),
                workloads: ClusterStatusWorkloadSummary {
                    pods: 2,
                    deployments: 1,
                    services: 1,
                    config_maps: 0,
                    secrets: 0,
                },
                recent_events: Vec::new(),
            })
        }
    }

    #[async_trait]
    impl miku_api::KubernetesResourceReader for DummyServices {
        async fn list_resources(&self, _query: ResourceQuery) -> miku_core::Result<ResourceList> {
            Ok(ResourceList::default())
        }
    }

    #[async_trait]
    impl PodLogService for DummyServices {
        async fn read_logs(&self, _query: PodLogQuery) -> miku_core::Result<Vec<LogLine>> {
            Ok(vec![LogLine {
                text: "ready".to_owned(),
            }])
        }
    }

    #[async_trait]
    impl ClusterInitializer for DummyServices {
        async fn initialize_cluster(
            &self,
            _request: ClusterInitializeRequest,
        ) -> miku_core::Result<ClusterConnectionInfo> {
            Ok(ClusterConnectionInfo {
                version: "v1.35.0".to_owned(),
                platform: None,
            })
        }
    }

    #[async_trait]
    impl KubernetesResourceWriter for DummyServices {
        async fn evict_pod(&self, _request: PodEvictRequest) -> miku_core::Result<()> {
            Ok(())
        }
    }

    #[async_trait]
    impl KubernetesWatchService for DummyServices {}

    #[async_trait]
    impl PodAttachService for DummyServices {}

    #[async_trait]
    impl LocalPreferenceStore for DummyServices {
        async fn get_preference(&self, _key: &str) -> miku_core::Result<Option<Value>> {
            Ok(None)
        }

        async fn set_preference(&self, _key: &str, _value: Value) -> miku_core::Result<()> {
            Ok(())
        }
    }

    #[async_trait]
    impl LlmSettingsStore for DummyServices {
        async fn get_llm_settings(&self) -> miku_core::Result<LlmProviderSettings> {
            Ok(LlmProviderSettings::default())
        }

        async fn set_llm_settings(&self, _settings: LlmProviderSettings) -> miku_core::Result<()> {
            Ok(())
        }
    }

    fn request() -> AgentTurnRequest {
        AgentTurnRequest {
            session_id: "agent-1".to_owned(),
            message: "List clusters".to_owned(),
            context: AgentContext {
                cluster_id: Some(ClusterId::new("local")),
                cluster_name: Some("local".to_owned()),
                selected_resource: None,
                namespace: None,
            },
            history: Vec::new(),
        }
    }

    fn provider(responses: Vec<ChatMessage>) -> Arc<dyn LlmProvider> {
        Arc::new(MockProvider {
            responses: Mutex::new(responses),
        })
    }

    #[tokio::test]
    async fn returns_final_message_without_tools() {
        let runtime = AgentRuntime::new(provider(vec![ChatMessage::assistant("done")]));

        let response = runtime
            .run_turn(request(), Arc::new(DummyServices))
            .await
            .unwrap();

        assert_eq!(response.message.content, "done");
        assert_eq!(response.status, AgentTurnStatus::Completed);
        assert!(response.tool_calls.is_empty());
    }

    #[tokio::test]
    async fn stops_when_complete_task_is_called() {
        let runtime = AgentRuntime::new(provider(vec![ChatMessage {
            role: "assistant".to_owned(),
            content: None,
            tool_call_id: None,
            tool_calls: vec![ToolCall {
                id: "call-1".to_owned(),
                r#type: "function".to_owned(),
                function: ToolCallFunction {
                    name: "complete_task".to_owned(),
                    arguments: r#"{"summary":"checked","status":"success"}"#.to_owned(),
                },
            }],
        }]));

        let response = runtime
            .run_turn(request(), Arc::new(DummyServices))
            .await
            .unwrap();

        assert_eq!(response.message.content, "checked");
        assert_eq!(response.status, AgentTurnStatus::Completed);
        assert_eq!(response.tool_calls[0].name, "complete_task");
    }

    #[test]
    fn readonly_registry_exposes_complete_task() {
        let definitions = ToolRegistry::miku_readonly()
            .definitions()
            .into_iter()
            .map(|definition: ToolDefinition| definition.name)
            .collect::<Vec<_>>();

        assert!(definitions.contains(&"list_clusters".to_owned()));
        assert!(definitions.contains(&"complete_task".to_owned()));
    }
}
