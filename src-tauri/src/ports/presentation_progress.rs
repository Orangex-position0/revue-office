use async_trait::async_trait;
use thiserror::Error;

use crate::contracts::presentation::PresentationProgress;

#[derive(Debug, Error)]
#[error("presentation progress consumer is unavailable")]
pub struct PresentationProgressError;

#[async_trait]
pub trait PresentationProgressSink: Send + Sync {
    async fn emit(&self, progress: PresentationProgress) -> Result<(), PresentationProgressError>;
}
