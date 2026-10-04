use async_trait::async_trait;
use sqlx::{MySqlPool, Row};

use crate::application::identity::ActorId;
use crate::application::projects::*;

pub struct MySqlProjectRepository {
    pool: MySqlPool,
}

impl MySqlProjectRepository {
    pub fn new(pool: MySqlPool) -> Self {
        Self { pool }
    }

    pub async fn connect(url: &str, max_connections: u32) -> anyhow::Result<Self> {
        let pool = sqlx::mysql::MySqlPoolOptions::new()
            .max_connections(max_connections)
            .connect(url)
            .await?;
        crate::infrastructure::persistence::migrations::run_migrations_mysql(&pool).await?;
        Ok(Self::new(pool))
    }

    fn error(error: sqlx::Error) -> ProjectError {
        ProjectError::Repository(anyhow::Error::new(error))
    }

    fn project(row: &sqlx::mysql::MySqlRow) -> Result<Project, ProjectError> {
        Ok(Project {
            id: row.try_get("id").map_err(Self::error)?,
            title: row.try_get("title").map_err(Self::error)?,
            description: row.try_get("description").map_err(Self::error)?,
            tool_kind: row.try_get("tool_kind").map_err(Self::error)?,
            owner_id: row.try_get("owner_id").map_err(Self::error)?,
            created_at: row.try_get("created_at").map_err(Self::error)?,
            updated_at: row.try_get("updated_at").map_err(Self::error)?,
        })
    }
}

const COLUMNS: &str = "id, title, description, tool_kind, owner_id, created_at, updated_at";

#[async_trait]
impl ProjectRepository for MySqlProjectRepository {
    async fn create(&self, value: NewProject) -> Result<Project, ProjectError> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query("INSERT INTO projects (id, title, description, tool_kind, owner_id, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?)")
            .bind(&id).bind(&value.title).bind(&value.description).bind(&value.tool_kind).bind(&value.owner_id.0).bind(&now).bind(&now)
            .execute(&self.pool).await.map_err(Self::error)?;
        Ok(Project {
            id,
            title: value.title,
            description: value.description,
            tool_kind: value.tool_kind,
            owner_id: value.owner_id.0,
            created_at: now.clone(),
            updated_at: now,
        })
    }

    async fn find(&self, owner: &ActorId, id: &ProjectId) -> Result<Option<Project>, ProjectError> {
        let sql = format!("SELECT {COLUMNS} FROM projects WHERE id = ? AND owner_id = ?");
        let row = sqlx::query(&sql)
            .bind(&id.0)
            .bind(&owner.0)
            .fetch_optional(&self.pool)
            .await
            .map_err(Self::error)?;
        row.as_ref().map(Self::project).transpose()
    }

    async fn list(&self, value: ProjectListQuery) -> Result<Vec<Project>, ProjectError> {
        let query = value
            .query
            .map(|item| format!("%{}%", item.trim()))
            .filter(|item| item != "%%");
        let rows = if let Some(query) = query {
            let sql = format!("SELECT {COLUMNS} FROM projects WHERE owner_id = ? AND (title LIKE ? OR COALESCE(description, '') LIKE ?) ORDER BY updated_at DESC");
            sqlx::query(&sql).bind(&value.owner_id.0).bind(&query).bind(&query).fetch_all(&self.pool).await
        } else {
            let sql = format!("SELECT {COLUMNS} FROM projects WHERE owner_id = ? ORDER BY updated_at DESC");
            sqlx::query(&sql).bind(&value.owner_id.0).fetch_all(&self.pool).await
        }.map_err(Self::error)?;
        rows.iter().map(Self::project).collect()
    }

    async fn update(&self, value: ProjectUpdate) -> Result<Option<Project>, ProjectError> {
        let Some(current) = self.find(&value.owner_id, &value.id).await? else {
            return Ok(None);
        };
        let title = value.title.unwrap_or(current.title);
        let description = value.description.unwrap_or(current.description);
        let tool_kind = value.tool_kind.unwrap_or(current.tool_kind);
        let updated_at = chrono::Utc::now().to_rfc3339();
        let result = sqlx::query("UPDATE projects SET title = ?, description = ?, tool_kind = ?, updated_at = ? WHERE id = ? AND owner_id = ?")
            .bind(&title).bind(&description).bind(&tool_kind).bind(&updated_at).bind(&value.id.0).bind(&value.owner_id.0)
            .execute(&self.pool).await.map_err(Self::error)?;
        if result.rows_affected() == 0 {
            return Ok(None);
        }
        Ok(Some(Project {
            id: current.id,
            title,
            description,
            tool_kind,
            owner_id: current.owner_id,
            created_at: current.created_at,
            updated_at,
        }))
    }

    async fn delete(&self, owner: &ActorId, id: &ProjectId) -> Result<bool, ProjectError> {
        Ok(
            sqlx::query("DELETE FROM projects WHERE id = ? AND owner_id = ?")
                .bind(&id.0)
                .bind(&owner.0)
                .execute(&self.pool)
                .await
                .map_err(Self::error)?
                .rows_affected()
                > 0,
        )
    }
}
