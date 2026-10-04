use std::path::PathBuf;

use async_trait::async_trait;

use crate::application::artifacts::{
    FileStorage, FileStorageError, ReadyArtifactFile, StagedArtifactFile,
};

pub struct LocalArtifactStorage {
    root: PathBuf,
}

impl LocalArtifactStorage {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn checked_path(&self, value: &str) -> Result<PathBuf, FileStorageError> {
        let path = PathBuf::from(value);
        if !path.starts_with(&self.root) {
            return Err(FileStorageError::InvalidPath(value.into()));
        }
        Ok(path)
    }
}

#[async_trait]
impl FileStorage for LocalArtifactStorage {
    fn staging_file(
        &self,
        artifact_id: &str,
        extension: &str,
    ) -> Result<StagedArtifactFile, FileStorageError> {
        if artifact_id.is_empty()
            || !artifact_id
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '-')
        {
            return Err(FileStorageError::InvalidPath(artifact_id.into()));
        }
        let extension = extension.trim_start_matches('.');
        if extension.is_empty()
            || !extension
                .chars()
                .all(|character| character.is_ascii_alphanumeric())
        {
            return Err(FileStorageError::InvalidPath(extension.into()));
        }
        let filename = format!("{artifact_id}.{extension}");
        Ok(StagedArtifactFile {
            path: self
                .root
                .join("staging")
                .join(&filename)
                .to_string_lossy()
                .into_owned(),
            final_path: self
                .root
                .join("ready")
                .join(filename)
                .to_string_lossy()
                .into_owned(),
        })
    }

    async fn write_staging(
        &self,
        file: &StagedArtifactFile,
        bytes: &[u8],
    ) -> Result<(), FileStorageError> {
        let path = self.checked_path(&file.path)?;
        let parent = path
            .parent()
            .ok_or_else(|| FileStorageError::InvalidPath(file.path.clone()))?;
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(FileStorageError::Io)?;
        tokio::fs::write(path, bytes)
            .await
            .map_err(FileStorageError::Io)
    }

    async fn validate(&self, file: &StagedArtifactFile) -> Result<(), FileStorageError> {
        let path = self.checked_path(&file.path)?;
        let metadata = tokio::fs::metadata(path)
            .await
            .map_err(FileStorageError::Io)?;
        if !metadata.is_file() || metadata.len() == 0 {
            return Err(FileStorageError::InvalidFile(file.path.clone()));
        }
        Ok(())
    }

    async fn promote(
        &self,
        file: &StagedArtifactFile,
    ) -> Result<ReadyArtifactFile, FileStorageError> {
        let staging_path = self.checked_path(&file.path)?;
        let final_path = self.checked_path(&file.final_path)?;
        let parent = final_path
            .parent()
            .ok_or_else(|| FileStorageError::InvalidPath(file.final_path.clone()))?;
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(FileStorageError::Io)?;
        tokio::fs::rename(&staging_path, &final_path)
            .await
            .map_err(FileStorageError::Io)?;
        let size = tokio::fs::metadata(&final_path)
            .await
            .map_err(FileStorageError::Io)?
            .len();
        Ok(ReadyArtifactFile {
            path: final_path.to_string_lossy().into_owned(),
            size,
        })
    }

    async fn recover_staging(
        &self,
        staging_path: &str,
    ) -> Result<Option<ReadyArtifactFile>, FileStorageError> {
        let staging_path = self.checked_path(staging_path)?;
        if staging_path.parent() != Some(self.root.join("staging").as_path()) {
            return Err(FileStorageError::InvalidPath(
                staging_path.to_string_lossy().into_owned(),
            ));
        }
        let filename = staging_path.file_name().ok_or_else(|| {
            FileStorageError::InvalidPath(staging_path.to_string_lossy().into_owned())
        })?;
        let final_path = self.root.join("ready").join(filename);
        match tokio::fs::metadata(&staging_path).await {
            Ok(metadata) if metadata.is_file() && metadata.len() > 0 => {
                let file = StagedArtifactFile {
                    path: staging_path.to_string_lossy().into_owned(),
                    final_path: final_path.to_string_lossy().into_owned(),
                };
                self.promote(&file).await.map(Some)
            }
            Ok(_) => Err(FileStorageError::InvalidFile(
                staging_path.to_string_lossy().into_owned(),
            )),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                match tokio::fs::metadata(&final_path).await {
                    Ok(metadata) if metadata.is_file() && metadata.len() > 0 => {
                        Ok(Some(ReadyArtifactFile {
                            path: final_path.to_string_lossy().into_owned(),
                            size: metadata.len(),
                        }))
                    }
                    Ok(_) => Err(FileStorageError::InvalidFile(
                        final_path.to_string_lossy().into_owned(),
                    )),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
                    Err(error) => Err(FileStorageError::Io(error)),
                }
            }
            Err(error) => Err(FileStorageError::Io(error)),
        }
    }

    async fn delete(&self, path: &str) -> Result<(), FileStorageError> {
        let path = self.checked_path(path)?;
        match tokio::fs::remove_file(path).await {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(FileStorageError::Io(error)),
        }
    }
}
