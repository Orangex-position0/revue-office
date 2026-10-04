use async_trait::async_trait;
use sqlx::{Row, SqlitePool};

use crate::application::assets::*;
use crate::application::identity::ActorId;

pub struct SqliteAssetRepository {
    pool: SqlitePool,
}

impl SqliteAssetRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
    pub async fn connect(url: &str, max: u32) -> anyhow::Result<Self> {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(max)
            .connect(url)
            .await?;
        crate::infrastructure::persistence::migrations::run_migrations_sqlite(&pool).await?;
        Ok(Self::new(pool))
    }
    fn error(e: sqlx::Error) -> AssetError {
        AssetError::Repository(anyhow::Error::new(e))
    }
    fn asset(row: &sqlx::sqlite::SqliteRow) -> Result<Asset, AssetError> {
        let metadata: Option<String> = row.try_get("metadata").map_err(Self::error)?;
        Ok(Asset {
            id: row.try_get("id").map_err(Self::error)?,
            owner_id: row.try_get("owner_id").map_err(Self::error)?,
            name: row.try_get("name").map_err(Self::error)?,
            file_path: row.try_get("file_path").map_err(Self::error)?,
            file_type: row.try_get("file_type").map_err(Self::error)?,
            file_size: row.try_get("file_size").map_err(Self::error)?,
            folder_id: row.try_get("folder_id").map_err(Self::error)?,
            description: row.try_get("description").map_err(Self::error)?,
            metadata: metadata.and_then(|v| serde_json::from_str(&v).ok()),
            created_at: row.try_get("created_at").map_err(Self::error)?,
            updated_at: row.try_get("updated_at").map_err(Self::error)?,
        })
    }
    fn folder(row: &sqlx::sqlite::SqliteRow) -> Result<Folder, AssetError> {
        Ok(Folder {
            id: row.try_get("id").map_err(Self::error)?,
            owner_id: row.try_get("owner_id").map_err(Self::error)?,
            name: row.try_get("name").map_err(Self::error)?,
            parent_id: row.try_get("parent_id").map_err(Self::error)?,
            created_at: row.try_get("created_at").map_err(Self::error)?,
            updated_at: row.try_get("updated_at").map_err(Self::error)?,
        })
    }
}

const FILE_COLUMNS: &str = "id, owner_id, name, file_path, file_type, file_size, folder_id, description, metadata, created_at, updated_at";

