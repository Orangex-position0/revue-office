use async_trait::async_trait;
use thiserror::Error;

use crate::contracts::presentation::{PresentationPlan, PresentationPlanRequest};

#[derive(Debug, Error)]
pub enum PresentationLlmError {
    #[error("presentation planning failed")]
    Unavailable(#[source] anyhow::Error),
    #[error("presentation plan is invalid: {0}")]
    InvalidResponse(String),
}

#[async_trait]
pub trait PresentationLlm: Send + Sync {
    async fn plan(
        &self,
        request: PresentationPlanRequest,
    ) -> Result<PresentationPlan, PresentationLlmError>;
}
