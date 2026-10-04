use thiserror::Error;

use super::ports::{PresentationExportError, PresentationPlannerError, PresentationStoreError};
use super::progress::PresentationProgressError;

#[derive(Debug, Error)]
pub enum PresentationCapabilityError {
    #[error(transparent)]
    Planner(#[from] PresentationPlannerError),
    #[error(transparent)]
    Store(#[from] PresentationStoreError),
    #[error(transparent)]
    Export(#[from] PresentationExportError),
    #[error(transparent)]
    Progress(#[from] PresentationProgressError),
    #[error("presentation topic cannot be empty")]
    EmptyTopic,
    #[error("presentation plan contains no slides")]
    EmptyPlan,
}
