use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::agent_core::{AgentRequest, ToolContext};
use crate::providers::{ChatMessage, ChatRole, ContentPart};

use super::prompt::OFFICE_AGENT_PROMPT;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfficeMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfficeAttachment {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub mime_type: String,
    pub size: usize,
    pub text_content: Option<String>,
    pub data_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OfficeAgentRequest {
    pub run_id: String,
    pub session_id: String,
    pub user_id: String,
    pub project_id: Option<String>,
    pub preferred_model: Option<String>,
    pub attachments: Vec<OfficeAttachment>,
    pub tool_config: Option<Value>,
    pub allowed_tools: Option<Vec<String>>,
    pub history: Vec<OfficeMessage>,
    pub user_message: String,
    pub max_turns: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct OfficeToolMetadata {
    pub session_id: String,
    pub user_id: String,
    pub project_id: Option<String>,
    pub preferred_model: Option<String>,
    pub attachments: Vec<OfficeAttachment>,
    pub tool_config: Option<Value>,
}

pub(crate) fn build_agent_request(request: OfficeAgentRequest, model: String) -> AgentRequest {
    let mut messages = Vec::with_capacity(request.history.len().min(8) + 2);
    messages.push(ChatMessage::text(ChatRole::System, OFFICE_AGENT_PROMPT));
    messages.extend(
        request
            .history
            .into_iter()
            .rev()
            .take(8)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .map(history_message),
    );

    let mut user = ChatMessage::text(ChatRole::User, request.user_message);
    for attachment in request
        .attachments
        .iter()
        .filter(|item| item.kind == "image")
    {
        if let Some(url) = normalize_image_url(attachment) {
            user.content.push(ContentPart::ImageUrl(url));
        }
    }
    messages.push(user);

    let allowed_tools = if request.allowed_tools.is_none()
        && request.attachments.iter().any(|item| item.kind == "image")
    {
        Some(Vec::new())
    } else {
        request.allowed_tools
    };
    let metadata = OfficeToolMetadata {
        session_id: request.session_id,
        user_id: request.user_id,
        project_id: request.project_id,
        preferred_model: request.preferred_model,
        attachments: request.attachments,
        tool_config: request.tool_config,
    };

    AgentRequest {
        run_id: request.run_id.clone(),
        model,
        messages,
        max_turns: request.max_turns.max(1),
        allowed_tools,
        temperature: Some(0.7),
        tool_context: ToolContext::new(request.run_id)
            .with_metadata(serde_json::to_value(metadata).unwrap_or_else(|_| json!({}))),
    }
}

fn history_message(message: OfficeMessage) -> ChatMessage {
    let role = match message.role.as_str() {
        "system" => ChatRole::System,
        "assistant" => ChatRole::Assistant,
        "tool" => ChatRole::Tool,
        _ => ChatRole::User,
    };
    ChatMessage::text(role, message.content)
}

fn normalize_image_url(attachment: &OfficeAttachment) -> Option<String> {
    let value = attachment.data_url.as_deref()?.trim();
    if value.starts_with("data:") || value.starts_with("http://") || value.starts_with("https://") {
        return Some(value.to_owned());
    }
    let mime = if attachment.mime_type.trim().is_empty() {
        "image/png"
    } else {
        attachment.mime_type.trim()
    };
    Some(format!("data:{mime};base64,{value}"))
}
