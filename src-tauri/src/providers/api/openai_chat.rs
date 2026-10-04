use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::providers::credentials::CredentialSet;
use async_trait::async_trait;
use eventsource_stream::Eventsource;
use futures::StreamExt;
use reqwest::{Client, StatusCode};
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use zeroize::{Zeroize, Zeroizing};

use crate::providers::builtin::OpenAiChatConfig;
use crate::providers::{
    ChatMessage, ChatProvider, ChatRequest, ChatResponse, ChatRole, ContentPart, ProviderError,
    ProviderEvent, StopReason, ToolCall, Usage,
};

pub(crate) struct OpenAiChatProvider {
    client: Client,
    endpoint: String,
    credentials: CredentialSet,
    cursor: AtomicUsize,
}

impl OpenAiChatProvider {
    pub(crate) fn new(config: OpenAiChatConfig) -> Result<Self, ProviderError> {
        let endpoint = config.endpoint.trim().trim_end_matches('/').to_string();
        let parsed =
            reqwest::Url::parse(&endpoint).map_err(|_| ProviderError::InvalidConfiguration)?;
        if !matches!(parsed.scheme(), "http" | "https")
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.query().is_some()
            || parsed.fragment().is_some()
            || !config.credentials.matches_endpoint(&endpoint)
        {
            return Err(ProviderError::InvalidConfiguration);
        }
        let client = Client::builder()
            .timeout(config.timeout)
            .build()
            .map_err(|_| ProviderError::InvalidConfiguration)?;
        Ok(Self {
            client,
            endpoint,
            credentials: config.credentials,
            cursor: AtomicUsize::new(0),
        })
    }

    fn redact(&self, raw: &str) -> String {
        let mut text = Zeroizing::new(raw.to_owned());
        for key in self.credentials.values() {
            let clean = text.replace(key.expose_secret(), "[REDACTED]");
            text.zeroize();
            *text = clean;
        }
        std::mem::take(&mut *text)
    }
    fn wipe_and_redact(&self, value: &mut String) {
        let clean = self.redact(value);
        value.zeroize();
        *value = clean;
    }
    // Retain a suffix so a key split across SSE chunks never reaches consumers.
    fn drain_redacted(&self, pending: &mut String, flush: bool) -> String {
        self.wipe_and_redact(pending);
        let hold = if flush {
            0
        } else {
            self.credentials
                .values()
                .iter()
                .map(|k| k.expose_secret().len())
                .max()
                .unwrap_or(1)
                .saturating_sub(1)
        };
        let mut end = pending.len().saturating_sub(hold);
        while !pending.is_char_boundary(end) {
            end -= 1;
        }
        pending.drain(..end).collect()
    }
    fn sanitize_response(&self, response: &mut ChatResponse) {
        self.wipe_and_redact(&mut response.model);
        for part in &mut response.message.content {
            match part {
                ContentPart::Text(value) | ContentPart::ImageUrl(value) => {
                    self.wipe_and_redact(value)
                }
            }
        }
        for call in &mut response.message.tool_calls {
            self.wipe_and_redact(&mut call.id);
            self.wipe_and_redact(&mut call.name);
            self.wipe_and_redact(&mut call.arguments);
        }
    }

    fn rotated_keys(&self) -> Vec<&str> {
        if self.credentials.is_empty() {
            return vec![""];
        }
        let start = self.cursor.fetch_add(1, Ordering::Relaxed) % self.credentials.len();
        self.credentials
            .values()
            .iter()
            .cycle()
            .skip(start)
            .take(self.credentials.len())
            .map(|value| value.expose_secret().as_str())
            .collect()
    }

    async fn send(&self, request: &WireRequest) -> Result<reqwest::Response, ProviderError> {
        let url = format!("{}/chat/completions", self.endpoint);
        let keys = self.rotated_keys();
        let mut last_error = ProviderError::Unavailable;
        for (index, key) in keys.iter().enumerate() {
            let header = Zeroizing::new(format!("Bearer {key}"));
            let mut authorization = reqwest::header::HeaderValue::from_str(&header)
                .map_err(|_| ProviderError::InvalidConfiguration)?;
            authorization.set_sensitive(true);
            let result = self
                .client
                .post(&url)
                .header(reqwest::header::AUTHORIZATION, authorization)
                .json(request)
                .send()
                .await;
            let response = match result {
                Ok(response) => response,
                Err(error) => {
                    last_error = if error.is_timeout() {
                        ProviderError::Timeout
                    } else {
                        ProviderError::Network
                    };
                    if index + 1 < keys.len() {
                        continue;
                    }
                    return Err(last_error);
                }
            };
            let status = response.status();
            if status.is_success() {
                return Ok(response);
            }
            last_error = status_error(status);
            if retryable_status(status) && index + 1 < keys.len() {
                continue;
            }
            return Err(last_error);
        }
        Err(last_error)
    }
}

