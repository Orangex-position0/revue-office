use thiserror::Error;

use crate::application::conversations::SessionRepositoryError;
use crate::capabilities::presentation::PresentationCapabilityError;

#[derive(Debug, Error)]
pub enum ProjectError {
    #[error("invalid project: {0}")]
    Invalid(String),
    #[error("project not found")]
    NotFound,
    #[error("project access is forbidden")]
    Forbidden,
    #[error("project repository is unavailable")]
    Repository(#[source] anyhow::Error),
    #[error(transparent)]
    Session(#[from] SessionRepositoryError),
    #[error(transparent)]
    Presentation(#[from] PresentationCapabilityError),
}
