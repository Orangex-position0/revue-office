use axum::routing::get;
use axum::{Json, Router};
use serde_json::json;

use crate::transport::http::HttpState;

#[derive(Clone)]
pub struct HealthInfo {
    pub app_name: String,
    pub llm_model: String,
    pub llm_provider: String,
}

pub fn router(info: HealthInfo) -> Router<HttpState> {
    Router::new().route(
        "/api/health",
        get(move || {
            let info = info.clone();
            async move {
                Json(json!({
                    "status": "ok",
                    "app": info.app_name,
                    "version": "0.2.0",
                    "llm_model": info.llm_model,
                    "llm_provider": info.llm_provider,
                }))
            }
        }),
    )
}
