use async_trait::async_trait;
use thiserror::Error;

use super::model::{
    PresentationExport, PresentationPlan, PresentationPlanRequest, PresentationProject,
};

#[derive(Debug, Error)]
pub enum PresentationPlannerError {
    #[error("presentation planning is unavailable")]
    Unavailable(#[source] anyhow::Error),
    #[error("presentation plan is invalid: {0}")]
    InvalidResponse(String),
}

#[async_trait]
pub trait PresentationPlanner: Send + Sync {
    async fn plan(
        &self,
        request: PresentationPlanRequest,
    ) -> Result<PresentationPlan, PresentationPlannerError>;
}

#[derive(Debug, Error)]
pub enum PresentationStoreError {
    #[error("presentation store is unavailable")]
    Unavailable(#[source] anyhow::Error),
}

#[async_trait]
pub trait PresentationStore: Send + Sync {
    async fn save(&self, project: &PresentationProject) -> Result<(), PresentationStoreError>;

    async fn load(
        &self,
        project_id: &str,
    ) -> Result<Option<PresentationProject>, PresentationStoreError>;

    async fn list(
        &self,
        _owner_id: &str,
    ) -> Result<Vec<PresentationProject>, PresentationStoreError> {
        Ok(vec![])
    }

    async fn delete(
        &self,
        _owner_id: &str,
        _project_id: &str,
    ) -> Result<bool, PresentationStoreError> {
        Ok(false)
    }
}

#[derive(Debug, Error)]
pub enum PresentationExportError {
    #[error("presentation export failed")]
    Failed(#[source] anyhow::Error),
}

#[async_trait]
pub trait PresentationExporter: Send + Sync {
    async fn export(
        &self,
        project: &PresentationProject,
    ) -> Result<PresentationExport, PresentationExportError>;
}
