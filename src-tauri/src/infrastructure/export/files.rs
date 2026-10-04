use std::path::{Path, PathBuf};

use async_trait::async_trait;

use crate::application::office_export::{ExportFileStore, ExportedFile, OfficeExportError};

pub struct LocalExportFileStore {
    root: PathBuf,
}

impl LocalExportFileStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn path(&self, filename: &str) -> Result<PathBuf, OfficeExportError> {
        if filename.is_empty()
            || filename != crate::application::office_export::sanitize_filename(filename)
            || Path::new(filename).components().count() != 1
        {
            return Err(OfficeExportError::Invalid("invalid export filename".into()));
        }
        Ok(self.root.join(filename))
    }
}

#[async_trait]
impl ExportFileStore for LocalExportFileStore {
    async fn save(&self, file: &ExportedFile) -> Result<(), OfficeExportError> {
        let path = self.path(&file.filename)?;
        tokio::fs::create_dir_all(&self.root)
            .await
            .map_err(|error| OfficeExportError::Storage(anyhow::Error::new(error)))?;
        let temporary = self
            .root
            .join(format!(".{}.{}.tmp", file.filename, uuid::Uuid::new_v4()));
        if let Err(error) = tokio::fs::write(&temporary, &file.bytes).await {
            return Err(OfficeExportError::Storage(anyhow::Error::new(error)));
        }
        let existing = tokio::fs::try_exists(&path)
            .await
            .map_err(|error| OfficeExportError::Storage(anyhow::Error::new(error)))?;
        if !existing {
            if let Err(error) = tokio::fs::rename(&temporary, &path).await {
                let _ = tokio::fs::remove_file(&temporary).await;
                return Err(OfficeExportError::Storage(anyhow::Error::new(error)));
            }
            return Ok(());
        }

        let backup = self
            .root
            .join(format!(".{}.{}.bak", file.filename, uuid::Uuid::new_v4()));
        if let Err(error) = tokio::fs::rename(&path, &backup).await {
            let _ = tokio::fs::remove_file(&temporary).await;
            return Err(OfficeExportError::Storage(anyhow::Error::new(error)));
        }
        if let Err(error) = tokio::fs::rename(&temporary, &path).await {
            let _ = tokio::fs::rename(&backup, &path).await;
            let _ = tokio::fs::remove_file(&temporary).await;
            return Err(OfficeExportError::Storage(anyhow::Error::new(error)));
        }
        tokio::fs::remove_file(&backup)
            .await
            .map_err(|error| OfficeExportError::Storage(anyhow::Error::new(error)))
    }

    async fn read(&self, filename: &str) -> Result<Option<ExportedFile>, OfficeExportError> {
        let path = self.path(filename)?;
        let bytes = match tokio::fs::read(&path).await {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(OfficeExportError::Storage(anyhow::Error::new(error)));
            }
        };
        Ok(Some(ExportedFile {
            filename: filename.into(),
            content_type: mime_guess::from_path(path)
                .first_or_octet_stream()
                .to_string(),
            bytes,
        }))
    }
}
