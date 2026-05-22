use std::pin::Pin;

use async_trait::async_trait;
use futures::{Stream, StreamExt};
use miku_api::LlmProviderSettings;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub type ProviderChatStream =
    Pin<Box<dyn Stream<Item = miku_core::Result<ProviderChatStreamEvent>> + Send>>;

#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn chat(&self, request: ProviderChatRequest) -> miku_core::Result<ProviderChatResponse>;

    async fn chat_stream(
        &self,
        request: ProviderChatRequest,
    ) -> miku_core::Result<ProviderChatStream> {
        let _ = request;
        Err(miku_core::MikuError::UnsupportedRuntime(
            "streaming chat is not implemented for this provider".to_owned(),
        ))
    }
}

#[derive(Clone, Debug)]
pub struct ProviderConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub stream: bool,
}

impl ProviderConfig {
    pub fn from_settings(settings: LlmProviderSettings) -> miku_core::Result<Self> {
        let base_url = required_setting(settings.base_url, "llm.base_url")?;
        let api_key = required_setting(settings.api_key, "llm.api_key")?;
        let model = required_setting(settings.model, "llm.model")?;

        Ok(Self {
            base_url,
            api_key,
            model,
            stream: settings.stream,
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

    fn endpoint(&self) -> String {
        format!(
            "{}/chat/completions",
            self.config.base_url.trim_end_matches('/')
        )
    }
}

fn required_setting(value: String, field: &str) -> miku_core::Result<String> {
    let value = value.trim().to_owned();
    if value.is_empty() {
        return Err(miku_core::MikuError::Config(format!(
            "{field} is required for agent mode"
        )));
    }

    Ok(value)
}

#[async_trait]
impl LlmProvider for OpenAiCompatibleProvider {
    #[tracing::instrument(name = "agent.provider.chat", skip_all, fields(model = %self.config.model))]
    async fn chat(&self, request: ProviderChatRequest) -> miku_core::Result<ProviderChatResponse> {
        if self.config.stream {
            return self.chat_stream_to_response(request).await;
        }

        let body = self.chat_completion_request(request, false);

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

    #[tracing::instrument(name = "agent.provider.chat_stream", skip_all, fields(model = %self.config.model))]
    async fn chat_stream(
        &self,
        request: ProviderChatRequest,
    ) -> miku_core::Result<ProviderChatStream> {
        let body = self.chat_completion_request(request, true);
        let bytes = self
            .client
            .post(self.endpoint())
            .bearer_auth(&self.config.api_key)
            .json(&body)
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
                Vec::<miku_core::Result<ProviderChatStreamEvent>>::new(),
                ChatMessageAccumulator::default(),
                false,
            ),
            |(mut bytes, mut buffer, mut pending, mut accumulator, mut finished)| async move {
                loop {
                    if let Some(event) = pending.pop() {
                        return Some((event, (bytes, buffer, pending, accumulator, finished)));
                    }

                    if finished {
                        return None;
                    }

                    match bytes.next().await {
                        Some(Ok(chunk)) => {
                            buffer.push_str(&String::from_utf8_lossy(&chunk));
                            let parsed = parse_chat_completion_sse_events(
                                &mut buffer,
                                &mut accumulator,
                                &mut finished,
                            );
                            pending.extend(parsed.into_iter().rev());
                        }
                        Some(Err(error)) => {
                            finished = true;
                            return Some((
                                Err(miku_core::MikuError::Transport(error.to_string())),
                                (bytes, buffer, pending, accumulator, finished),
                            ));
                        }
                        None => {
                            finished = true;
                            if buffer.trim().is_empty() {
                                return None;
                            }

                            return Some((
                                Err(miku_core::MikuError::Transport(
                                    "LLM stream ended with an incomplete SSE frame".to_owned(),
                                )),
                                (bytes, buffer, pending, accumulator, finished),
                            ));
                        }
                    }
                }
            },
        )
        .boxed())
    }
}

impl OpenAiCompatibleProvider {
    async fn chat_stream_to_response(
        &self,
        request: ProviderChatRequest,
    ) -> miku_core::Result<ProviderChatResponse> {
        let mut stream = self.chat_stream(request).await?;
        while let Some(event) = stream.next().await {
            match event? {
                ProviderChatStreamEvent::Delta(_) => {}
                ProviderChatStreamEvent::Completed(response) => return Ok(response),
            }
        }

        Err(miku_core::MikuError::Transport(
            "LLM stream ended before a completed response".to_owned(),
        ))
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

#[derive(Clone, Debug)]
pub enum ProviderChatStreamEvent {
    Delta(ChatMessageDelta),
    Completed(ProviderChatResponse),
}

#[derive(Clone, Debug, Deserialize)]
pub struct ChatMessageDelta {
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub tool_calls: Vec<ToolCallDelta>,
    #[serde(default)]
    pub finish_reason: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ToolCallDelta {
    pub index: usize,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub r#type: Option<String>,
    #[serde(default)]
    pub function: ToolCallFunctionDelta,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ToolCallFunctionDelta {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub arguments: Option<String>,
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
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<ChatTool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<&'static str>,
    stream: bool,
}

impl OpenAiCompatibleProvider {
    fn chat_completion_request(
        &self,
        request: ProviderChatRequest,
        stream: bool,
    ) -> ChatCompletionRequest {
        let tools: Vec<ChatTool> = request.tools.into_iter().map(ChatTool::from).collect();
        let tool_choice = (!tools.is_empty()).then_some("auto");

        ChatCompletionRequest {
            model: self.config.model.clone(),
            messages: request.messages,
            tools,
            tool_choice,
            stream,
        }
    }
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

#[derive(Deserialize)]
struct ChatCompletionChunk {
    choices: Vec<ChatChunkChoice>,
}

#[derive(Deserialize)]
struct ChatChunkChoice {
    delta: ChatMessageDelta,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Default)]
struct ChatMessageAccumulator {
    role: Option<String>,
    content: String,
    tool_calls: Vec<ToolCallAccumulator>,
}

impl ChatMessageAccumulator {
    fn apply(&mut self, delta: &ChatMessageDelta) {
        if let Some(role) = &delta.role {
            self.role = Some(role.clone());
        }

        if let Some(content) = &delta.content {
            self.content.push_str(content);
        }

        for tool_delta in &delta.tool_calls {
            while self.tool_calls.len() <= tool_delta.index {
                self.tool_calls.push(ToolCallAccumulator::default());
            }
            self.tool_calls[tool_delta.index].apply(tool_delta);
        }
    }

    fn message(&self) -> ChatMessage {
        ChatMessage {
            role: self.role.clone().unwrap_or_else(|| "assistant".to_owned()),
            content: (!self.content.is_empty()).then(|| self.content.clone()),
            tool_call_id: None,
            tool_calls: self
                .tool_calls
                .iter()
                .map(ToolCallAccumulator::tool_call)
                .collect(),
        }
    }
}

#[derive(Default)]
struct ToolCallAccumulator {
    id: Option<String>,
    r#type: Option<String>,
    name: String,
    arguments: String,
}

impl ToolCallAccumulator {
    fn apply(&mut self, delta: &ToolCallDelta) {
        if let Some(id) = &delta.id {
            self.id = Some(id.clone());
        }

        if let Some(tool_type) = &delta.r#type {
            self.r#type = Some(tool_type.clone());
        }

        if let Some(name) = &delta.function.name {
            self.name.push_str(name);
        }

        if let Some(arguments) = &delta.function.arguments {
            self.arguments.push_str(arguments);
        }
    }

    fn tool_call(&self) -> ToolCall {
        ToolCall {
            id: self.id.clone().unwrap_or_default(),
            r#type: self.r#type.clone().unwrap_or_else(|| "function".to_owned()),
            function: ToolCallFunction {
                name: self.name.clone(),
                arguments: self.arguments.clone(),
            },
        }
    }
}

fn parse_chat_completion_sse_events(
    buffer: &mut String,
    accumulator: &mut ChatMessageAccumulator,
    finished: &mut bool,
) -> Vec<miku_core::Result<ProviderChatStreamEvent>> {
    let mut events = Vec::new();

    while let Some((index, delimiter_len)) = next_sse_frame_boundary(buffer) {
        let frame = buffer[..index].to_owned();
        buffer.drain(..index + delimiter_len);

        let data = sse_frame_data(&frame);
        if data.is_empty() {
            continue;
        }

        let data = data.join("\n");
        if data.trim() == "[DONE]" {
            *finished = true;
            events.push(Ok(ProviderChatStreamEvent::Completed(
                ProviderChatResponse {
                    message: accumulator.message(),
                },
            )));
            continue;
        }

        let chunk = match serde_json::from_str::<ChatCompletionChunk>(&data) {
            Ok(chunk) => chunk,
            Err(error) => {
                events.push(Err(miku_core::MikuError::Transport(error.to_string())));
                continue;
            }
        };

        for choice in chunk.choices {
            let mut delta = choice.delta;
            if delta.finish_reason.is_none() {
                delta.finish_reason = choice.finish_reason;
            }
            accumulator.apply(&delta);
            events.push(Ok(ProviderChatStreamEvent::Delta(delta)));
        }
    }

    events
}

fn sse_frame_data(frame: &str) -> Vec<String> {
    frame
        .lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .map(|value| value.trim_start().to_owned())
        .collect()
}

fn next_sse_frame_boundary(buffer: &str) -> Option<(usize, usize)> {
    let lf = buffer.find("\n\n").map(|index| (index, 2));
    let crlf = buffer.find("\r\n\r\n").map(|index| (index, 4));

    match (lf, crlf) {
        (Some(lf), Some(crlf)) => Some(lf.min(crlf)),
        (Some(boundary), None) | (None, Some(boundary)) => Some(boundary),
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_config_rejects_empty_settings_fields() {
        let error = ProviderConfig::from_settings(LlmProviderSettings {
            base_url: "https://api.openai.com/v1".to_owned(),
            api_key: " ".to_owned(),
            model: "gpt-5.1".to_owned(),
            stream: false,
        })
        .unwrap_err();

        assert!(error.to_string().contains("llm.api_key is required"));
    }

    #[test]
    fn provider_config_builds_from_settings() {
        let config = ProviderConfig::from_settings(LlmProviderSettings {
            base_url: " https://api.openai.com/v1 ".to_owned(),
            api_key: " sk-test ".to_owned(),
            model: " gpt-5.1 ".to_owned(),
            stream: true,
        })
        .unwrap();

        assert_eq!(config.base_url, "https://api.openai.com/v1");
        assert_eq!(config.api_key, "sk-test");
        assert_eq!(config.model, "gpt-5.1");
        assert!(config.stream);
    }

    #[test]
    fn parses_streamed_content_into_completed_message() {
        let mut buffer = concat!(
            "data: {\"choices\":[{\"delta\":{\"role\":\"assistant\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"hel\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"lo\"},\"finish_reason\":\"stop\"}]}\n\n",
            "data: [DONE]\n\n",
        )
        .to_owned();
        let mut accumulator = ChatMessageAccumulator::default();
        let mut finished = false;

        let events = parse_chat_completion_sse_events(&mut buffer, &mut accumulator, &mut finished);

        assert!(finished);
        assert!(buffer.is_empty());
        assert_eq!(events.len(), 4);
        let Ok(ProviderChatStreamEvent::Completed(response)) = &events[3] else {
            panic!("expected completed stream event");
        };
        assert_eq!(response.message.role, "assistant");
        assert_eq!(response.message.content.as_deref(), Some("hello"));
        assert!(response.message.tool_calls.is_empty());
    }

    #[test]
    fn parses_streamed_tool_call_arguments_into_completed_message() {
        let mut buffer = concat!(
            "data: {\"choices\":[{\"delta\":{\"role\":\"assistant\",\"tool_calls\":[",
            "{\"index\":0,\"id\":\"call_1\",\"type\":\"function\",\"function\":{\"name\":\"list_pods\",\"arguments\":\"{\\\"namespace\\\"\"}}",
            "]}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[",
            "{\"index\":0,\"function\":{\"arguments\":\":\\\"default\\\"}\"}}",
            "]},\"finish_reason\":\"tool_calls\"}]}\n\n",
            "data: [DONE]\n\n",
        )
        .to_owned();
        let mut accumulator = ChatMessageAccumulator::default();
        let mut finished = false;

        let events = parse_chat_completion_sse_events(&mut buffer, &mut accumulator, &mut finished);

        assert!(finished);
        let Ok(ProviderChatStreamEvent::Completed(response)) = events.last().unwrap() else {
            panic!("expected completed stream event");
        };
        assert_eq!(response.message.content, None);
        assert_eq!(response.message.tool_calls.len(), 1);
        let tool_call = &response.message.tool_calls[0];
        assert_eq!(tool_call.id, "call_1");
        assert_eq!(tool_call.r#type, "function");
        assert_eq!(tool_call.function.name, "list_pods");
        assert_eq!(tool_call.function.arguments, "{\"namespace\":\"default\"}");
    }

    #[test]
    fn parses_crlf_sse_frames() {
        let mut buffer = concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"ok\"}}]}\r\n\r\n",
            "data: [DONE]\r\n\r\n",
        )
        .to_owned();
        let mut accumulator = ChatMessageAccumulator::default();
        let mut finished = false;

        let events = parse_chat_completion_sse_events(&mut buffer, &mut accumulator, &mut finished);

        assert!(finished);
        assert!(buffer.is_empty());
        let Ok(ProviderChatStreamEvent::Completed(response)) = events.last().unwrap() else {
            panic!("expected completed stream event");
        };
        assert_eq!(response.message.content.as_deref(), Some("ok"));
    }
}
