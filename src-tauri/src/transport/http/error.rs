use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;
use thiserror::Error;

use crate::application::error::ChatApplicationError;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("未认证")]
    Unauthorized,
    #[error("无权访问")]
    Forbidden,
    #[error("未找到: {0}")]
    NotFound(String),
    #[error("参数错误: {0}")]
    BadRequest(String),
    #[error("LLM 调用失败: {0}")]
    Llm(String),
    #[error("工具执行失败: {0}")]
    Tool(String),
    #[error("内部错误")]
    Internal(#[source] anyhow::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, self.to_string()),
            Self::Forbidden => (StatusCode::FORBIDDEN, self.to_string()),
            Self::NotFound(_) => (StatusCode::NOT_FOUND, self.to_string()),
            Self::BadRequest(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            Self::Llm(_) => (StatusCode::BAD_GATEWAY, self.to_string()),
            Self::Tool(_) => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
            Self::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "内部错误".into()),
        };
        (status, Json(json!({ "detail": message }))).into_response()
    }
}

pub fn chat_start_error(error: ChatApplicationError) -> AppError {
    match error {
        ChatApplicationError::EmptyMessage => AppError::BadRequest("消息和附件不能同时为空".into()),
        ChatApplicationError::NotFound => AppError::NotFound("会话不存在".into()),
        ChatApplicationError::Forbidden => AppError::Forbidden,
        ChatApplicationError::Repository(error) => AppError::Internal(anyhow::Error::new(error)),
    }
}
