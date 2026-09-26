use std::sync::Arc;

use crate::application::session_service::SessionApplicationService;
use crate::db::DbPool;
use crate::infrastructure::persistence::legacy_session::LegacyAnySessionRepository;
use crate::infrastructure::persistence::sqlite::SqliteSessionRepository;
use crate::ports::repositories::session::SessionRepository;

pub async fn build_session_service(
    database_url: &str,
    max_connections: u32,
    legacy_pool: DbPool,
) -> anyhow::Result<Arc<SessionApplicationService>> {
    let repository: Arc<dyn SessionRepository> = if database_url.starts_with("mysql://") {
        Arc::new(LegacyAnySessionRepository::new(legacy_pool))
    } else {
        Arc::new(SqliteSessionRepository::connect(database_url, max_connections).await?)
    };
    Ok(Arc::new(SessionApplicationService::new(repository)))
}
