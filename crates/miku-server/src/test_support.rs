use futures::StreamExt;
use miku_api::*;
use miku_core::ClusterId;
use std::sync::{LazyLock, Mutex};

pub(crate) struct DummyServices;

static LLM_SETTINGS: LazyLock<Mutex<LlmProviderSettings>> =
    LazyLock::new(|| Mutex::new(LlmProviderSettings::default()));

#[async_trait::async_trait]
impl ClusterRegistry for DummyServices {
    async fn list_clusters(&self) -> miku_core::Result<Vec<ClusterSummary>> {
        Ok(vec![ClusterSummary {
            id: ClusterId::new("local"),
            name: "local".to_owned(),
            context: "kind-miku".to_owned(),
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

#[async_trait::async_trait]
impl KubernetesResourceReader for DummyServices {
    async fn list_resources(&self, query: ResourceQuery) -> miku_core::Result<ResourceList> {
        assert_eq!(query.resource.plural, "pods");
        Ok(ResourceList {
            items: vec![ResourceSummary {
                name: "api".to_owned(),
                namespace: Some("default".to_owned()),
                kind: "Pod".to_owned(),
                status: Some("Running".to_owned()),
                raw: serde_json::json!({}),
            }],
            continue_token: None,
        })
    }
}

#[async_trait::async_trait]
impl ClusterInitializer for DummyServices {
    async fn initialize_cluster(
        &self,
        request: ClusterInitializeRequest,
    ) -> miku_core::Result<ClusterConnectionInfo> {
        assert_eq!(request.cluster_id, ClusterId::new("local"));
        Ok(ClusterConnectionInfo {
            version: "v1.35.0".to_owned(),
            platform: Some("darwin/arm64".to_owned()),
        })
    }
}

#[async_trait::async_trait]
impl ClusterStatusReader for DummyServices {
    async fn get_cluster_status(
        &self,
        request: ClusterStatusRequest,
    ) -> miku_core::Result<ClusterStatusReport> {
        assert_eq!(request.cluster_id, ClusterId::new("local"));
        Ok(ClusterStatusReport {
            overview: ClusterStatusOverview {
                version: "v1.35.0".to_owned(),
                platform: Some("darwin/arm64".to_owned()),
                namespaces: 2,
                nodes: 1,
                pods: 3,
                ready_nodes: 1,
                unhealthy_pods: 0,
            },
            conditions: vec![ClusterStatusCondition {
                name: "Nodes".to_owned(),
                status: "1/1 ready".to_owned(),
                severity: ClusterStatusSeverity::Ok,
                message: "All nodes are ready".to_owned(),
            }],
            workloads: ClusterStatusWorkloadSummary {
                pods: 3,
                deployments: 1,
                services: 2,
                config_maps: 4,
                secrets: 5,
            },
            recent_events: vec![ClusterStatusEventSummary {
                namespace: Some("default".to_owned()),
                involved_object: "Pod/api".to_owned(),
                reason: "Started".to_owned(),
                message: "Started container api".to_owned(),
                event_type: "Normal".to_owned(),
            }],
        })
    }
}

#[async_trait::async_trait]
impl KubernetesResourceWriter for DummyServices {
    async fn apply_resource(
        &self,
        request: ResourceApplyRequest,
    ) -> miku_core::Result<ResourceSummary> {
        Ok(ResourceSummary {
            name: request.name,
            namespace: request.namespace,
            kind: "Pod".to_owned(),
            status: Some("Running".to_owned()),
            raw: request.manifest,
        })
    }

    async fn delete_resource(&self, _request: ResourceDeleteRequest) -> miku_core::Result<()> {
        Ok(())
    }

    async fn evict_pod(&self, _request: PodEvictRequest) -> miku_core::Result<()> {
        Ok(())
    }
}

#[async_trait::async_trait]
impl KubernetesWatchService for DummyServices {
    async fn watch_resources(
        &self,
        query: ResourceQuery,
    ) -> miku_core::Result<BoxEventStream<ResourceEvent>> {
        assert_eq!(query.resource.plural, "pods");
        assert_eq!(query.namespace.as_deref(), Some("default"));
        Ok(futures::stream::once(async {
            Ok(ResourceEvent::Snapshot(ResourceList {
                items: vec![ResourceSummary {
                    name: "api".to_owned(),
                    namespace: Some("default".to_owned()),
                    kind: "Pod".to_owned(),
                    status: Some("Running".to_owned()),
                    raw: serde_json::json!({}),
                }],
                continue_token: None,
            }))
        })
        .boxed())
    }
}

#[async_trait::async_trait]
impl PodLogService for DummyServices {
    async fn read_logs(&self, _query: PodLogQuery) -> miku_core::Result<Vec<LogLine>> {
        Ok(vec![LogLine {
            text: "api started".to_owned(),
        }])
    }
}

#[async_trait::async_trait]
impl PodAttachService for DummyServices {
    async fn attach_pod(&self, _request: PodAttachRequest) -> miku_core::Result<PodAttachSession> {
        let (input, _input_rx) = futures::channel::mpsc::unbounded();
        let output = futures::stream::iter([
            Ok(PodAttachOutput::Stdout(b"attached\n".to_vec())),
            Ok(PodAttachOutput::Closed),
        ])
        .boxed();
        Ok(PodAttachSession { input, output })
    }
}

#[async_trait::async_trait]
impl LocalPreferenceStore for DummyServices {
    async fn get_preference(&self, _key: &str) -> miku_core::Result<Option<serde_json::Value>> {
        Ok(None)
    }

    async fn set_preference(&self, _key: &str, _value: serde_json::Value) -> miku_core::Result<()> {
        Ok(())
    }
}

#[async_trait::async_trait]
impl LlmSettingsStore for DummyServices {
    async fn get_llm_settings(&self) -> miku_core::Result<LlmProviderSettings> {
        Ok(LLM_SETTINGS.lock().unwrap().clone())
    }

    async fn set_llm_settings(&self, settings: LlmProviderSettings) -> miku_core::Result<()> {
        *LLM_SETTINGS.lock().unwrap() = settings;
        Ok(())
    }
}

#[async_trait::async_trait]
impl AgentService for DummyServices {
    async fn run_agent_turn(
        &self,
        request: AgentTurnRequest,
    ) -> miku_core::Result<AgentTurnResponse> {
        Ok(AgentTurnResponse {
            session_id: request.session_id,
            message: AgentMessage {
                role: AgentRole::Assistant,
                content: "Agent response".to_owned(),
            },
            status: AgentTurnStatus::Completed,
            tool_calls: Vec::new(),
            events: vec![AgentEvent::Completed {
                status: AgentTurnStatus::Completed,
                summary: "Agent response".to_owned(),
            }],
        })
    }
}

impl MikuServices for DummyServices {}