#[async_trait]
impl ChatProvider for OpenAiChatProvider {
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, ProviderError> {
        let wire_request = WireRequest::from_request(request, false);
        let response = self.send(&wire_request).await?;
        let wire: WireResponse = response
            .json()
            .await
            .map_err(|_| ProviderError::InvalidResponse)?;
        let mut response: ChatResponse = wire.try_into()?;
        self.sanitize_response(&mut response);
        Ok(response)
    }

    async fn stream_chat(
        &self,
        request: ChatRequest,
        events: mpsc::Sender<ProviderEvent>,
    ) -> Result<ChatResponse, ProviderError> {
        let requested_model = request.model.clone();
        let wire_request = WireRequest::from_request(request, true);
        let response = self.send(&wire_request).await?;
        let mut stream = response.bytes_stream().eventsource();
        let mut text = String::new();
        let mut pending_text = Zeroizing::new(String::new());
        let mut calls: BTreeMap<usize, ToolCallBuffer> = BTreeMap::new();
        let mut model = requested_model;
        let mut usage = None;
        let mut stop_reason = None;
        let mut done = false;

        while let Some(item) = stream.next().await {
            let event = item.map_err(|_| ProviderError::InvalidResponse)?;
            let data = Zeroizing::new(event.data);
            if data.trim() == "[DONE]" {
                done = true;
                break;
            }
            let chunk: StreamChunk =
                serde_json::from_str(&data).map_err(|_| ProviderError::InvalidResponse)?;
            if let Some(mut chunk_model) = chunk.model.filter(|value| !value.is_empty()) {
                self.wipe_and_redact(&mut chunk_model);
                model = chunk_model;
            }
            if let Some(wire_usage) = chunk.usage {
                let mapped = wire_usage.into_usage();
                send_event(&events, ProviderEvent::Usage(mapped)).await?;
                usage = Some(mapped);
            }
            for choice in chunk.choices {
                if let Some(mut content) = choice.delta.content {
                    pending_text.push_str(&content);
                    content.zeroize();
                    let content = self.drain_redacted(&mut pending_text, false);
                    text.push_str(&content);
                    if !content.is_empty() {
                        send_event(&events, ProviderEvent::TextDelta(content)).await?;
                    }
                }
                for tool in choice.delta.tool_calls {
                    let buffer = calls.entry(tool.index).or_default();
                    if let Some(id) = tool.id.as_ref() {
                        buffer.id = self.redact(id);
                    }
                    let mut name = None;
                    let mut arguments_delta = String::new();
                    if let Some(function) = tool.function {
                        if let Some(mut value) = function.name {
                            self.wipe_and_redact(&mut value);
                            buffer.name = value;
                            name = Some(buffer.name.clone());
                        }
                        if let Some(mut value) = function.arguments {
                            buffer.pending.push_str(&value);
                            value.zeroize();
                            arguments_delta = self.drain_redacted(&mut buffer.pending, false);
                            buffer.arguments.push_str(&arguments_delta);
                        }
                    }
                    send_event(
                        &events,
                        ProviderEvent::ToolCallDelta {
                            index: tool.index,
                            id: tool.id.map(|mut id| {
                                self.wipe_and_redact(&mut id);
                                id
                            }),
                            name,
                            arguments_delta,
                        },
                    )
                    .await?;
                }
                if let Some(reason) = choice.finish_reason {
                    stop_reason = Some(map_stop_reason(Some(reason.as_str())));
                }
            }
        }

        if !done {
            return Err(ProviderError::InvalidResponse);
        }
        let stop_reason = stop_reason.ok_or(ProviderError::InvalidResponse)?;
        let content = self.drain_redacted(&mut pending_text, true);
        text.push_str(&content);
        if !content.is_empty() {
            send_event(&events, ProviderEvent::TextDelta(content)).await?;
        }
        for (index, buffer) in &mut calls {
            let delta = self.drain_redacted(&mut buffer.pending, true);
            buffer.arguments.push_str(&delta);
            if !delta.is_empty() {
                send_event(
                    &events,
                    ProviderEvent::ToolCallDelta {
                        index: *index,
                        id: None,
                        name: None,
                        arguments_delta: delta,
                    },
                )
                .await?;
            }
        }
        let tool_calls = calls
            .into_values()
            .map(ToolCallBuffer::finish)
            .collect::<Result<Vec<_>, _>>()?;
        send_event(&events, ProviderEvent::Finished(stop_reason)).await?;
        Ok(ChatResponse {
            message: ChatMessage {
                role: ChatRole::Assistant,
                content: if text.is_empty() {
                    Vec::new()
                } else {
                    vec![ContentPart::Text(text)]
                },
                tool_calls,
                tool_call_id: None,
            },
            model,
            stop_reason,
            usage,
        })
    }
}

