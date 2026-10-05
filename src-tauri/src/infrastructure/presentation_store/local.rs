use std::path::{Path, PathBuf};

use async_trait::async_trait;

use crate::capabilities::presentation::{
    PresentationProject, PresentationStore, PresentationStoreError,
};

pub struct LocalPresentationStore {
    root: PathBuf,
}

impl LocalPresentationStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn project_path(&self, project_id: &str) -> Result<PathBuf, PresentationStoreError> {
        if project_id.is_empty()
            || !project_id
                .chars()
                .all(|value| value.is_ascii_alphanumeric() || matches!(value, '-' | '_'))
        {
            return Err(PresentationStoreError::Unavailable(anyhow::anyhow!(
                "invalid presentation project id"
            )));
        }
        Ok(self.root.join(format!("{project_id}.json")))
    }

    async fn read_path(path: &Path) -> Result<Option<PresentationProject>, PresentationStoreError> {
        match tokio::fs::read(path).await {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|error| PresentationStoreError::Unavailable(error.into())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(PresentationStoreError::Unavailable(error.into())),
        }
    }
}

#[async_trait]
impl PresentationStore for LocalPresentationStore {
    async fn save(&self, project: &PresentationProject) -> Result<(), PresentationStoreError> {
        tokio::fs::create_dir_all(&self.root)
            .await
            .map_err(|error| PresentationStoreError::Unavailable(error.into()))?;
        let path = self.project_path(&project.id)?;
        let temporary = self
            .root
            .join(format!(".{}.{}.tmp", project.id, uuid::Uuid::new_v4()));
        let backup = self
            .root
            .join(format!(".{}.{}.bak", project.id, uuid::Uuid::new_v4()));
        let bytes = serde_json::to_vec_pretty(project)
            .map_err(|error| PresentationStoreError::Unavailable(error.into()))?;
        tokio::fs::write(&temporary, bytes)
            .await
            .map_err(|error| PresentationStoreError::Unavailable(error.into()))?;

        let had_previous = tokio::fs::metadata(&path).await.is_ok();
        if had_previous && let Err(error) = tokio::fs::rename(&path, &backup).await {
            let _ = tokio::fs::remove_file(&temporary).await;
            return Err(PresentationStoreError::Unavailable(error.into()));
        }
        if let Err(error) = tokio::fs::rename(&temporary, &path).await {
            if had_previous {
                let _ = tokio::fs::rename(&backup, &path).await;
            }
            let _ = tokio::fs::remove_file(&temporary).await;
            return Err(PresentationStoreError::Unavailable(error.into()));
        }
        if had_previous {
            let _ = tokio::fs::remove_file(&backup).await;
        }
        Ok(())
    }

    async fn load(
        &self,
        project_id: &str,
    ) -> Result<Option<PresentationProject>, PresentationStoreError> {
        Self::read_path(&self.project_path(project_id)?).await
    }

    async fn list(
        &self,
        owner_id: &str,
    ) -> Result<Vec<PresentationProject>, PresentationStoreError> {
        let mut entries = match tokio::fs::read_dir(&self.root).await {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
            Err(error) => return Err(PresentationStoreError::Unavailable(error.into())),
        };
        let mut projects = Vec::new();
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|error| PresentationStoreError::Unavailable(error.into()))?
        {
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            if let Ok(Some(project)) = Self::read_path(&path).await
                && project.owner_id == owner_id
            {
                projects.push(project);
            }
        }
        projects.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        Ok(projects)
    }

    async fn delete(
        &self,
        owner_id: &str,
        project_id: &str,
    ) -> Result<bool, PresentationStoreError> {
        let path = self.project_path(project_id)?;
        let Some(project) = Self::read_path(&path).await? else {
            return Ok(false);
        };
        if project.owner_id != owner_id {
            return Ok(false);
        }
        match tokio::fs::remove_file(path).await {
            Ok(()) => Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(PresentationStoreError::Unavailable(error.into())),
        }
    }
}
