use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::application::conversations::model::ConversationUpdate;
use crate::application::conversations::{SessionApplicationError, SessionApplicationService};
use crate::transport::http::HttpState;
use crate::transport::http::auth::AuthenticatedActor;
use crate::transport::http::error::AppError;

pub fn router() -> Router<HttpState> {
    Router::new()
        .route("/api/chat/sessions", get(list_sessions))
        .route(
            "/api/chat/session/:session_id",
            get(get_session)
                .patch(update_session)
                .delete(delete_session),
        )
        .route("/api/chat/session/:session_id/messages", get(get_messages))
        .route("/api/chat/session/:session_id/clear", post(clear_session))
}

#[derive(Deserialize)]
struct SessionListQuery {
    q: Option<String>,
}

#[derive(Deserialize)]
struct UpdateSessionPayload {
    title: Option<String>,
    project_id: Option<Value>,
    order_col: Option<i64>,
}

fn transport_error(error: SessionApplicationError) -> AppError {
    match error {
        SessionApplicationError::NotFound => AppError::NotFound("会话不存在".into()),
        SessionApplicationError::Forbidden => AppError::Forbidden,
        SessionApplicationError::EmptyTitle => AppError::BadRequest("标题不能为空".into()),
        SessionApplicationError::Repository(error) => {
            AppError::Internal(anyhow::anyhow!(error.to_string()))
        }
    }
}

async fn list_sessions(
    State(service): State<Arc<SessionApplicationService>>,
    user: AuthenticatedActor,
    Query(query): Query<SessionListQuery>,
) -> Result<Json<Value>, AppError> {
    let sessions = service
        .list(&user.0, 50, query.q.as_deref())
        .await
        .map_err(transport_error)?;
    Ok(Json(json!({"sessions": sessions})))
}

async fn get_session(
    State(service): State<Arc<SessionApplicationService>>,
    user: AuthenticatedActor,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    Ok(Json(json!(
        service
            .detail(&user.0, &id)
            .await
            .map_err(transport_error)?
    )))
}

async fn get_messages(
    State(service): State<Arc<SessionApplicationService>>,
    user: AuthenticatedActor,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let messages = service
        .messages(&user.0, &id, 100)
        .await
        .map_err(transport_error)?;
    Ok(Json(json!({"messages": messages})))
}

async fn update_session(
    State(service): State<Arc<SessionApplicationService>>,
    user: AuthenticatedActor,
    Path(id): Path<String>,
    Json(payload): Json<UpdateSessionPayload>,
) -> Result<Json<Value>, AppError> {
    let project_id = match payload.project_id {
        Some(Value::Null) => Some(None),
        Some(Value::String(value)) => {
            Some(Some(value.trim().to_owned()).filter(|value| !value.is_empty()))
        }
        Some(_) => return Err(AppError::BadRequest("项目 ID 格式不正确".into())),
        None => None,
    };
    let updated = service
        .update(
            &user.0,
            &id,
            ConversationUpdate {
                title: payload.title,
                project_id,
                order: payload.order_col,
            },
        )
        .await
        .map_err(transport_error)?;
    Ok(Json(json!({"updated": updated})))
}

async fn delete_session(
    State(service): State<Arc<SessionApplicationService>>,
    user: AuthenticatedActor,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let deleted = service
        .delete(&user.0, &id)
        .await
        .map_err(transport_error)?;
    Ok(Json(json!({"deleted": deleted})))
}

async fn clear_session(
    State(service): State<Arc<SessionApplicationService>>,
    user: AuthenticatedActor,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let cleared = service.clear(&user.0, &id).await.map_err(transport_error)?;
    Ok(Json(json!({"cleared": cleared})))
}
