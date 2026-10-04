use async_trait::async_trait;
use thiserror::Error;

use super::model::PresentationProject;

#[derive(Debug, Clone, PartialEq)]
pub enum PresentationProgress {
    Planning,
    ProjectCreated {
        project: PresentationProject,
    },
    SlideGenerated {
        project: PresentationProject,
        current_index: usize,
        total_slides: usize,
    },
    GenerationCompleted {
        project: PresentationProject,
    },
}

#[derive(Debug, Error)]
#[error("presentation progress consumer is unavailable")]
pub struct PresentationProgressError;

#[async_trait]
pub trait PresentationProgressSink: Send + Sync {
    async fn emit(&self, progress: PresentationProgress) -> Result<(), PresentationProgressError>;
}
