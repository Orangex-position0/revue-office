use async_trait::async_trait;
use thiserror::Error;

use crate::contracts::presentation::PresentationProject;

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
}
