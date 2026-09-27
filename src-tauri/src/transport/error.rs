use crate::application::error::ChatApplicationError;
use crate::error::AppError;

pub fn chat_start_error(error: ChatApplicationError) -> AppError {
    match error {
        ChatApplicationError::EmptyMessage => AppError::BadRequest("消息和附件不能同时为空".into()),
        ChatApplicationError::NotFound => AppError::NotFound("会话不存在".into()),
        ChatApplicationError::Forbidden => AppError::Forbidden,
        ChatApplicationError::Repository(error) => AppError::Internal(anyhow::Error::new(error)),
    }
}
