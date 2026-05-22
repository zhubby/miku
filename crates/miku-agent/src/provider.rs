use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn chat(&self, request: ProviderChatRequest) -> miku_core::Result<ProviderChatResponse>;
}

#[derive(Clone, Debug)]
pub struct ProviderConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

impl ProviderConfig {
    pub fn from_env() -> miku_core::Result<Self> {
        let base_url = std::env::var("MIKU_LLM_BASE_URL").map_err(|_| {
            miku_core::MikuError::Config("MIKU_LLM_BASE_URL is required for agent mode".to_owned())
        })?;
        let api_key = std::env::var("MIKU_LLM_API_KEY").map_err(|_| {
            miku_core::MikuError::Config("MIKU_LLM_API_KEY is required for agent mode".to_owned())
        })?;
        let model = std::env::var("MIKU_LLM_MODEL").map_err(|_| {
            miku_core::MikuError::Config("MIKU_LLM_MODEL is required for agent mode".to_owned())
        })?;

        Ok(Self {
            base_url,
            api_key,
            model,
        })
    }
}

#[derive(Clone, Debug)]
pub struct OpenAiCompatibleProvider {
    config: ProviderConfig,
    client: reqwest::Client,
}

impl OpenAiCompatibleProvider {
    pub fn new(config: ProviderConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
        }
    }

    pub fn from_env() -> miku_core::Result<Self> {
        Ok(Self::new(ProviderConfig::from_env()?))
    }

    fn endpoint(&self) -> String {
        format!(
            "{}/chat/completions",
            self.config.base_url.trim_end_matches('/')
        )
    }
}

#[async_trait]
impl LlmProvider for OpenAiCompatibleProvider {
    #[tracing::instrument(name = "agent.provider.chat", skip_all, fields(model = %self.config.model))]
    async fn chat(&self, request: ProviderChatRequest) -> miku_core::Result<ProviderChatResponse> {
        let body = ChatCompletionRequest {
            model: self.config.model.clone(),
            messages: request.messages,
            tools: request.tools.into_iter().map(ChatTool::from).collect(),
            tool_choice: "auto",
        };

        let response = self
            .client
            .post(self.endpoint())
            .bearer_auth(&self.config.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))?
            .error_for_status()
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))?
            .json::<ChatCompletionResponse>()
            .await
            .map_err(|error| miku_core::MikuError::Transport(error.to_string()))?;

        let choice = response.choices.into_iter().next().ok_or_else(|| {
            miku_core::MikuError::Transport("LLM response did not include a choice".to_owned())
        })?;

        Ok(ProviderChatResponse {
            message: choice.message,
        })
    }
}

#[derive(Clone, Debug)]
pub struct ProviderChatRequest {
    pub messages: Vec<ChatMessage>,
    pub tools: Vec<ToolDefinition>,
}

#[derive(Clone, Debug)]
pub struct ProviderChatResponse {
    pub message: ChatMessage,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".to_owned(),
            content: Some(content.into()),
            tool_call_id: None,
            tool_calls: Vec::new(),
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".to_owned(),
            content: Some(content.into()),
            tool_call_id: None,
            tool_calls: Vec::new(),
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".to_owned(),
            content: Some(content.into()),
            tool_call_id: None,
            tool_calls: Vec::new(),
        }
    }

    pub fn tool(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: "tool".to_owned(),
            content: Some(content.into()),
            tool_call_id: Some(tool_call_id.into()),
            tool_calls: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(default)]
    pub r#type: String,
    pub function: ToolCallFunction,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolCallFunction {
    pub name: String,
    pub arguments: String,
}

#[derive(Clone, Debug)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

#[derive(Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatMessage>,
    tools: Vec<ChatTool>,
    tool_choice: &'static str,
}

#[derive(Serialize)]
struct ChatTool {
    r#type: &'static str,
    function: ChatToolFunction,
}

impl From<ToolDefinition> for ChatTool {
    fn from(value: ToolDefinition) -> Self {
        Self {
            r#type: "function",
            function: ChatToolFunction {
                name: value.name,
                description: value.description,
                parameters: value.parameters,
            },
        }
    }
}

#[derive(Serialize)]
struct ChatToolFunction {
    name: String,
    description: String,
    parameters: Value,
}

#[derive(Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}
