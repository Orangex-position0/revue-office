use async_trait::async_trait;
use sqlx::mysql::{MySqlPoolOptions, MySqlRow};
use sqlx::{MySqlPool, Row};

use crate::contracts::artifact::{
    ArtifactFinalization, ArtifactPublication, ArtifactPublicationStatus, NewArtifactPublication,
};
use crate::contracts::conversation::{
    Conversation, ConversationArtifact, ConversationMessage, NewConversation,
};
use crate::ports::repositories::artifact_publication::{
    ArtifactPublicationRepository, ArtifactPublicationRepositoryError,
};
use crate::ports::repositories::session::{SessionRepository, SessionRepositoryError};

pub struct MySqlSessionRepository {
    pool: MySqlPool,
}

impl MySqlSessionRepository {
    pub async fn connect(database_url: &str, max_connections: u32) -> anyhow::Result<Self> {
        let pool = MySqlPoolOptions::new()
            .max_connections(max_connections)
            .connect(database_url)
            .await?;
        crate::db::run_migrations_mysql(&pool).await?;
        Ok(Self { pool })
    }

    fn unavailable(error: sqlx::Error) -> SessionRepositoryError {
        SessionRepositoryError::Unavailable(anyhow::Error::new(error))
    }

    fn conversation(row: &MySqlRow) -> Result<Conversation, SessionRepositoryError> {
        Ok(Conversation {
            id: row.try_get("id").map_err(Self::unavailable)?,
            owner_id: row.try_get("owner_id").map_err(Self::unavailable)?,
            project_id: row.try_get("project_id").map_err(Self::unavailable)?,
            tool_kind: row.try_get("tool_kind").map_err(Self::unavailable)?,
            title: row.try_get("title").map_err(Self::unavailable)?,
            summary: row.try_get("summary").map_err(Self::unavailable)?,
            message_count: row.try_get("message_count").map_err(Self::unavailable)?,
            order: row.try_get("order_col").map_err(Self::unavailable)?,
            created_at: row.try_get("created_at").map_err(Self::unavailable)?,
            updated_at: row.try_get("updated_at").map_err(Self::unavailable)?,
        })
    }
}

#[async_trait]
impl SessionRepository for MySqlSessionRepository {
    async fn create(
        &self,
        request: NewConversation,
    ) -> Result<Conversation, SessionRepositoryError> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let order = chrono::Utc::now().timestamp();
        sqlx::query(
            "INSERT INTO sessions (id, owner_id, project_id, tool_kind, title, message_count, order_col, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, 0, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&request.owner_id)
        .bind(&request.project_id)
        .bind(&request.tool_kind)
        .bind(&request.title)
        .bind(order)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(Self::unavailable)?;

        Ok(Conversation {
            id,
            owner_id: request.owner_id,
            project_id: request.project_id,
            tool_kind: request.tool_kind,
            title: request.title,
            summary: None,
            message_count: 0,
            order,
            created_at: now.clone(),
            updated_at: now,
        })
    }

