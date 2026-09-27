use std::sync::Arc;

use crate::agent::runtime::AgentRuntime;
use crate::application::chat_service::ChatApplicationService;
use crate::application::session_service::SessionApplicationService;
use crate::infrastructure::agent::LegacyAgentRuntimeDriver;
use crate::infrastructure::persistence::mysql::MySqlSessionRepository;
use crate::infrastructure::persistence::sqlite::SqliteSessionRepository;
use crate::ports::repositories::session::SessionRepository;

pub struct ApplicationServices {
    pub session: Arc<SessionApplicationService>,
    pub chat: Arc<ChatApplicationService>,
}

pub async fn build_application_services(
    database_url: &str,
    max_connections: u32,
) -> anyhow::Result<ApplicationServices> {
    let repository: Arc<dyn SessionRepository> = if database_url.starts_with("mysql://") {
        Arc::new(MySqlSessionRepository::connect(database_url, max_connections).await?)
    } else {
        Arc::new(SqliteSessionRepository::connect(database_url, max_connections).await?)
    };
    let runtime = Arc::new(AgentRuntime::new(Arc::new(LegacyAgentRuntimeDriver), 256));

    Ok(ApplicationServices {
        session: Arc::new(SessionApplicationService::new(repository.clone())),
        chat: Arc::new(ChatApplicationService::new(repository, runtime, 256)),
    })
}
