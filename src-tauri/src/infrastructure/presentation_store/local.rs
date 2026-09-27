use async_trait::async_trait;

use crate::contracts::presentation::PresentationProject;
use crate::db::project_repo;
use crate::models::PptProject;
use crate::ports::presentation_store::{PresentationStore, PresentationStoreError};

pub struct LocalPresentationStore;

#[async_trait]
impl PresentationStore for LocalPresentationStore {
    async fn save(&self, project: &PresentationProject) -> Result<(), PresentationStoreError> {
        let project: PptProject =
            serde_json::from_value(serde_json::to_value(project).map_err(map_error)?)
                .map_err(map_error)?;
        project_repo::save_ppt_project(&project)
            .map_err(|error| PresentationStoreError::Unavailable(error.into()))
    }

    async fn load(
        &self,
        project_id: &str,
    ) -> Result<Option<PresentationProject>, PresentationStoreError> {
        project_repo::load_ppt_project(project_id)
            .map_err(|error| PresentationStoreError::Unavailable(error.into()))?
            .map(|project| {
                serde_json::from_value(serde_json::to_value(project).map_err(map_error)?)
                    .map_err(map_error)
            })
            .transpose()
    }
}

fn map_error(error: serde_json::Error) -> PresentationStoreError {
    PresentationStoreError::Unavailable(anyhow::Error::new(error))
}
