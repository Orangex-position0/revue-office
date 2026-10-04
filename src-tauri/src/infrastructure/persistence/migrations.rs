use anyhow::Result;

pub(crate) async fn run_migrations_sqlite(pool: &sqlx::SqlitePool) -> Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS _revue_migrations (\
            version INTEGER PRIMARY KEY, \
            name TEXT NOT NULL, \
            applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP\
        )",
    )
    .execute(pool)
    .await?;

    let baseline_applied: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM _revue_migrations WHERE version = ?")
            .bind(1_i64)
            .fetch_one(pool)
            .await?;
    if baseline_applied == 0 {
        sqlx::raw_sql(include_str!("../../../../migrations/001_init.sql"))
            .execute(pool)
            .await?;
        sqlx::query("INSERT INTO _revue_migrations (version, name) VALUES (?, ?)")
            .bind(1_i64)
            .bind("baseline_schema")
            .execute(pool)
            .await?;
    }

    let order_column_applied: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM _revue_migrations WHERE version = ?")
            .bind(2_i64)
            .fetch_one(pool)
            .await?;
    if order_column_applied == 0 {
        let has_order_col: bool = sqlx::query_scalar(
            "SELECT COUNT(*) > 0 FROM pragma_table_info('sessions') WHERE name = 'order_col'",
        )
        .fetch_one(pool)
        .await?;

        if !has_order_col {
            sqlx::query("ALTER TABLE sessions ADD COLUMN order_col INTEGER NOT NULL DEFAULT 0")
                .execute(pool)
                .await?;
        }
        sqlx::query("INSERT INTO _revue_migrations (version, name) VALUES (?, ?)")
            .bind(2_i64)
            .bind("sessions_order_col")
            .execute(pool)
            .await?;
    }

    let artifact_publications_applied: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM _revue_migrations WHERE version = ?")
            .bind(3_i64)
            .fetch_one(pool)
            .await?;
    if artifact_publications_applied == 0 {
        sqlx::raw_sql(include_str!(
            "../../../../migrations/002_artifact_publications.sql"
        ))
        .execute(pool)
        .await?;
        sqlx::query("INSERT INTO _revue_migrations (version, name) VALUES (?, ?)")
            .bind(3_i64)
            .bind("artifact_publications")
            .execute(pool)
            .await?;
    }

    Ok(())
}

pub(crate) async fn run_migrations_mysql(pool: &sqlx::MySqlPool) -> Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS _revue_migrations (\
            version BIGINT PRIMARY KEY, \
            name VARCHAR(255) NOT NULL, \
            applied_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP\
        ) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4",
    )
    .execute(pool)
    .await?;

    let applied: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM _revue_migrations WHERE version = ?")
            .bind(1_i64)
            .fetch_one(pool)
            .await?;

    if applied == 0 {
        sqlx::raw_sql(include_str!("../../../../migrations/001_init_mysql.sql"))
            .execute(pool)
            .await?;
        sqlx::query("INSERT INTO _revue_migrations (version, name) VALUES (?, ?)")
            .bind(1_i64)
            .bind("baseline_schema")
            .execute(pool)
            .await?;
    }

    let artifact_publications_applied: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM _revue_migrations WHERE version = ?")
            .bind(3_i64)
            .fetch_one(pool)
            .await?;
    if artifact_publications_applied == 0 {
        sqlx::raw_sql(include_str!(
            "../../../../migrations/002_artifact_publications_mysql.sql"
        ))
        .execute(pool)
        .await?;
        sqlx::query("INSERT INTO _revue_migrations (version, name) VALUES (?, ?)")
            .bind(3_i64)
            .bind("artifact_publications")
            .execute(pool)
            .await?;
    }

    Ok(())
}

