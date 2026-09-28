use async_trait::async_trait;
use thiserror::Error;

use crate::contracts::presentation::PresentationProject;

#[derive(Debug, Error)]
pub enum PresentationExportError {
    #[error("presentation export failed")]
    Failed(#[source] anyhow::Error),
}

#[async_trait]
pub trait PresentationExporter: Send + Sync {
    async fn export_pptx(
        &self,
        project: &PresentationProject,
    ) -> Result<Vec<u8>, PresentationExportError>;
}