#[async_trait]
impl AssetRepository for SqliteAssetRepository {
    async fn create(&self, value: NewAssetRecord) -> Result<Asset, AssetError> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let metadata = value
            .metadata
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|e| AssetError::Repository(e.into()))?;
        sqlx::query("INSERT INTO files (id, owner_id, name, file_path, file_type, file_size, folder_id, description, metadata, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
            .bind(&id).bind(&value.owner_id.0).bind(&value.name).bind(&value.location.0).bind(&value.file_type).bind(value.file_size)
            .bind(value.folder_id.as_ref().map(|v| &v.0)).bind(&value.description).bind(&metadata).bind(&now).bind(&now)
            .execute(&self.pool).await.map_err(Self::error)?;
        Ok(Asset {
            id,
            owner_id: value.owner_id.0,
            name: value.name,
            file_path: value.location.0,
            file_type: value.file_type,
            file_size: value.file_size,
            folder_id: value.folder_id.map(|v| v.0),
            description: value.description,
            metadata: value.metadata,
            created_at: now.clone(),
            updated_at: now,
        })
    }
    async fn get(&self, owner: &ActorId, id: &AssetId) -> Result<Option<Asset>, AssetError> {
        let sql = format!("SELECT {FILE_COLUMNS} FROM files WHERE id = ? AND owner_id = ?");
        let row = sqlx::query(&sql)
            .bind(&id.0)
            .bind(&owner.0)
            .fetch_optional(&self.pool)
            .await
            .map_err(Self::error)?;
        row.as_ref().map(Self::asset).transpose()
    }
    async fn list(&self, q: AssetListQuery) -> Result<Vec<Asset>, AssetError> {
        let (sql, folder) = if q.folder_id.is_some() {
            (
                format!(
                    "SELECT {FILE_COLUMNS} FROM files WHERE owner_id = ? AND folder_id = ? ORDER BY updated_at DESC"
                ),
                q.folder_id.map(|v| v.0),
            )
        } else {
            (
                format!(
                    "SELECT {FILE_COLUMNS} FROM files WHERE owner_id = ? AND folder_id IS NULL ORDER BY updated_at DESC"
                ),
                None,
            )
        };
        let mut query = sqlx::query(&sql).bind(&q.owner_id.0);
        if let Some(folder) = folder {
            query = query.bind(folder);
        }
        query
            .fetch_all(&self.pool)
            .await
            .map_err(Self::error)?
            .iter()
            .map(Self::asset)
            .collect()
    }
    async fn search(&self, q: AssetSearchQuery) -> Result<Vec<Asset>, AssetError> {
        let pattern = format!("%{}%", q.query.unwrap_or_default().trim());
        let sql = format!(
            "SELECT {FILE_COLUMNS} FROM files WHERE owner_id = ? AND (? = '%%' OR name LIKE ? OR COALESCE(description, '') LIKE ?) ORDER BY updated_at DESC"
        );
        sqlx::query(&sql)
            .bind(&q.owner_id.0)
            .bind(&pattern)
            .bind(&pattern)
            .bind(&pattern)
            .fetch_all(&self.pool)
            .await
            .map_err(Self::error)?
            .iter()
            .map(Self::asset)
            .collect()
    }
    async fn stats(&self, owner: &ActorId) -> Result<AssetStats, AssetError> {
        let row = sqlx::query("SELECT COUNT(*) AS total_files, COALESCE(SUM(file_size), 0) AS total_size FROM files WHERE owner_id = ?")
            .bind(&owner.0).fetch_one(&self.pool).await.map_err(Self::error)?;
        let mut by_type = std::collections::HashMap::new();
        for row in sqlx::query(
            "SELECT file_type, COUNT(*) AS count FROM files WHERE owner_id = ? GROUP BY file_type",
        )
        .bind(&owner.0)
        .fetch_all(&self.pool)
        .await
        .map_err(Self::error)?
        {
            by_type.insert(
                row.try_get("file_type").map_err(Self::error)?,
                row.try_get("count").map_err(Self::error)?,
            );
        }
        Ok(AssetStats {
            by_type,
            total_size: row.try_get("total_size").map_err(Self::error)?,
            total_files: row.try_get("total_files").map_err(Self::error)?,
        })
    }
    async fn delete(&self, owner: &ActorId, id: &AssetId) -> Result<bool, AssetError> {
        Ok(
            sqlx::query("DELETE FROM files WHERE id = ? AND owner_id = ?")
                .bind(&id.0)
                .bind(&owner.0)
                .execute(&self.pool)
                .await
                .map_err(Self::error)?
                .rows_affected()
                > 0,
        )
    }
    async fn get_folder(
        &self,
        owner: &ActorId,
        id: &FolderId,
    ) -> Result<Option<Folder>, AssetError> {
        let row = sqlx::query("SELECT id, owner_id, name, parent_id, created_at, updated_at FROM folders WHERE id = ? AND owner_id = ?")
            .bind(&id.0).bind(&owner.0).fetch_optional(&self.pool).await.map_err(Self::error)?;
        row.as_ref().map(Self::folder).transpose()
    }
    async fn list_folders(
        &self,
        owner: &ActorId,
        parent: Option<&FolderId>,
    ) -> Result<Vec<Folder>, AssetError> {
        let rows = if let Some(parent) = parent {
            sqlx::query("SELECT id, owner_id, name, parent_id, created_at, updated_at FROM folders WHERE owner_id = ? AND parent_id = ? ORDER BY name ASC").bind(&owner.0).bind(&parent.0).fetch_all(&self.pool).await
        } else { sqlx::query("SELECT id, owner_id, name, parent_id, created_at, updated_at FROM folders WHERE owner_id = ? AND parent_id IS NULL ORDER BY name ASC").bind(&owner.0).fetch_all(&self.pool).await }.map_err(Self::error)?;
        rows.iter().map(Self::folder).collect()
    }
    async fn create_folder(&self, value: NewFolder) -> Result<Folder, AssetError> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query("INSERT INTO folders (id, owner_id, name, parent_id, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)")
            .bind(&id).bind(&value.owner_id.0).bind(&value.name).bind(value.parent_id.as_ref().map(|v| &v.0)).bind(&now).bind(&now)
            .execute(&self.pool).await.map_err(Self::error)?;
        Ok(Folder {
            id,
            owner_id: value.owner_id.0,
            name: value.name,
            parent_id: value.parent_id.map(|v| v.0),
            created_at: now.clone(),
            updated_at: now,
        })
    }
    async fn folder_tree_manifest(
        &self,
        owner: &ActorId,
        id: &FolderId,
    ) -> Result<Option<FolderTreeManifest>, AssetError> {
        if self.get_folder(owner, id).await?.is_none() {
            return Ok(None);
        }
        let mut ids = vec![id.clone()];
        let mut index = 0;
        while index < ids.len() {
            let children =
                sqlx::query("SELECT id FROM folders WHERE owner_id = ? AND parent_id = ?")
                    .bind(&owner.0)
                    .bind(&ids[index].0)
                    .fetch_all(&self.pool)
                    .await
                    .map_err(Self::error)?;
            for row in children {
                ids.push(FolderId(row.try_get("id").map_err(Self::error)?));
            }
            index += 1;
        }
        let mut assets = Vec::new();
        for folder in &ids {
            assets.extend(
                self.list(AssetListQuery {
                    owner_id: owner.clone(),
                    folder_id: Some(folder.clone()),
                })
                .await?,
            );
        }
        Ok(Some(FolderTreeManifest {
            folder_ids: ids,
            assets,
        }))
    }
    async fn delete_folder_tree(
        &self,
        owner: &ActorId,
        manifest: &FolderTreeManifest,
    ) -> Result<bool, AssetError> {
        let mut tx = self.pool.begin().await.map_err(Self::error)?;
        for asset in &manifest.assets {
            sqlx::query("DELETE FROM files WHERE id = ? AND owner_id = ?")
                .bind(&asset.id)
                .bind(&owner.0)
                .execute(&mut *tx)
                .await
                .map_err(Self::error)?;
        }
        let mut deleted = false;
        for folder in manifest.folder_ids.iter().rev() {
            deleted |= sqlx::query("DELETE FROM folders WHERE id = ? AND owner_id = ?")
                .bind(&folder.0)
                .bind(&owner.0)
                .execute(&mut *tx)
                .await
                .map_err(Self::error)?
                .rows_affected()
                > 0;
        }
        tx.commit().await.map_err(Self::error)?;
        Ok(deleted)
    }
}
