mod provider;
mod runtime;
mod tools;

use std::sync::Arc;

use async_trait::async_trait;
use miku_api::{
    AgentService, AgentTurnRequest, AgentTurnResponse, ClusterRegistry, ClusterStatusReader,
    KubernetesResourceReader, PodLogService, ServiceBounds,
};

pub use provider::{LlmProvider, OpenAiCompatibleProvider, ProviderConfig};
pub use runtime::{AgentRuntime, RunLimits};

pub trait AgentToolServices:
    ClusterRegistry + ClusterStatusReader + KubernetesResourceReader + PodLogService + ServiceBounds
{
}

impl<T> AgentToolServices for T where
    T: ClusterRegistry
        + ClusterStatusReader
        + KubernetesResourceReader
        + PodLogService
        + ServiceBounds
{
}

#[derive(Clone)]
pub struct MikuAgentService<S> {
    runtime: AgentRuntime,
    services: Arc<S>,
}

impl<S> MikuAgentService<S>
where
    S: AgentToolServices + 'static,
{
    pub fn new(provider: Arc<dyn LlmProvider>, services: Arc<S>) -> Self {
        Self {
            runtime: AgentRuntime::new(provider),
            services,
        }
    }

    pub fn with_limits(mut self, limits: RunLimits) -> Self {
        self.runtime = self.runtime.with_limits(limits);
        self
    }

    pub fn from_env(services: Arc<S>) -> miku_core::Result<Self> {
        let provider = Arc::new(OpenAiCompatibleProvider::from_env()?);
        Ok(Self::new(provider, services))
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<S> AgentService for MikuAgentService<S>
where
    S: AgentToolServices + 'static,
{
    async fn run_agent_turn(
        &self,
        request: AgentTurnRequest,
    ) -> miku_core::Result<AgentTurnResponse> {
        self.runtime.run_turn(request, self.services.clone()).await
    }
}
