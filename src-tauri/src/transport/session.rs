use axum::extract::{Path, Query};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::application::session_service::SessionApplicationError;
use crate::auth::middleware::AuthUser;
use crate::contracts::conversation::ConversationUpdate;
use crate::error::AppError;

pub fn router() -> Router {
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
    user: AuthUser,
    Query(query): Query<SessionListQuery>,
) -> Result<Json<Value>, AppError> {
    let sessions = crate::app::state::session_service()
        .list(&user.0.id, 50, query.q.as_deref())
        .await
        .map_err(transport_error)?;
    Ok(Json(json!({ "sessions": sessions })))
}

async fn get_session(
    user: AuthUser,
    Path(session_id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let session = crate::app::state::session_service()
        .detail(&user.0.id, &session_id)
        .await
        .map_err(transport_error)?;
    Ok(Json(json!(session)))
}

async fn get_messages(
    user: AuthUser,
    Path(session_id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let messages = crate::app::state::session_service()
        .messages(&user.0.id, &session_id, 100)
        .await
        .map_err(transport_error)?;
    Ok(Json(json!({ "messages": messages })))
}

async fn update_session(
    user: AuthUser,
    Path(session_id): Path<String>,
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
    let updated = crate::app::state::session_service()
        .update(
            &user.0.id,
            &session_id,
            ConversationUpdate {
                title: payload.title,
                project_id,
                order: payload.order_col,
            },
        )
        .await
        .map_err(transport_error)?;
    Ok(Json(json!({ "updated": updated })))
}

async fn delete_session(
    user: AuthUser,
    Path(session_id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let deleted = crate::app::state::session_service()
        .delete(&user.0.id, &session_id)
        .await
        .map_err(transport_error)?;
    Ok(Json(json!({ "deleted": deleted })))
}

async fn clear_session(
    user: AuthUser,
    Path(session_id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let cleared = crate::app::state::session_service()
        .clear(&user.0.id, &session_id)
        .await
        .map_err(transport_error)?;
    Ok(Json(json!({ "cleared": cleared })))
}
