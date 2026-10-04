use std::sync::Arc;

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::Value;

use crate::application::preferences::{
    AppSettings, McpConnectionRequest, McpConnectionResult, PreferenceApplicationService,
    PreferenceError, PreferenceUpdate,
};
use crate::transport::http::HttpState;
use crate::transport::http::auth::AuthenticatedActor;
use crate::transport::http::error::AppError;

pub fn router() -> Router<HttpState> {
    Router::new()
        .route("/api/settings", get(get_settings).put(save_settings))
        .route("/api/settings/mcp/test", post(test_mcp_service))
}

fn map_error(error: PreferenceError) -> AppError {
    match error {
        PreferenceError::Invalid(message) => AppError::BadRequest(message),
        PreferenceError::Forbidden => AppError::Forbidden,
        PreferenceError::Conflict => {
            AppError::BadRequest("设置已被其他请求更新，请重新读取后再试".into())
        }
        PreferenceError::Unavailable => AppError::BadRequest(
            "安全凭据存储或迁移暂不可用；请解锁/配置安全存储后重试，旧凭据不会被导出".into(),
        ),
    }
}

async fn get_settings(
    State(service): State<Arc<PreferenceApplicationService>>,
    AuthenticatedActor(actor): AuthenticatedActor,
) -> Result<Json<AppSettings>, AppError> {
    service.get(&actor).await.map(Json).map_err(map_error)
}

async fn save_settings(
    State(service): State<Arc<PreferenceApplicationService>>,
    AuthenticatedActor(actor): AuthenticatedActor,
    Json(payload): Json<Value>,
) -> Result<Json<AppSettings>, AppError> {
    service
        .save(&actor, PreferenceUpdate::new(payload))
        .await
        .map(Json)
        .map_err(map_error)
}

async fn test_mcp_service(
    State(service): State<Arc<PreferenceApplicationService>>,
    AuthenticatedActor(actor): AuthenticatedActor,
    Json(payload): Json<Value>,
) -> Result<Json<McpConnectionResult>, AppError> {
    let request = McpConnectionRequest::from_value(payload).map_err(map_error)?;
    service
        .test_mcp(&actor, request)
        .await
        .map(Json)
        .map_err(map_error)
}
