use async_trait::async_trait;
use secrecy::{ExposeSecret, SecretString};
use sqlx::{MySqlPool, Row};

use crate::infrastructure::credentials::migration::PreferencePayloadStore;
use crate::providers::credentials::CredentialError;

pub struct MySqlPreferencePayloadStore {
    pool: MySqlPool,
}

impl MySqlPreferencePayloadStore {
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
}

#[async_trait]
impl PreferencePayloadStore for MySqlPreferencePayloadStore {
    async fn read(&self, actor: &str) -> Result<Option<SecretString>, CredentialError> {
        let row = sqlx::query(
            "SELECT CAST(payload AS CHAR) AS payload FROM user_settings WHERE user_id = ?",
        )
        .bind(actor)
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| CredentialError::Persistence)?;
        row.map(|row| {
            row.try_get::<String, _>("payload")
                .map(SecretString::new)
                .map_err(|_| CredentialError::Persistence)
        })
        .transpose()
    }

    async fn compare_and_swap(
        &self,
        actor: &str,
        expected: Option<&SecretString>,
        replacement: &str,
    ) -> Result<(), CredentialError> {
        let now = chrono::Utc::now().to_rfc3339();
        let result = if let Some(expected) = expected {
            sqlx::query("UPDATE user_settings SET payload = ?, updated_at = ? WHERE user_id = ? AND BINARY CAST(payload AS CHAR) = BINARY ?")
                .bind(replacement)
                .bind(&now)
                .bind(actor)
                .bind(expected.expose_secret())
                .execute(&self.pool)
                .await
                .map_err(|_| CredentialError::Persistence)?
        } else {
            sqlx::query("INSERT INTO user_settings (id, user_id, payload, created_at, updated_at) VALUES (?, ?, ?, ?, ?)")
                .bind(uuid::Uuid::new_v4().to_string())
                .bind(actor)
                .bind(replacement)
                .bind(&now)
                .bind(&now)
                .execute(&self.pool)
                .await
                .map_err(|error| {
                    if error.as_database_error().is_some_and(|error| error.is_unique_violation()) {
                        CredentialError::Conflict
                    } else {
                        CredentialError::Persistence
                    }
                })?
        };
        if result.rows_affected() != 1 {
            return Err(CredentialError::Conflict);
        }
        Ok(())
    }

    async fn actor_ids(&self) -> Result<Vec<String>, CredentialError> {
        let rows = sqlx::query("SELECT user_id FROM user_settings")
            .fetch_all(&self.pool)
            .await
            .map_err(|_| CredentialError::Persistence)?;
        rows.into_iter()
            .map(|row| {
                row.try_get("user_id")
                    .map_err(|_| CredentialError::Persistence)
            })
            .collect()
    }
}
