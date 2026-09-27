use async_trait::async_trait;
use thiserror::Error;

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

    /// Recover a publication interrupted before or after the atomic promote.
    async fn recover_staging(
        &self,
        staging_path: &str,
    ) -> Result<Option<ReadyArtifactFile>, FileStorageError>;

    async fn delete(&self, path: &str) -> Result<(), FileStorageError>;
}
