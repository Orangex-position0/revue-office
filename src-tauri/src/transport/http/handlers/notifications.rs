use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;

use crate::application::notifications::{NotificationApplicationService, NotificationError};
use crate::transport::http::HttpState;
use crate::transport::http::auth::AuthenticatedActor;
use crate::transport::http::error::AppError;

pub fn router() -> Router<HttpState> {
    Router::new()
        .route("/api/notifications", get(list_notifications))
        .route("/api/notifications/unread", get(unread_count))
        .route("/api/notifications/:id/read", post(mark_as_read))
        .route("/api/notifications/read-all", post(mark_all_as_read))
        .route("/api/notifications/:id", delete(delete_notification))
}

fn map_error(error: NotificationError) -> AppError {
    AppError::Internal(anyhow::Error::new(error))
}

#[derive(Deserialize)]
struct NotifQuery {
    #[serde(default)]
    unread_only: Option<bool>,
    #[serde(default)]
    page: Option<u32>,
    #[serde(default)]
    page_size: Option<u32>,
}

async fn list_notifications(
    State(service): State<Arc<NotificationApplicationService>>,
    AuthenticatedActor(actor): AuthenticatedActor,
    Query(query): Query<NotifQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let notifications = service
        .list(
            &actor,
            query.unread_only.unwrap_or(false),
            query.page,
            query.page_size,
        )
        .await
        .map_err(map_error)?;
    Ok(Json(json!({ "notifications": notifications })))
}

async fn unread_count(
    State(service): State<Arc<NotificationApplicationService>>,
    AuthenticatedActor(actor): AuthenticatedActor,
) -> Result<Json<serde_json::Value>, AppError> {
    let count = service.unread_count(&actor).await.map_err(map_error)?;
    Ok(Json(json!({ "count": count })))
}

async fn mark_as_read(
    State(service): State<Arc<NotificationApplicationService>>,
    AuthenticatedActor(actor): AuthenticatedActor,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let ok = service.mark_read(&actor, id).await.map_err(map_error)?;
    Ok(Json(json!({ "ok": ok })))
}

async fn mark_all_as_read(
    State(service): State<Arc<NotificationApplicationService>>,
    AuthenticatedActor(actor): AuthenticatedActor,
) -> Result<Json<serde_json::Value>, AppError> {
    service.mark_all_read(&actor).await.map_err(map_error)?;
    Ok(Json(json!({ "ok": true })))
}

async fn delete_notification(
    State(service): State<Arc<NotificationApplicationService>>,
    AuthenticatedActor(actor): AuthenticatedActor,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let deleted = service.delete(&actor, id).await.map_err(map_error)?;
    Ok(Json(json!({ "deleted": deleted })))
}
