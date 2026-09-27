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
    let run = crate::app::state::chat_service()
        .start_chat(ChatCommand {
            owner_id: user.0.id,
            session_id: request.session_id,
            project_id: request.project_id,
            message,
            max_turns: 6,
        })
        .await
        .map_err(chat_start_error)?;
    let session_id = run.session_id;
    let stream = ReceiverStream::new(run.events)
        .map(move |event| Ok(application_event_frame(&session_id, event).into_event()));

    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15))))
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
