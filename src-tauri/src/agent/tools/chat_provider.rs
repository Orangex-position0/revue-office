use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::agent::tool::ToolAttachment;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct ToolChatMessage {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestMessage {
    pub role: String,
    pub content: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl RequestMessage {
    pub fn from_chat_message(msg: &ToolChatMessage) -> Self {
        Self {
            role: msg.role.clone(),
            content: Value::String(msg.content.clone()),
            tool_calls: msg.tool_calls.clone(),
            tool_call_id: msg.tool_call_id.clone(),
        }
    }

    pub fn from_multimodal_user_message(
        msg: &ToolChatMessage,
        attachments: &[ToolAttachment],
    ) -> Self {
        let mut content = Vec::new();

        if !msg.content.trim().is_empty() {
            content.push(json!({
                "type": "text",
                "text": msg.content,
            }));
        }

        for attachment in attachments.iter().filter(|item| {
            item.kind == "image"
                && item
                    .data_url
                    .as_deref()
                    .map(|value| !value.trim().is_empty())
                    .unwrap_or(false)
        }) {
            if let Some(image_url) = normalize_image_url(attachment) {
                content.push(json!({
                    "type": "image_url",
                    "image_url": {
                        "url": image_url,
                        "detail": "high",
                    }
                }));
            }
        }

        if content.is_empty() {
            return Self::from_chat_message(msg);
        }

        Self {
            role: msg.role.clone(),
            content: Value::Array(content),
            tool_calls: msg.tool_calls.clone(),
            tool_call_id: msg.tool_call_id.clone(),
        }
    }
}

fn normalize_image_url(attachment: &ToolAttachment) -> Option<String> {
    let value = attachment.data_url.as_deref()?.trim();
    if value.is_empty() {
        return None;
    }
    if value.starts_with("data:") || value.starts_with("http://") || value.starts_with("https://") {
        return Some(value.to_string());
    }

    let mime = if attachment.mime_type.trim().is_empty() {
        "image/png"
    } else {
        attachment.mime_type.trim()
    };
    Some(format!("data:{mime};base64,{value}"))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDef {
    #[serde(rename = "type")]
    pub def_type: String,
    pub function: FunctionSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionSpec {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionResponse {
    pub choices: Vec<Choice>,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Choice {
    pub message: ResponseMessage,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseMessage {
    pub role: String,
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: ToolFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolFunction {
    pub name: String,
    pub arguments: String,
}

// Private Tool helper. All model I/O uses the injected neutral Provider
// contract; no credential, config, database, or HTTP dependency.
use crate::agent::tool::ToolContext;
use crate::providers::{self, ContentPart, ResolvedChatProvider, StopReason};
use anyhow::{Result, anyhow};

#[cfg(test)]
mod credential_tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::{Arc, Mutex};
    struct FakeProvider(Mutex<Vec<providers::ChatRequest>>);
    #[async_trait]
    impl providers::ChatProvider for FakeProvider {
        async fn chat(
            &self,
            request: providers::ChatRequest,
        ) -> Result<providers::ChatResponse, providers::ProviderError> {
            self.0.lock().unwrap().push(request);
            Ok(providers::ChatResponse {
                message: providers::ChatMessage::text(
                    providers::ChatRole::Assistant,
                    "synthetic response",
                ),
                model: "model".into(),
                stop_reason: StopReason::EndTurn,
                usage: None,
            })
        }
        async fn stream_chat(
            &self,
            _: providers::ChatRequest,
            _: tokio::sync::mpsc::Sender<providers::ProviderEvent>,
        ) -> Result<providers::ChatResponse, providers::ProviderError> {
            Err(providers::ProviderError::Unavailable)
        }
    }
    struct FakeResolver(Arc<FakeProvider>);
    #[async_trait]
    impl providers::ChatProviderResolver for FakeResolver {
        async fn resolve(
            &self,
            _: &str,
            _: Option<&str>,
        ) -> Result<ResolvedChatProvider, providers::ProviderError> {
            Ok(ResolvedChatProvider {
                provider: self.0.clone(),
                model: "selected-model".into(),
            })
        }
    }
    #[tokio::test]
    async fn credential_legacy_tool_facade_uses_injected_provider_and_latest_image_attachment() {
        let fake = Arc::new(FakeProvider(Mutex::new(vec![])));
        let context = ToolContext::new(
            "session".into(),
            "actor".into(),
            None,
            None,
            vec![],
            |_, _| {},
        );
        assert!(ToolChatClient::for_context(&context).await.is_err()); // No global/Config fallback.
        let context = context.with_provider_resolver(Arc::new(FakeResolver(fake.clone())));
        let client = ToolChatClient::for_context(&context).await.unwrap();
        let message = |text: &str| ToolChatMessage {
            role: "user".into(),
            content: text.into(),
            tool_calls: None,
            tool_call_id: None,
        };
        let attachment = ToolAttachment {
            id: "image".into(),
            name: "Synthetic".into(),
            kind: "image".into(),
            mime_type: "image/png".into(),
            size: 4,
            text_content: None,
            data_url: Some("aGVsbG8=".into()),
        };
        let response = client
            .chat_with_attachments(
                &[message("history"), message("latest")],
                None,
                Some(&[attachment]),
            )
            .await
            .unwrap();
        assert!(response.choices[0].message.content.as_deref() == Some("synthetic response"));
        let requests = fake.0.lock().unwrap();
        assert!(requests[0].model == "selected-model");
        assert!(requests[0].messages[0].content.len() == 1);
        assert!(requests[0].messages[1].content.iter().any(
            |p| matches!(p, ContentPart::ImageUrl(url) if url == "data:image/png;base64,aGVsbG8=")
        ));
    }
}

pub(super) struct ToolChatClient {
    resolved: ResolvedChatProvider,
}
impl ToolChatClient {
    pub async fn for_context(ctx: &ToolContext) -> Result<Self> {
        let resolver = ctx
            .provider_resolver()
            .ok_or_else(|| anyhow!("provider unavailable"))?;
        let resolved = resolver
            .resolve(&ctx.user_id, ctx.preferred_model.as_deref())
            .await?;
        Ok(Self { resolved })
    }
    pub async fn chat(
        &self,
        messages: &[ToolChatMessage],
        tools: Option<&[FunctionDef]>,
    ) -> Result<ChatCompletionResponse> {
        self.chat_with_attachments(messages, tools, None).await
    }
    pub async fn chat_with_attachments(
        &self,
        messages: &[ToolChatMessage],
        tools: Option<&[FunctionDef]>,
        attachments: Option<&[ToolAttachment]>,
    ) -> Result<ChatCompletionResponse> {
        let image = attachments.is_some_and(|a| {
            a.iter().any(|a| {
                a.kind == "image" && a.data_url.as_deref().is_some_and(|s| !s.trim().is_empty())
            })
        });
        match self.send(messages, tools, attachments).await {
            Ok(result) => Ok(result),
            Err(_) if image && tools.is_some() => {
                match self.send(messages, None, attachments).await {
                    Ok(result) => Ok(result),
                    Err(_) => self.send(messages, tools, None).await,
                }
            }
            Err(_) if image => self.send(messages, tools, None).await,
            Err(error) => Err(error),
        }
    }
    async fn send(
        &self,
        messages: &[ToolChatMessage],
        tools: Option<&[FunctionDef]>,
        attachments: Option<&[ToolAttachment]>,
    ) -> Result<ChatCompletionResponse> {
        let latest_user = messages.iter().rposition(|m| m.role == "user");
        let messages = messages
            .iter()
            .enumerate()
            .map(|(index, message)| {
                let wire = if Some(index) == latest_user {
                    RequestMessage::from_multimodal_user_message(
                        message,
                        attachments.unwrap_or_default(),
                    )
                } else {
                    RequestMessage::from_chat_message(message)
                };
                let role = match wire.role.as_str() {
                    "system" => providers::ChatRole::System,
                    "assistant" => providers::ChatRole::Assistant,
                    "tool" => providers::ChatRole::Tool,
                    _ => providers::ChatRole::User,
                };
                let content = match wire.content {
                    serde_json::Value::String(text) => vec![ContentPart::Text(text)],
                    serde_json::Value::Array(values) => values
                        .into_iter()
                        .filter_map(|value| match value["type"].as_str() {
                            Some("text") => {
                                value["text"].as_str().map(|s| ContentPart::Text(s.into()))
                            }
                            Some("image_url") => value["image_url"]["url"]
                                .as_str()
                                .map(|s| ContentPart::ImageUrl(s.into())),
                            _ => None,
                        })
                        .collect(),
                    _ => vec![],
                };
                let tool_calls = wire
                    .tool_calls
                    .unwrap_or_default()
                    .into_iter()
                    .map(|value| {
                        let call: ToolCall = serde_json::from_value(value)
                            .map_err(|_| anyhow!("invalid tool call"))?;
                        Ok(providers::ToolCall {
                            id: call.id,
                            name: call.function.name,
                            arguments: call.function.arguments,
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                Ok(providers::ChatMessage {
                    role,
                    content,
                    tool_calls,
                    tool_call_id: wire.tool_call_id,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let tools = tools
            .unwrap_or_default()
            .iter()
            .map(|tool| providers::ToolDefinition {
                name: tool.function.name.clone(),
                description: tool.function.description.clone(),
                parameters: tool.function.parameters.clone(),
            })
            .collect();
        let response = self
            .resolved
            .provider
            .chat(providers::ChatRequest {
                model: self.resolved.model.clone(),
                messages,
                tools,
                temperature: Some(0.7),
            })
            .await?;
        let content = response.message.text_content();
        let tool_calls: Vec<_> = response
            .message
            .tool_calls
            .into_iter()
            .map(|call| ToolCall {
                id: call.id,
                call_type: "function".into(),
                function: ToolFunction {
                    name: call.name,
                    arguments: call.arguments,
                },
            })
            .collect();
        let finish_reason = match response.stop_reason {
            StopReason::EndTurn => "stop",
            StopReason::ToolCalls => "tool_calls",
            StopReason::Length => "length",
            StopReason::ContentFiltered => "content_filter",
            StopReason::Unknown => "unknown",
        };
        Ok(ChatCompletionResponse {
            model: response.model,
            choices: vec![Choice {
                message: ResponseMessage {
                    role: "assistant".into(),
                    content: Some(content),
                    tool_calls: (!tool_calls.is_empty()).then_some(tool_calls),
                },
                finish_reason: Some(finish_reason.into()),
            }],
        })
    }
    pub fn extract_json(text: &str) -> Result<serde_json::Value> {
        let cleaned = text
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();
        if let Ok(value) = serde_json::from_str(cleaned) {
            return Ok(value);
        }
        for (start, end) in [
            (cleaned.find('{'), cleaned.rfind('}')),
            (cleaned.find('['), cleaned.rfind(']')),
        ] {
            if let (Some(start), Some(end)) = (start, end) {
                if end > start {
                    if let Ok(value) = serde_json::from_str(&cleaned[start..=end]) {
                        return Ok(value);
                    }
                }
            }
        }
        Err(anyhow!("模型未返回可解析 JSON"))
    }
}
