use async_trait::async_trait;
use secrecy::SecretString;
use sqlx::{Row, SqlitePool};

use crate::application::identity::{Account, AccountRecord, AccountRepository, IdentityError};

pub struct SqliteAccountRepository {
    pool: SqlitePool,
}

impl SqliteAccountRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn connect(database_url: &str, max_connections: u32) -> anyhow::Result<Self> {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(max_connections)
            .connect(database_url)
            .await?;
        crate::infrastructure::persistence::migrations::run_migrations_sqlite(&pool).await?;
        Ok(Self::new(pool))
    }
}

fn account(row: &sqlx::sqlite::SqliteRow) -> Result<Account, sqlx::Error> {
    Ok(Account {
        id: row.try_get("id")?,
        username: row.try_get("username")?,
        email: row.try_get("email")?,
        avatar: row.try_get("avatar")?,
        role: row.try_get("role")?,
    })
}

#[async_trait]
impl AccountRepository for SqliteAccountRepository {
    async fn find_by_username(
        &self,
        username: &str,
    ) -> Result<Option<AccountRecord>, IdentityError> {
        let row = sqlx::query(
            "SELECT id, username, email, password_hash, avatar, role FROM users WHERE username = ?",
        )
        .bind(username)
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| IdentityError::Unauthenticated)?;
        row.map(|row| {
            Ok(AccountRecord {
                account: account(&row).map_err(|_| IdentityError::Unauthenticated)?,
                password_hash: SecretString::new(
                    row.try_get("password_hash")
                        .map_err(|_| IdentityError::Unauthenticated)?,
                ),
            })
        })
        .transpose()
    }

    async fn find_by_id(&self, id: &str) -> Result<Option<Account>, IdentityError> {
        sqlx::query("SELECT id, username, email, avatar, role FROM users WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|_| IdentityError::Unauthenticated)?
            .map(|row| account(&row).map_err(|_| IdentityError::Unauthenticated))
            .transpose()
    }

    async fn create(
        &self,
        username: &str,
        email: Option<&str>,
        password_hash: SecretString,
    ) -> Result<Account, IdentityError> {
        use secrecy::ExposeSecret;
        let account = Account {
            id: uuid::Uuid::new_v4().to_string(),
            username: username.to_owned(),
            email: email.map(str::to_owned),
            avatar: None,
            role: "user".into(),
        };
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO users (id, username, email, password_hash, role, created_at, updated_at) VALUES (?, ?, ?, ?, 'user', ?, ?)",
        )
        .bind(&account.id)
        .bind(&account.username)
        .bind(&account.email)
        .bind(password_hash.expose_secret())
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|_| IdentityError::Unauthenticated)?;
        Ok(account)
    }

    async fn find_or_create_external(&self, username: &str) -> Result<Account, IdentityError> {
        if let Some(record) = self.find_by_username(username).await? {
            return Ok(record.account);
        }
        let hash = bcrypt::hash(uuid::Uuid::new_v4().to_string(), 10)
            .map_err(|_| IdentityError::Unauthenticated)?;
        self.create(username, None, SecretString::new(hash)).await
    }
}
