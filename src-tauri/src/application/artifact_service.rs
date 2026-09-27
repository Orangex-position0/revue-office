use std::sync::Arc;

use thiserror::Error;

use crate::contracts::artifact::{
    ArtifactDraft, ArtifactFinalization, ArtifactPublication, NewArtifactPublication,
};
use crate::ports::file_storage::{FileStorage, FileStorageError};
use crate::ports::repositories::artifact_publication::{
    ArtifactPublicationRepository, ArtifactPublicationRepositoryError,
};

#[derive(Debug, Error)]
pub enum ArtifactServiceError {
    #[error(transparent)]
    Repository(#[from] ArtifactPublicationRepositoryError),
    #[error(transparent)]
    Storage(#[from] FileStorageError),
    #[error("artifact publication could not be finalized")]
    FinalizationConflict,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ArtifactReconciliationReport {
    pub recovered: usize,
    pub failed: usize,
}

pub struct ArtifactService {
    repository: Arc<dyn ArtifactPublicationRepository>,
    storage: Arc<dyn FileStorage>,
}

impl ArtifactService {
    pub fn new(
        repository: Arc<dyn ArtifactPublicationRepository>,
        storage: Arc<dyn FileStorage>,
    ) -> Self {
        Self {
            repository,
            storage,
        }
    }

    pub async fn publish(
        &self,
        draft: ArtifactDraft,
    ) -> Result<ArtifactPublication, ArtifactServiceError> {
        let artifact_id = uuid::Uuid::new_v4().to_string();
        let staged_file = self.storage.staging_file(&artifact_id, &draft.extension)?;
        let publication = self
            .repository
            .reserve(NewArtifactPublication {
                id: artifact_id,
                session_id: draft.session_id,
                owner_id: draft.owner_id,
                kind: draft.kind,
                title: draft.title,
                content: draft.content.clone(),
                staging_path: staged_file.path.clone(),
            })
            .await?;

        if let Err(error) = self.storage.write_staging(&staged_file, &draft.bytes).await {
            let _ = self.storage.delete(&staged_file.path).await;
            let _ = self
                .repository
                .fail(&publication.id, &error.to_string())
                .await;
            return Err(error.into());
        }
        if let Err(error) = self.storage.validate(&staged_file).await {
            let _ = self.storage.delete(&staged_file.path).await;
            let _ = self
                .repository
                .fail(&publication.id, &error.to_string())
                .await;
            return Err(error.into());
        }
        let ready_file = match self.storage.promote(&staged_file).await {
            Ok(file) => file,
            Err(error) => {
                let _ = self.storage.delete(&staged_file.path).await;
                let _ = self
                    .repository
                    .fail(&publication.id, &error.to_string())
                    .await;
                return Err(error.into());
            }
        };
        let content = with_file_metadata(draft.content, &ready_file.path, ready_file.size);
        match self
            .repository
            .finalize(
                &publication.id,
                ArtifactFinalization {
                    final_path: ready_file.path.clone(),
                    content,
                },
            )
            .await
        {
            Ok(Some(ready)) => Ok(ready),
            Ok(None) => {
                let _ = self.storage.delete(&ready_file.path).await;
                let _ = self
                    .repository
                    .fail(&publication.id, "artifact finalization conflict")
                    .await;
                Err(ArtifactServiceError::FinalizationConflict)
            }
            Err(error) => {
                let _ = self.storage.delete(&ready_file.path).await;
                let _ = self
                    .repository
                    .fail(&publication.id, &error.to_string())
                    .await;
                Err(error.into())
            }
        }
    }

    pub async fn reconcile_pending(
        &self,
    ) -> Result<ArtifactReconciliationReport, ArtifactServiceError> {
        let pending = self.repository.pending().await?;
        let mut report = ArtifactReconciliationReport::default();
        for publication in pending {
            match self
                .storage
                .recover_staging(
                    publication
                        .staging_path
                        .as_deref()
                        .ok_or(ArtifactServiceError::FinalizationConflict)?,
                )
                .await
            {
                Ok(Some(file)) => {
                    let content = with_file_metadata(publication.content, &file.path, file.size);
                    match self
                        .repository
                        .finalize(
                            &publication.id,
                            ArtifactFinalization {
                                final_path: file.path.clone(),
                                content,
                            },
                        )
                        .await?
                    {
                        Some(_) => report.recovered += 1,
                        None => {
                            let _ = self.storage.delete(&file.path).await;
                            return Err(ArtifactServiceError::FinalizationConflict);
                        }
                    }
                }
                Ok(None) => {
                    self.repository
                        .fail(
                            &publication.id,
                            "staging and final artifact files are missing",
                        )
                        .await?;
                    report.failed += 1;
                }
                Err(error) => {
                    if let Some(path) = &publication.staging_path {
                        let _ = self.storage.delete(path).await;
                    }
                    self.repository
                        .fail(&publication.id, &error.to_string())
                        .await?;
                    report.failed += 1;
                }
            }
        }
        Ok(report)
    }
}

fn with_file_metadata(
    content: serde_json::Value,
    file_path: &str,
    file_size: u64,
) -> serde_json::Value {
    match content {
        serde_json::Value::Object(mut object) => {
            object.insert("file_path".into(), file_path.into());
            object.insert("file_size".into(), file_size.into());
            serde_json::Value::Object(object)
        }
        value => serde_json::json!({
            "value": value,
            "file_path": file_path,
            "file_size": file_size,
        }),
    }
}