    async fn find_by_id(&self, id: &str) -> Result<Option<Conversation>, SessionRepositoryError> {
        let row = sqlx::query(
            "SELECT id, owner_id, project_id, tool_kind, title, summary, message_count, order_col, created_at, updated_at \
             FROM sessions WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(Self::unavailable)?;
        row.as_ref().map(Self::conversation).transpose()
    }

    async fn list_by_owner(
        &self,
        owner_id: &str,
        limit: u32,
        query: Option<&str>,
    ) -> Result<Vec<Conversation>, SessionRepositoryError> {
        let rows = if let Some(query) = query.filter(|query| !query.trim().is_empty()) {
            let pattern = format!("%{}%", query.trim());
            sqlx::query(
                "SELECT id, owner_id, project_id, tool_kind, title, summary, message_count, order_col, created_at, updated_at \
                 FROM sessions WHERE owner_id = ? AND (title LIKE ? OR COALESCE(summary, '') LIKE ?) \
                 ORDER BY order_col ASC, updated_at DESC LIMIT ?",
            )
            .bind(owner_id)
            .bind(&pattern)
            .bind(&pattern)
            .bind(i64::from(limit))
            .fetch_all(&self.pool)
            .await
            .map_err(Self::unavailable)?
        } else {
            sqlx::query(
                "SELECT id, owner_id, project_id, tool_kind, title, summary, message_count, order_col, created_at, updated_at \
                 FROM sessions WHERE owner_id = ? ORDER BY order_col ASC, updated_at DESC LIMIT ?",
            )
            .bind(owner_id)
            .bind(i64::from(limit))
            .fetch_all(&self.pool)
            .await
            .map_err(Self::unavailable)?
        };
        rows.iter().map(Self::conversation).collect()
    }

    async fn append_message(
        &self,
        session_id: &str,
        message: ConversationMessage,
    ) -> Result<(), SessionRepositoryError> {
        let id = uuid::Uuid::new_v4().to_string();
        let tool_input = message
            .tool_calls
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|error| SessionRepositoryError::InvalidData(error.to_string()))?;
        let mut transaction = self.pool.begin().await.map_err(Self::unavailable)?;
        sqlx::query(
            "INSERT INTO messages (id, session_id, role, content, tool_name, tool_input, tool_output, created_at) \
             VALUES (?, ?, ?, ?, NULL, ?, ?, ?)",
        )
        .bind(id)
        .bind(session_id)
        .bind(message.role)
        .bind(message.content)
        .bind(tool_input)
        .bind(message.tool_call_id)
        .bind(&message.created_at)
        .execute(&mut *transaction)
        .await
        .map_err(Self::unavailable)?;
        sqlx::query(
            "UPDATE sessions SET message_count = message_count + 1, updated_at = ? WHERE id = ?",
        )
        .bind(message.created_at)
        .bind(session_id)
        .execute(&mut *transaction)
        .await
        .map_err(Self::unavailable)?;
        transaction.commit().await.map_err(Self::unavailable)?;
        Ok(())
    }

    async fn history(
        &self,
        session_id: &str,
        limit: u32,
    ) -> Result<Vec<ConversationMessage>, SessionRepositoryError> {
        let rows = sqlx::query(
            "SELECT role, content, tool_input, tool_output, created_at FROM messages \
             WHERE session_id = ? ORDER BY created_at ASC LIMIT ?",
        )
        .bind(session_id)
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await
        .map_err(Self::unavailable)?;

        rows.into_iter()
            .map(|row| {
                let tool_input: Option<String> =
                    row.try_get("tool_input").map_err(Self::unavailable)?;
                let tool_calls = tool_input
                    .map(|value| serde_json::from_str(&value))
                    .transpose()
                    .map_err(|error| SessionRepositoryError::InvalidData(error.to_string()))?;
                Ok(ConversationMessage {
                    role: row.try_get("role").map_err(Self::unavailable)?,
                    content: row.try_get("content").map_err(Self::unavailable)?,
                    tool_calls,
                    tool_call_id: row.try_get("tool_output").map_err(Self::unavailable)?,
                    created_at: row.try_get("created_at").map_err(Self::unavailable)?,
                })
            })
            .collect()
    }

    async fn legacy_artifacts(
        &self,
        session_id: &str,
    ) -> Result<Vec<ConversationArtifact>, SessionRepositoryError> {
        let payload: Option<String> =
            sqlx::query_scalar("SELECT payload FROM session_artifacts WHERE session_id = ?")
                .bind(session_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(Self::unavailable)?;
        payload
            .map(|payload| {
                serde_json::from_str(&payload)
                    .map_err(|error| SessionRepositoryError::InvalidData(error.to_string()))
            })
            .transpose()
            .map(Option::unwrap_or_default)
    }

    async fn replace_legacy_artifacts(
        &self,
        session_id: &str,
        artifacts: Vec<ConversationArtifact>,
    ) -> Result<(), SessionRepositoryError> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let payload = serde_json::to_string(&artifacts)
            .map_err(|error| SessionRepositoryError::InvalidData(error.to_string()))?;
        sqlx::query(
            "INSERT INTO session_artifacts (id, session_id, payload, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?) \
             ON DUPLICATE KEY UPDATE payload = VALUES(payload), updated_at = VALUES(updated_at)",
        )
        .bind(id)
        .bind(session_id)
        .bind(payload)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(Self::unavailable)?;
        Ok(())
    }

