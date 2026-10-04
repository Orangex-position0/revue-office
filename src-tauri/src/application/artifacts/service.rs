use std::sync::Arc;

use thiserror::Error;

use super::{
    ArtifactDraft, ArtifactFinalization, ArtifactPublication, ArtifactPublicationRepository,
    ArtifactPublicationRepositoryError, FileStorage, FileStorageError, NewArtifactPublication,
    ReadyArtifactFile,
};

#[derive(Debug, Error)]
pub enum ArtifactServiceError {
    #[error(transparent)]
    Repository(#[from] ArtifactPublicationRepositoryError),
    #[error(transparent)]
    Storage(#[from] FileStorageError),
    #[error("artifact publication could not be finalized")]
    FinalizationConflict,
    #[error("artifact repository returned a non-ready publication")]
    InvalidReadyPublication,
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
            self.compensate_staging(&publication.id, &staged_file.path, &error.to_string())
                .await;
            return Err(error.into());
        }
        if let Err(error) = self.storage.validate(&staged_file).await {
            self.compensate_staging(&publication.id, &staged_file.path, &error.to_string())
                .await;
            return Err(error.into());
        }
        let ready_file = match self.storage.promote(&staged_file).await {
            Ok(file) => file,
            Err(error) => {
                self.compensate_staging(&publication.id, &staged_file.path, &error.to_string())
                    .await;
                return Err(error.into());
            }
        };
        self.finalize(publication, draft.content, ready_file).await
    }

    async fn finalize(
        &self,
        publication: ArtifactPublication,
        content: serde_json::Value,
        ready_file: ReadyArtifactFile,
    ) -> Result<ArtifactPublication, ArtifactServiceError> {
        let content = with_file_metadata(content, &ready_file.path, ready_file.size);
        let finalized = self
            .repository
            .finalize(
                &publication.id,
                ArtifactFinalization {
                    final_path: ready_file.path.clone(),
                    content,
                },
            )
            .await;
        match finalized {
            Ok(Some(ready)) if ready.is_ready() => Ok(ready),
            Ok(Some(_)) => {
                self.compensate_ready(
                    &publication.id,
                    &ready_file.path,
                    "artifact repository returned a non-ready publication",
                )
                .await;
                Err(ArtifactServiceError::InvalidReadyPublication)
            }
            Ok(None) => {
                self.compensate_ready(
                    &publication.id,
                    &ready_file.path,
                    "artifact finalization conflict",
                )
                .await;
                Err(ArtifactServiceError::FinalizationConflict)
            }
            Err(error) => {
                self.compensate_ready(&publication.id, &ready_file.path, &error.to_string())
                    .await;
                Err(error.into())
            }
        }
    }

    async fn compensate_staging(&self, id: &str, path: &str, reason: &str) {
        let _ = self.storage.delete(path).await;
        let _ = self.repository.fail(id, reason).await;
    }

    async fn compensate_ready(&self, id: &str, path: &str, reason: &str) {
        let _ = self.storage.delete(path).await;
        let _ = self.repository.fail(id, reason).await;
    }

    pub async fn reconcile_pending(
        &self,
    ) -> Result<ArtifactReconciliationReport, ArtifactServiceError> {
        let pending = self.repository.pending().await?;
        let mut report = ArtifactReconciliationReport::default();
        for publication in pending {
            let Some(staging_path) = publication.staging_path.as_deref() else {
                self.repository
                    .fail(&publication.id, "pending artifact has no staging path")
                    .await?;
                report.failed += 1;
                continue;
            };
            match self.storage.recover_staging(staging_path).await {
                Ok(Some(file)) => {
                    let content =
                        with_file_metadata(publication.content.clone(), &file.path, file.size);
                    match self
                        .repository
                        .finalize(
                            &publication.id,
                            ArtifactFinalization {
                                final_path: file.path.clone(),
                                content,
                            },
                        )
                        .await
                    {
                        Ok(Some(ready)) if ready.is_ready() => report.recovered += 1,
                        Ok(_) => {
                            self.compensate_ready(
                                &publication.id,
                                &file.path,
                                "artifact reconciliation finalization conflict",
                            )
                            .await;
                            report.failed += 1;
                        }
                        Err(error) => {
                            self.compensate_ready(&publication.id, &file.path, &error.to_string())
                                .await;
                            report.failed += 1;
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
                    let _ = self.storage.delete(staging_path).await;
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
