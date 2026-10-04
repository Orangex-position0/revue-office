use async_trait::async_trait;
use thiserror::Error;

use super::{ArtifactFinalization, ArtifactPublication, NewArtifactPublication};

#[derive(Debug, Error)]
pub enum ArtifactPublicationRepositoryError {
    #[error("artifact publication repository is unavailable")]
    Unavailable(#[source] anyhow::Error),
    #[error("artifact publication data is invalid: {0}")]
    InvalidData(String),
    #[error("artifact publication state conflict")]
    Conflict,
}

#[async_trait]
pub trait ArtifactPublicationRepository: Send + Sync {
    async fn reserve(
        &self,
        publication: NewArtifactPublication,
    ) -> Result<ArtifactPublication, ArtifactPublicationRepositoryError>;

    async fn find(
        &self,
        id: &str,
    ) -> Result<Option<ArtifactPublication>, ArtifactPublicationRepositoryError>;

    async fn finalize(
        &self,
        id: &str,
        finalization: ArtifactFinalization,
    ) -> Result<Option<ArtifactPublication>, ArtifactPublicationRepositoryError>;

    async fn fail(&self, id: &str, error: &str)
    -> Result<bool, ArtifactPublicationRepositoryError>;

    async fn pending(&self)
    -> Result<Vec<ArtifactPublication>, ArtifactPublicationRepositoryError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedArtifactFile {
    pub path: String,
    pub final_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadyArtifactFile {
    pub path: String,
    pub size: u64,
}

#[derive(Debug, Error)]
pub enum FileStorageError {
    #[error("artifact file path is invalid: {0}")]
    InvalidPath(String),
    #[error("artifact file is invalid: {0}")]
    InvalidFile(String),
    #[error("artifact file storage failed")]
    Io(#[source] std::io::Error),
}

#[async_trait]
pub trait FileStorage: Send + Sync {
    fn staging_file(
        &self,
        artifact_id: &str,
        extension: &str,
    ) -> Result<StagedArtifactFile, FileStorageError>;

    async fn write_staging(
        &self,
        file: &StagedArtifactFile,
        bytes: &[u8],
    ) -> Result<(), FileStorageError>;

    async fn validate(&self, file: &StagedArtifactFile) -> Result<(), FileStorageError>;

    async fn promote(
        &self,
        file: &StagedArtifactFile,
    ) -> Result<ReadyArtifactFile, FileStorageError>;

    /// Recover a publication interrupted immediately before or after its atomic promote.
    async fn recover_staging(
        &self,
        staging_path: &str,
    ) -> Result<Option<ReadyArtifactFile>, FileStorageError>;

    async fn delete(&self, path: &str) -> Result<(), FileStorageError>;
}
