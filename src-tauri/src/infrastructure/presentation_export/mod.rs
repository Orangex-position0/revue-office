use async_trait::async_trait;

use crate::contracts::presentation::PresentationProject;
use crate::models::PptProject;
use crate::ports::presentation_export::{PresentationExportError, PresentationExporter};

pub struct PptxPresentationExporter;

#[async_trait]
impl PresentationExporter for PptxPresentationExporter {
    async fn export_pptx(
        &self,
        project: &PresentationProject,
    ) -> Result<Vec<u8>, PresentationExportError> {
        let project: PptProject = serde_json::from_value(
            serde_json::to_value(project)
                .map_err(|error| PresentationExportError::Failed(anyhow::Error::new(error)))?,
        )
        .map_err(|error| PresentationExportError::Failed(anyhow::Error::new(error)))?;
        let path = std::env::temp_dir().join(format!(
            "revue-presentation-export-{}.pptx",
            uuid::Uuid::new_v4()
        ));
        crate::render::pptx_render::render_pptx(&project, &path)
            .map_err(PresentationExportError::Failed)?;
        let bytes = tokio::fs::read(&path)
            .await
            .map_err(|error| PresentationExportError::Failed(anyhow::Error::new(error)));
        let _ = tokio::fs::remove_file(path).await;
        bytes
    }
}
