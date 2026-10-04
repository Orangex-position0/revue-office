use thiserror::Error;

use crate::application::conversations::SessionRepositoryError;

#[derive(Debug, Error)]
pub enum ChatApplicationError {
    #[error("chat message must not be empty")]
    EmptyMessage,
    #[error("conversation not found")]
    NotFound,
    #[error("conversation access is forbidden")]
    Forbidden,
    #[error(transparent)]
    Repository(#[from] SessionRepositoryError),
}
