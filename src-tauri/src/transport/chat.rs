use std::convert::Infallible;
use std::time::Duration;

use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::post;
use axum::{Json, Router};
use futures::{Stream, StreamExt};
use tokio_stream::wrappers::ReceiverStream;

use super::error::chat_start_error;
use super::sse::application_event_frame;
use crate::application::chat_service::ChatCommand;
use crate::auth::middleware::AuthUser;
use crate::contracts::agent_run::RuntimeAttachment;
use crate::error::AppError;
use crate::models::ChatRequest;

pub fn router() -> Router {
    Router::new().route("/api/chat/stream", post(chat_stream))
}

async fn chat_stream(
    user: AuthUser,
    Json(request): Json<ChatRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, AppError> {
    let message = request_message(&request);
    let runtime_message = runtime_message(&message, request.attachments.as_deref());
    let attachments = request
        .attachments
        .unwrap_or_default()
        .into_iter()
        .map(|attachment| RuntimeAttachment {
            id: attachment.id,
            name: attachment.name,
            kind: attachment.kind,
            mime_type: attachment.mime_type,
            size: attachment.size,
            text_content: attachment.text_content,
            data_url: attachment.data_url,
        })
        .collect();
    let allowed_tools = match request.tool_kind.as_deref() {
        Some("image") => Some(vec!["image_prompt".into()]),
        Some("video") => Some(vec!["video_generate".into()]),
        _ => None,
    };
    let run = crate::app::state::chat_service()
        .start_chat(ChatCommand {
            owner_id: user.0.id,
            session_id: request.session_id,
            project_id: request.project_id,
            message,
            runtime_message: Some(runtime_message),
            preferred_model: request.model,
            attachments,
            tool_config: request.tool_config,
            allowed_tools,
            max_turns: 6,
        })
        .await
        .map_err(chat_start_error)?;
    let session_id = run.session_id;
    let stream = ReceiverStream::new(run.events)
        .map(move |event| Ok(application_event_frame(&session_id, event).into_event()));

    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15))))
}

fn runtime_message(message: &str, attachments: Option<&[crate::models::ChatAttachment]>) -> String {
    let text_context = attachments
        .unwrap_or_default()
        .iter()
        .filter_map(|attachment| {
            attachment.text_content.as_deref().map(|content| {
                format!(
                    "附件：{}\n{}",
                    attachment.name,
                    content.chars().take(12_000).collect::<String>()
                )
            })
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    if text_context.is_empty() {
        message.to_owned()
    } else {
        format!("{message}\n\n{text_context}")
    }
}

fn request_message(request: &ChatRequest) -> String {
    if !request.message.trim().is_empty() {
        return request.message.clone();
    }
    request
        .attachments
        .as_deref()
        .filter(|attachments| !attachments.is_empty())
        .map(|attachments| {
            let names = attachments
                .iter()
                .map(|attachment| attachment.name.as_str())
                .collect::<Vec<_>>()
                .join("、");
            format!("请处理附件：{names}")
        })
        .unwrap_or_default()
}
