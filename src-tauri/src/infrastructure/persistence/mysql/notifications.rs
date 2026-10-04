use async_trait::async_trait;
use sqlx::{MySqlPool, Row};

use crate::application::identity::ActorId;
use crate::application::notifications::{
    Notification, NotificationError, NotificationId, NotificationQuery, NotificationRepository,
};

pub struct MySqlNotificationRepository {
    pool: MySqlPool,
}

impl MySqlNotificationRepository {
    pub fn new(pool: MySqlPool) -> Self {
        Self { pool }
    }

    pub async fn connect(database_url: &str, max_connections: u32) -> anyhow::Result<Self> {
        let pool = sqlx::mysql::MySqlPoolOptions::new()
            .max_connections(max_connections)
            .connect(database_url)
            .await?;
        crate::infrastructure::persistence::migrations::run_migrations_mysql(&pool).await?;
        Ok(Self::new(pool))
    }

    fn error(error: sqlx::Error) -> NotificationError {
        NotificationError::Repository(anyhow::Error::new(error))
    }

    fn notification(row: &sqlx::mysql::MySqlRow) -> Result<Notification, NotificationError> {
        let is_read: i8 = row.try_get("is_read").map_err(Self::error)?;
        Ok(Notification {
            id: row.try_get("id").map_err(Self::error)?,
            user_id: row.try_get("user_id").map_err(Self::error)?,
            notification_type: row.try_get("type").map_err(Self::error)?,
            title: row.try_get("title").map_err(Self::error)?,
            content: row.try_get("content").map_err(Self::error)?,
            is_read: is_read != 0,
            link: row.try_get("link").map_err(Self::error)?,
            created_at: row.try_get("created_at").map_err(Self::error)?,
        })
    }
}

const COLUMNS: &str = "id, user_id, type, title, content, is_read, link, created_at";

#[async_trait]
impl NotificationRepository for MySqlNotificationRepository {
    async fn list(&self, query: NotificationQuery) -> Result<Vec<Notification>, NotificationError> {
        let sql = if query.unread_only {
            format!(
                "SELECT {COLUMNS} FROM notifications WHERE user_id = ? AND is_read = 0 ORDER BY created_at DESC LIMIT ? OFFSET ?"
            )
        } else {
            format!(
                "SELECT {COLUMNS} FROM notifications WHERE user_id = ? ORDER BY created_at DESC LIMIT ? OFFSET ?"
            )
        };
        sqlx::query(&sql)
            .bind(&query.owner_id.0)
            .bind(query.limit())
            .bind(query.offset())
            .fetch_all(&self.pool)
            .await
            .map_err(Self::error)?
            .iter()
            .map(Self::notification)
            .collect()
    }

    async fn unread_count(&self, owner: &ActorId) -> Result<i64, NotificationError> {
        sqlx::query_scalar(
            "SELECT CAST(COUNT(*) AS SIGNED) FROM notifications WHERE user_id = ? AND is_read = 0",
        )
        .bind(&owner.0)
        .fetch_one(&self.pool)
        .await
        .map_err(Self::error)
    }

    async fn mark_read(
        &self,
        owner: &ActorId,
        id: &NotificationId,
    ) -> Result<bool, NotificationError> {
        Ok(
            sqlx::query("UPDATE notifications SET is_read = 1 WHERE id = ? AND user_id = ?")
                .bind(&id.0)
                .bind(&owner.0)
                .execute(&self.pool)
                .await
                .map_err(Self::error)?
                .rows_affected()
                > 0,
        )
    }

    async fn mark_all_read(&self, owner: &ActorId) -> Result<(), NotificationError> {
        sqlx::query("UPDATE notifications SET is_read = 1 WHERE user_id = ?")
            .bind(&owner.0)
            .execute(&self.pool)
            .await
            .map_err(Self::error)?;
        Ok(())
    }

    async fn delete(
        &self,
        owner: &ActorId,
        id: &NotificationId,
    ) -> Result<bool, NotificationError> {
        Ok(
            sqlx::query("DELETE FROM notifications WHERE id = ? AND user_id = ?")
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
