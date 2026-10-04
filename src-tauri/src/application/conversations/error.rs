use thiserror::Error;

use super::ports::SessionRepositoryError;

#[derive(Debug, Error)]
pub enum SessionApplicationError {
    #[error("conversation not found")]
    NotFound,
    #[error("conversation access is forbidden")]
    Forbidden,
    #[error("conversation title must not be empty")]
    EmptyTitle,
    #[error(transparent)]
    Repository(#[from] SessionRepositoryError),
}