    async fn update_title(
        &self,
        session_id: &str,
        owner_id: &str,
        title: &str,
    ) -> Result<bool, SessionRepositoryError> {
        let result = sqlx::query(
            "UPDATE sessions SET title = ?, updated_at = ? WHERE id = ? AND owner_id = ?",
        )
        .bind(title)
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(session_id)
        .bind(owner_id)
        .execute(&self.pool)
        .await
        .map_err(Self::unavailable)?;
        Ok(result.rows_affected() > 0)
    }

    async fn update_placement(
        &self,
        session_id: &str,
        owner_id: &str,
        project_id: Option<&str>,
        order: i64,
    ) -> Result<bool, SessionRepositoryError> {
        let result = sqlx::query(
            "UPDATE sessions SET project_id = ?, order_col = ? WHERE id = ? AND owner_id = ?",
        )
        .bind(project_id)
        .bind(order)
        .bind(session_id)
        .bind(owner_id)
        .execute(&self.pool)
        .await
        .map_err(Self::unavailable)?;
        Ok(result.rows_affected() > 0)
    }

    async fn update_summary(
        &self,
        session_id: &str,
        summary: &str,
    ) -> Result<(), SessionRepositoryError> {
        sqlx::query("UPDATE sessions SET summary = ?, updated_at = ? WHERE id = ?")
            .bind(summary)
            .bind(chrono::Utc::now().to_rfc3339())
            .bind(session_id)
            .execute(&self.pool)
            .await
            .map_err(Self::unavailable)?;
        Ok(())
    }

    async fn delete(
        &self,
        session_id: &str,
        owner_id: &str,
    ) -> Result<bool, SessionRepositoryError> {
        let result = sqlx::query("DELETE FROM sessions WHERE id = ? AND owner_id = ?")
            .bind(session_id)
            .bind(owner_id)
            .execute(&self.pool)
            .await
            .map_err(Self::unavailable)?;
        Ok(result.rows_affected() > 0)
    }

    async fn clear_messages(
        &self,
        session_id: &str,
        owner_id: &str,
    ) -> Result<bool, SessionRepositoryError> {
        let mut transaction = self.pool.begin().await.map_err(Self::unavailable)?;
        let exists: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM sessions WHERE id = ? AND owner_id = ?")
                .bind(session_id)
                .bind(owner_id)
                .fetch_one(&mut *transaction)
                .await
                .map_err(Self::unavailable)?;
        if exists == 0 {
            return Ok(false);
        }
        sqlx::query("DELETE FROM messages WHERE session_id = ?")
            .bind(session_id)
            .execute(&mut *transaction)
            .await
            .map_err(Self::unavailable)?;
        sqlx::query("UPDATE sessions SET message_count = 0, updated_at = ? WHERE id = ?")
            .bind(chrono::Utc::now().to_rfc3339())
            .bind(session_id)
            .execute(&mut *transaction)
            .await
            .map_err(Self::unavailable)?;
        transaction.commit().await.map_err(Self::unavailable)?;
        Ok(true)
    }
}

