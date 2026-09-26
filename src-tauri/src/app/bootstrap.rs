use std::sync::Arc;

use crate::application::session_service::SessionApplicationService;
use crate::infrastructure::persistence::mysql::MySqlSessionRepository;
use crate::infrastructure::persistence::sqlite::SqliteSessionRepository;
use crate::ports::repositories::session::SessionRepository;

pub async fn build_session_service(
    database_url: &str,
    max_connections: u32,
) -> anyhow::Result<Arc<SessionApplicationService>> {
    let repository: Arc<dyn SessionRepository> = if database_url.starts_with("mysql://") {
        Arc::new(MySqlSessionRepository::connect(database_url, max_connections).await?)
    } else {
        Arc::new(SqliteSessionRepository::connect(database_url, max_connections).await?)
    };
    Ok(Arc::new(SessionApplicationService::new(repository)))
}
