use super::DbPool;
use crate::error::AppResult;
use serde::{Deserialize, Serialize};
use sqlx::Row;

/// Legacy read model retained for dashboard and project routes that have not yet migrated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionRow {
    pub id: String,
    pub owner_id: String,
    pub project_id: Option<String>,
    pub tool_kind: Option<String>,
    pub title: String,
    pub summary: Option<String>,
    pub message_count: i64,
    pub order_col: i64,
    pub created_at: String,
    pub updated_at: String,
}

pub async fn list_by_owner(
    pool: &DbPool,
    owner_id: &str,
    limit: i64,
    query: Option<&str>,
) -> AppResult<Vec<SessionRow>> {
    let query = query
        .map(|item| format!("%{}%", item.trim()))
        .filter(|item| item != "%%");

    let rows = if let Some(query) = query {
        sqlx::query(
            "SELECT id, owner_id, project_id, tool_kind, title, CAST(summary AS CHAR) AS summary, message_count, order_col, created_at, updated_at
             FROM sessions
             WHERE owner_id = ? AND (title LIKE ? OR COALESCE(summary, '') LIKE ?)
             ORDER BY order_col ASC, updated_at DESC LIMIT ?",
        )
        .bind(owner_id)
        .bind(&query)
        .bind(&query)
        .bind(limit)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query(
            "SELECT id, owner_id, project_id, tool_kind, title, CAST(summary AS CHAR) AS summary, message_count, order_col, created_at, updated_at
             FROM sessions WHERE owner_id = ? ORDER BY order_col ASC, updated_at DESC LIMIT ?",
        )
        .bind(owner_id)
        .bind(limit)
        .fetch_all(pool)
        .await?
    };

    rows.into_iter()
        .map(|row| {
            Ok(SessionRow {
                id: row.try_get(0)?,
                owner_id: row.try_get(1)?,
                project_id: row.try_get(2)?,
                tool_kind: row.try_get(3)?,
                title: row.try_get(4)?,
                summary: row.try_get(5)?,
                message_count: row.try_get(6)?,
                order_col: row.try_get(7)?,
                created_at: row.try_get(8)?,
                updated_at: row.try_get(9)?,
            })
        })
        .collect()
}