async fn send_event(
    sender: &mpsc::Sender<ProviderEvent>,
    event: ProviderEvent,
) -> Result<(), ProviderError> {
    sender
        .send(event)
        .await
        .map_err(|_| ProviderError::Cancelled)
}

fn retryable_status(status: StatusCode) -> bool {
    matches!(
        status,
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN | StatusCode::TOO_MANY_REQUESTS
    ) || status.is_server_error()
}

fn status_error(status: StatusCode) -> ProviderError {
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => ProviderError::Authentication,
        StatusCode::TOO_MANY_REQUESTS => ProviderError::RateLimited,
        status if status.is_server_error() => ProviderError::Unavailable,
        _ => ProviderError::InvalidResponse,
    }
}

fn map_stop_reason(reason: Option<&str>) -> StopReason {
    match reason {
        Some("stop") => StopReason::EndTurn,
        Some("tool_calls") | Some("function_call") => StopReason::ToolCalls,
        Some("length") => StopReason::Length,
        Some("content_filter") => StopReason::ContentFiltered,
        _ => StopReason::Unknown,
    }
}

#[derive(Serialize)]
struct WireRequest {
    model: String,
    messages: Vec<WireRequestMessage>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<WireToolDefinition>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream_options: Option<StreamOptions>,
}

impl WireRequest {
    fn from_request(request: ChatRequest, stream: bool) -> Self {
        Self {
            model: request.model,
            messages: request
                .messages
                .into_iter()
                .map(WireRequestMessage::from)
                .collect(),
            tools: request
                .tools
                .into_iter()
                .map(|tool| WireToolDefinition {
                    kind: "function",
                    function: WireFunctionDefinition {
                        name: tool.name,
                        description: tool.description,
                        parameters: tool.parameters,
                    },
                })
                .collect(),
            temperature: request.temperature,
            stream,
            stream_options: stream.then_some(StreamOptions {
                include_usage: true,
            }),
        }
    }
}

#[derive(Serialize)]
struct StreamOptions {
    include_usage: bool,
}

#[derive(Serialize)]
struct WireRequestMessage {
    role: &'static str,
    content: serde_json::Value,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tool_calls: Vec<WireRequestToolCall>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

impl From<ChatMessage> for WireRequestMessage {
    fn from(message: ChatMessage) -> Self {
        let content = if message.content.len() == 1 {
            match message
                .content
                .into_iter()
                .next()
                .expect("one content part")
            {
                ContentPart::Text(text) => serde_json::Value::String(text),
                ContentPart::ImageUrl(url) => serde_json::json!([{
                    "type": "image_url", "image_url": { "url": url, "detail": "high" }
                }]),
            }
        } else {
            serde_json::Value::Array(
                message
                    .content
                    .into_iter()
                    .map(|part| match part {
                        ContentPart::Text(text) => {
                            serde_json::json!({"type": "text", "text": text})
                        }
                        ContentPart::ImageUrl(url) => serde_json::json!({
                            "type": "image_url", "image_url": { "url": url, "detail": "high" }
                        }),
                    })
                    .collect(),
            )
        };
        Self {
            role: match message.role {
                ChatRole::System => "system",
                ChatRole::User => "user",
                ChatRole::Assistant => "assistant",
                ChatRole::Tool => "tool",
            },
            content,
            tool_calls: message
                .tool_calls
                .into_iter()
                .map(|call| WireRequestToolCall {
                    id: call.id,
                    kind: "function",
                    function: WireRequestFunctionCall {
                        name: call.name,
                        arguments: call.arguments,
                    },
                })
                .collect(),
            tool_call_id: message.tool_call_id,
        }
    }
}

#[derive(Serialize)]
struct WireToolDefinition {
    #[serde(rename = "type")]
    kind: &'static str,
    function: WireFunctionDefinition,
}

#[derive(Serialize)]
struct WireFunctionDefinition {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Serialize)]
struct WireRequestToolCall {
    id: String,
    #[serde(rename = "type")]
    kind: &'static str,
    function: WireRequestFunctionCall,
}

