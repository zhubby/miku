mod client;
mod clusters;
mod pods;
mod resource_cache;
mod resources;
mod status;

pub use client::KubeServices;
pub use resources::{api_resource, resource_query_path};

use async_trait::async_trait;
use miku_api::{
    AgentConversation, AgentConversationStore, AgentConversationSummary, AgentPersistedMessage,
    AgentService, AgentTurnRequest, AgentTurnResponse, AppendAgentMessageRequest,
    ClusterConfigStore, ClusterRegistry, CreateAgentConversationRequest, LlmProviderSettings,
    LlmSettingsStore, LocalPreferenceStore, MikuServices,
};
use std::sync::Arc;

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

#[async_trait]
impl<S> LlmSettingsStore for KubeServices<S>
where
    S: LlmSettingsStore + Clone + Send + Sync,
{
    async fn get_llm_settings(&self) -> miku_core::Result<LlmProviderSettings> {
        self.store.get_llm_settings().await
    }

    async fn set_llm_settings(&self, settings: LlmProviderSettings) -> miku_core::Result<()> {
        self.store.set_llm_settings(settings).await
    }
}

#[async_trait]
impl<S> AgentService for KubeServices<S>
where
    S: ClusterConfigStore
        + ClusterRegistry
        + LocalPreferenceStore
        + LlmSettingsStore
        + Clone
        + Send
        + Sync
        + 'static,
{
    async fn run_agent_turn(
        &self,
        request: AgentTurnRequest,
    ) -> miku_core::Result<AgentTurnResponse> {
        let settings = self.get_llm_settings().await?;
        let provider_config = miku_agent::ProviderConfig::from_settings(settings)?;
        let provider = Arc::new(miku_agent::OpenAiCompatibleProvider::new(provider_config));
        let agent = miku_agent::MikuAgentService::new(provider, Arc::new(self.clone()));
        agent.run_agent_turn(request).await
    }
}

#[async_trait]
impl<S> AgentConversationStore for KubeServices<S>
where
    S: AgentConversationStore + Clone + Send + Sync,
{
    async fn list_agent_conversations(&self) -> miku_core::Result<Vec<AgentConversationSummary>> {
        self.store.list_agent_conversations().await
    }

    async fn get_agent_conversation(
        &self,
        id: &str,
    ) -> miku_core::Result<Option<AgentConversation>> {
        self.store.get_agent_conversation(id).await
    }

    async fn create_agent_conversation(
        &self,
        request: CreateAgentConversationRequest,
    ) -> miku_core::Result<AgentConversationSummary> {
        self.store.create_agent_conversation(request).await
    }

    async fn append_agent_message(
        &self,
        request: AppendAgentMessageRequest,
    ) -> miku_core::Result<AgentPersistedMessage> {
        self.store.append_agent_message(request).await
    }

    async fn delete_agent_conversation(&self, id: &str) -> miku_core::Result<()> {
        self.store.delete_agent_conversation(id).await
    }
}

impl<S> MikuServices for KubeServices<S> where
    S: ClusterConfigStore
        + ClusterRegistry
        + AgentConversationStore
        + LocalPreferenceStore
        + LlmSettingsStore
        + Clone
        + Send
        + Sync
        + 'static
{
}

#[cfg(test)]
mod tests {
    use miku_api::{AgentContext, AgentService, AgentTurnRequest};

    use crate::KubeServices;

    #[tokio::test]
    async fn run_agent_turn_requires_file_llm_settings() {
        let temp = tempfile::tempdir().unwrap();
        let store = miku_store::SqliteStore::initialize(miku_store::StorePaths::from_root(
            temp.path().join(".miku"),
        ))
        .await
        .unwrap();
        let services = KubeServices::new_offline(store);

        let result = services
            .run_agent_turn(AgentTurnRequest {
                session_id: "agent-1".to_owned(),
                message: "hello".to_owned(),
                context: AgentContext {
                    cluster_id: None,
                    cluster_name: None,
                    selected_resource: None,
                    namespace: None,
                },
                history: Vec::new(),
            })
            .await;

        assert!(result.unwrap_err().to_string().contains("llm.base_url"));
    }
}
