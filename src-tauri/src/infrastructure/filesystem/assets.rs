use std::path::{Path, PathBuf};

use async_trait::async_trait;

use crate::application::assets::{
    AssetError, AssetLocation, AssetStorage, AssetWrite, QuarantinedAsset, StoredAsset,
};

pub struct LocalAssetStorage {
    root: PathBuf,
}

impl LocalAssetStorage {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn resolve(&self, location: &AssetLocation) -> Result<PathBuf, AssetError> {
        let path = PathBuf::from(&location.0);
        if path.is_absolute() {
            return Ok(path);
        }
        if path
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
        {
            return Err(AssetError::Invalid(
                "asset path escapes storage root".into(),
            ));
        }
        // Legacy rows may contain either `data/files/<owner>/<file>` or a
        // path relative to that root. Avoid prefixing an already rooted
        // relative path a second time.
        if path.starts_with(&self.root) {
            Ok(path)
        } else {
            Ok(self.root.join(path))
        }
    }

    fn safe_owner(owner: &str) -> Result<&str, AssetError> {
        if owner.is_empty()
            || !owner
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(AssetError::Invalid("invalid asset owner".into()));
        }
        Ok(owner)
    }
}

#[async_trait]
impl AssetStorage for LocalAssetStorage {
    async fn write(&self, request: AssetWrite) -> Result<StoredAsset, AssetError> {
        let owner = Self::safe_owner(&request.owner_id.0)?;
        let extension = Path::new(&request.name)
            .extension()
            .and_then(|v| v.to_str())
            .filter(|v| !v.is_empty())
            .map(|v| format!(".{v}"))
            .unwrap_or_default();
        let directory = self.root.join(owner);
        tokio::fs::create_dir_all(&directory)
            .await
            .map_err(|e| AssetError::Storage(anyhow::Error::new(e)))?;
        let path = directory.join(format!("{}{}", uuid::Uuid::new_v4(), extension));
        tokio::fs::write(&path, request.bytes)
            .await
            .map_err(|e| AssetError::Storage(anyhow::Error::new(e)))?;
        Ok(StoredAsset {
            location: AssetLocation(path.to_string_lossy().into_owned()),
        })
    }

    async fn read(&self, location: &AssetLocation) -> Result<Vec<u8>, AssetError> {
        let path = self.resolve(location)?;
        tokio::fs::read(path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                AssetError::NotFound
            } else {
                AssetError::Storage(anyhow::Error::new(e))
            }
        })
    }

    async fn quarantine(
        &self,
        location: &AssetLocation,
    ) -> Result<Option<QuarantinedAsset>, AssetError> {
        let original = self.resolve(location)?;
        if !tokio::fs::try_exists(&original)
            .await
            .map_err(|e| AssetError::Storage(anyhow::Error::new(e)))?
        {
            return Ok(None);
        }
        let directory = self.root.join(".quarantine");
        tokio::fs::create_dir_all(&directory)
            .await
            .map_err(|e| AssetError::Storage(anyhow::Error::new(e)))?;
        let quarantine = directory.join(uuid::Uuid::new_v4().to_string());
        tokio::fs::rename(&original, &quarantine)
            .await
            .map_err(|e| AssetError::Storage(anyhow::Error::new(e)))?;
        Ok(Some(QuarantinedAsset {
            original: AssetLocation(original.to_string_lossy().into_owned()),
            quarantine: AssetLocation(quarantine.to_string_lossy().into_owned()),
        }))
    }

    async fn restore(&self, asset: &QuarantinedAsset) -> Result<(), AssetError> {
        let original = self.resolve(&asset.original)?;
        let quarantine = self.resolve(&asset.quarantine)?;
        if let Some(parent) = original.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| AssetError::Storage(anyhow::Error::new(e)))?;
        }
        tokio::fs::rename(quarantine, original)
            .await
            .map_err(|e| AssetError::Storage(anyhow::Error::new(e)))
    }

    async fn purge(&self, asset: QuarantinedAsset) -> Result<(), AssetError> {
        let quarantine = self.resolve(&asset.quarantine)?;
        match tokio::fs::remove_file(quarantine).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(AssetError::Storage(anyhow::Error::new(e))),
        }
    }
}