fn mysql_artifact_publication(
    row: &sqlx::mysql::MySqlRow,
) -> Result<ArtifactPublication, ArtifactPublicationRepositoryError> {
    let unavailable =
        |error| ArtifactPublicationRepositoryError::Unavailable(anyhow::Error::new(error));
    let status: String = row.try_get("status").map_err(unavailable)?;
    let content: String = row.try_get("content").map_err(unavailable)?;
    Ok(ArtifactPublication {
        id: row.try_get("id").map_err(unavailable)?,
        session_id: row.try_get("session_id").map_err(unavailable)?,
        owner_id: row.try_get("owner_id").map_err(unavailable)?,
        kind: row.try_get("kind").map_err(unavailable)?,
        title: row.try_get("title").map_err(unavailable)?,
        status: ArtifactPublicationStatus::try_from(status.as_str())
            .map_err(ArtifactPublicationRepositoryError::InvalidData)?,
        content: serde_json::from_str(&content)
            .map_err(|error| ArtifactPublicationRepositoryError::InvalidData(error.to_string()))?,
        staging_path: row.try_get("staging_path").map_err(unavailable)?,
        final_path: row.try_get("final_path").map_err(unavailable)?,
        error: row.try_get("error").map_err(unavailable)?,
        version: row.try_get("version").map_err(unavailable)?,
        created_at: row.try_get("created_at").map_err(unavailable)?,
        updated_at: row.try_get("updated_at").map_err(unavailable)?,
    })
}