#[derive(Serialize)]
struct WireRequestFunctionCall {
    name: String,
    arguments: String,
}

#[derive(Deserialize)]
struct WireResponse {
    choices: Vec<WireChoice>,
    model: String,
    usage: Option<WireUsage>,
}

impl TryFrom<WireResponse> for ChatResponse {
    type Error = ProviderError;

    fn try_from(response: WireResponse) -> Result<Self, Self::Error> {
        let choice = response
            .choices
            .into_iter()
            .next()
            .ok_or(ProviderError::InvalidResponse)?;
        Ok(Self {
            message: choice.message.try_into()?,
            model: response.model,
            stop_reason: map_stop_reason(choice.finish_reason.as_deref()),
            usage: response.usage.map(WireUsage::into_usage),
        })
    }
}

#[derive(Deserialize)]
struct WireChoice {
    message: WireResponseMessage,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct WireResponseMessage {
    role: String,
    content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<WireResponseToolCall>,
}

impl TryFrom<WireResponseMessage> for ChatMessage {
    type Error = ProviderError;

    fn try_from(message: WireResponseMessage) -> Result<Self, Self::Error> {
        let role = match message.role.as_str() {
            "system" => ChatRole::System,
            "user" => ChatRole::User,
            "assistant" => ChatRole::Assistant,
            "tool" => ChatRole::Tool,
            _ => return Err(ProviderError::InvalidResponse),
        };
        Ok(Self {
            role,
            content: message.content.map(ContentPart::Text).into_iter().collect(),
            tool_calls: message
                .tool_calls
                .into_iter()
                .map(|call| ToolCall {
                    id: call.id,
                    name: call.function.name,
                    arguments: call.function.arguments,
                })
                .collect(),
            tool_call_id: None,
        })
    }
}

#[derive(Deserialize)]
struct WireResponseToolCall {
    id: String,
    function: WireResponseFunctionCall,
}

#[derive(Deserialize)]
struct WireResponseFunctionCall {
    name: String,
    arguments: String,
}

#[derive(Deserialize)]
struct WireUsage {
    #[serde(default)]
    prompt_tokens: u64,
    #[serde(default)]
    completion_tokens: u64,
}

impl WireUsage {
    fn into_usage(self) -> Usage {
        Usage {
            input_tokens: self.prompt_tokens,
            output_tokens: self.completion_tokens,
        }
    }
}

#[derive(Deserialize)]
struct StreamChunk {
    #[serde(default)]
    choices: Vec<StreamChoice>,
    model: Option<String>,
    usage: Option<WireUsage>,
}

#[derive(Deserialize)]
struct StreamChoice {
    delta: StreamDelta,
    finish_reason: Option<String>,
}

#[derive(Default, Deserialize)]
struct StreamDelta {
    content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<StreamToolCall>,
}

#[derive(Deserialize)]
struct StreamToolCall {
    index: usize,
    id: Option<String>,
    function: Option<StreamFunctionCall>,
}

#[derive(Deserialize)]
struct StreamFunctionCall {
    name: Option<String>,
    arguments: Option<String>,
}

#[derive(Default)]
struct ToolCallBuffer {
    id: String,
    name: String,
    arguments: String,
    pending: Zeroizing<String>,
}

impl ToolCallBuffer {
    fn finish(self) -> Result<ToolCall, ProviderError> {
        if self.id.is_empty() || self.name.is_empty() {
            return Err(ProviderError::InvalidResponse);
        }
        Ok(ToolCall {
            id: self.id,
            name: self.name,
            arguments: self.arguments,
        })
    }
}