#[cfg(test)]
fn validate_mysql_test_database_url(database_url: &str, explicitly_enabled: bool) -> Result<()> {
    anyhow::ensure!(
        explicitly_enabled,
        "MySQL integration tests require an explicit opt-in"
    );

    let database_name = database_url
        .split('?')
        .next()
        .and_then(|url| url.rsplit('/').next())
        .unwrap_or_default();
    anyhow::ensure!(
        database_url.starts_with("mysql://")
            && (database_name.ends_with("_test") || database_name.starts_with("test_")),
        "MySQL integration tests require an isolated database named with test_ or _test"
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use sqlx::sqlite::SqlitePoolOptions;

    use super::run_migrations_sqlite;

    #[tokio::test]
    async fn migration_safety_sqlite_records_version_and_preserves_existing_data() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory SQLite should connect");

        run_migrations_sqlite(&pool)
            .await
            .expect("first migration run should succeed");
        sqlx::query(
            "INSERT INTO users (id, username, password_hash, created_at, updated_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("sentinel-user")
        .bind("sentinel")
        .bind("not-a-real-hash")
        .bind("2026-09-26T00:00:00Z")
        .bind("2026-09-26T00:00:00Z")
        .execute(&pool)
        .await
        .expect("sentinel insert should succeed");

        run_migrations_sqlite(&pool)
            .await
            .expect("repeated migration run should succeed");

        let username: String = sqlx::query_scalar("SELECT username FROM users WHERE id = ?")
            .bind("sentinel-user")
            .fetch_one(&pool)
            .await
            .expect("repeated migration must preserve existing rows");
        let applied: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM _revue_migrations WHERE version = 1")
                .fetch_one(&pool)
                .await
                .expect("migration ledger should exist");

        assert_eq!(username, "sentinel");
        assert_eq!(applied, 1, "baseline migration must be recorded once");
    }

    #[test]
    fn migration_safety_rejects_unisolated_mysql_test_targets() {
        assert!(
            super::validate_mysql_test_database_url(
                "mysql://user:secret@localhost/revue_office_test",
                false,
            )
            .is_err()
        );
        assert!(
            super::validate_mysql_test_database_url(
                "mysql://user:secret@localhost/revue_office",
                true,
            )
            .is_err()
        );
        assert!(
            super::validate_mysql_test_database_url(
                "mysql://user:secret@localhost/revue_office_test",
                true,
            )
            .is_ok()
        );
    }

    #[tokio::test]
    #[ignore = "requires REVUE_ALLOW_MYSQL_TEST=1 and MYSQL_TEST_DATABASE_URL"]
    async fn migration_safety_mysql_preserves_existing_data() {
        let database_url = std::env::var("MYSQL_TEST_DATABASE_URL")
            .expect("MYSQL_TEST_DATABASE_URL must point to an isolated test database");
        let explicitly_enabled = std::env::var("REVUE_ALLOW_MYSQL_TEST").as_deref() == Ok("1");
        super::validate_mysql_test_database_url(&database_url, explicitly_enabled)
            .expect("MySQL test target must be explicitly enabled and isolated");

        let pool = sqlx::mysql::MySqlPoolOptions::new()
            .max_connections(1)
            .connect(&database_url)
            .await
            .expect("isolated MySQL should connect");
        super::run_migrations_mysql(&pool)
            .await
            .expect("first migration run should succeed");
        sqlx::query("DELETE FROM users WHERE id = ?")
            .bind("migration-sentinel")
            .execute(&pool)
            .await
            .expect("stale sentinel cleanup should succeed");
        sqlx::query(
            "INSERT INTO users (id, username, password_hash, created_at, updated_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("migration-sentinel")
        .bind("migration-sentinel")
        .bind("not-a-real-hash")
        .bind("2026-09-26T00:00:00Z")
        .bind("2026-09-26T00:00:00Z")
        .execute(&pool)
        .await
        .expect("sentinel insert should succeed");

        super::run_migrations_mysql(&pool)
            .await
            .expect("repeated migration run should succeed");

        let username: String = sqlx::query_scalar("SELECT username FROM users WHERE id = ?")
            .bind("migration-sentinel")
            .fetch_one(&pool)
            .await
            .expect("repeated migration must preserve existing rows");
        assert_eq!(username, "migration-sentinel");
    }

    #[test]
    fn migration_safety_mysql_init_is_non_destructive_and_repeatable() {
        let migration =
            include_str!("../../../../migrations/001_init_mysql.sql").to_ascii_uppercase();

        assert!(
            !migration.contains("DROP TABLE"),
            "MySQL initialization must never drop application tables"
        );

        let create_table_count = migration.matches("CREATE TABLE").count();
        let guarded_create_count = migration.matches("CREATE TABLE IF NOT EXISTS").count();
        assert_eq!(
            create_table_count, guarded_create_count,
            "every MySQL CREATE TABLE must be safe to run repeatedly"
        );
    }
}