#[async_trait]
impl ArtifactPublicationRepository for MySqlSessionRepository {
    async fn reserve(
        &self,
        publication: NewArtifactPublication,
    ) -> Result<ArtifactPublication, ArtifactPublicationRepositoryError> {
        let now = chrono::Utc::now().to_rfc3339();
        let content = serde_json::to_string(&publication.content)
            .map_err(|error| ArtifactPublicationRepositoryError::InvalidData(error.to_string()))?;
        sqlx::query(
            "INSERT INTO artifact_publications (id, session_id, owner_id, kind, title, status, content, staging_path, version, created_at, updated_at) VALUES (?, ?, ?, ?, ?, 'publishing', ?, ?, 1, ?, ?)",
        )
        .bind(&publication.id)
        .bind(&publication.session_id)
        .bind(&publication.owner_id)
        .bind(&publication.kind)
        .bind(&publication.title)
        .bind(content)
        .bind(&publication.staging_path)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|error| ArtifactPublicationRepositoryError::Unavailable(anyhow::Error::new(error)))?;
        Ok(ArtifactPublication {
            id: publication.id,
            session_id: publication.session_id,
            owner_id: publication.owner_id,
            kind: publication.kind,
            title: publication.title,
            status: ArtifactPublicationStatus::Publishing,
            content: publication.content,
            staging_path: Some(publication.staging_path),
            final_path: None,
            error: None,
            version: 1,
            created_at: now.clone(),
            updated_at: now,
        })
    }

    async fn find(
        &self,
        id: &str,
    ) -> Result<Option<ArtifactPublication>, ArtifactPublicationRepositoryError> {
        let row = sqlx::query("SELECT * FROM artifact_publications WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| {
                ArtifactPublicationRepositoryError::Unavailable(anyhow::Error::new(error))
            })?;
        row.as_ref().map(mysql_artifact_publication).transpose()
    }

    async fn finalize(
        &self,
        id: &str,
        finalization: ArtifactFinalization,
    ) -> Result<Option<ArtifactPublication>, ArtifactPublicationRepositoryError> {
        let content = serde_json::to_string(&finalization.content)
            .map_err(|error| ArtifactPublicationRepositoryError::InvalidData(error.to_string()))?;
        let now = chrono::Utc::now().to_rfc3339();
        let mut transaction = self.pool.begin().await.map_err(|error| {
            ArtifactPublicationRepositoryError::Unavailable(anyhow::Error::new(error))
        })?;
        let result = sqlx::query("UPDATE artifact_publications SET status = 'ready', content = ?, final_path = ?, staging_path = NULL, error = NULL, updated_at = ? WHERE id = ? AND status = 'publishing'")
            .bind(content)
            .bind(finalization.final_path)
            .bind(&now)
            .bind(id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| ArtifactPublicationRepositoryError::Unavailable(anyhow::Error::new(error)))?;
        if result.rows_affected() == 0 {
            transaction.rollback().await.map_err(|error| {
                ArtifactPublicationRepositoryError::Unavailable(anyhow::Error::new(error))
            })?;
            return Ok(None);
        }
        let row = sqlx::query("SELECT * FROM artifact_publications WHERE id = ?")
            .bind(id)
            .fetch_one(&mut *transaction)
            .await
            .map_err(|error| {
                ArtifactPublicationRepositoryError::Unavailable(anyhow::Error::new(error))
            })?;
        let publication = mysql_artifact_publication(&row)?;
        let session_exists: i64 =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM sessions WHERE id = ?)")
                .bind(&publication.session_id)
                .fetch_one(&mut *transaction)
                .await
                .map_err(|error| {
                    ArtifactPublicationRepositoryError::Unavailable(anyhow::Error::new(error))
                })?;
        if session_exists != 0 {
            let payload: Option<String> = sqlx::query_scalar(
                "SELECT payload FROM session_artifacts WHERE session_id = ? FOR UPDATE",
            )
            .bind(&publication.session_id)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(|error| {
                ArtifactPublicationRepositoryError::Unavailable(anyhow::Error::new(error))
            })?;
            let mut artifacts: Vec<ConversationArtifact> = payload
                .map(|payload| serde_json::from_str(&payload))
                .transpose()
                .map_err(|error| {
                    ArtifactPublicationRepositoryError::InvalidData(error.to_string())
                })?
                .unwrap_or_default();
            artifacts.retain(|artifact| artifact.id != publication.id);
            artifacts.push(ConversationArtifact {
                id: publication.id.clone(),
                kind: publication.kind.clone(),
                tool_kind: publication.kind.clone(),
                title: publication.title.clone(),
                status: publication.status.as_str().into(),
                content: publication.content.clone(),
                version: publication.version,
                created_at: publication.created_at.clone(),
                updated_at: publication.updated_at.clone(),
            });
            let payload = serde_json::to_string(&artifacts).map_err(|error| {
                ArtifactPublicationRepositoryError::InvalidData(error.to_string())
            })?;
            sqlx::query("INSERT INTO session_artifacts (id, session_id, payload, created_at, updated_at) VALUES (?, ?, ?, ?, ?) ON DUPLICATE KEY UPDATE payload = VALUES(payload), updated_at = VALUES(updated_at)")
                .bind(uuid::Uuid::new_v4().to_string())
                .bind(&publication.session_id)
                .bind(payload)
                .bind(&now)
                .bind(&now)
                .execute(&mut *transaction)
                .await
                .map_err(|error| ArtifactPublicationRepositoryError::Unavailable(anyhow::Error::new(error)))?;
        }
        transaction.commit().await.map_err(|error| {
            ArtifactPublicationRepositoryError::Unavailable(anyhow::Error::new(error))
        })?;
        Ok(Some(publication))
    }

    async fn fail(
        &self,
        id: &str,
        error: &str,
    ) -> Result<bool, ArtifactPublicationRepositoryError> {
        let result = sqlx::query("UPDATE artifact_publications SET status = 'failed', error = ?, updated_at = ? WHERE id = ? AND status = 'publishing'")
            .bind(error)
            .bind(chrono::Utc::now().to_rfc3339())
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|source| ArtifactPublicationRepositoryError::Unavailable(anyhow::Error::new(source)))?;
        Ok(result.rows_affected() > 0)
    }

    async fn pending(
        &self,
    ) -> Result<Vec<ArtifactPublication>, ArtifactPublicationRepositoryError> {
        let rows = sqlx::query("SELECT * FROM artifact_publications WHERE status = 'publishing' ORDER BY created_at ASC")
            .fetch_all(&self.pool)
            .await
            .map_err(|error| ArtifactPublicationRepositoryError::Unavailable(anyhow::Error::new(error)))?;
        rows.iter().map(mysql_artifact_publication).collect()
    }
}
