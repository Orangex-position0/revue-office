use std::path::PathBuf;
use std::sync::Arc;

use crate::agent::runtime::AgentRuntime;
use crate::application::artifact_service::ArtifactService;
use crate::application::chat_service::ChatApplicationService;
use crate::application::session_service::SessionApplicationService;
use crate::capabilities::presentation::PresentationCapability;
use crate::infrastructure::agent::LegacyAgentRuntimeDriver;
use crate::infrastructure::filesystem::artifact_storage::LocalArtifactStorage;
use crate::infrastructure::llm::agent::ConfiguredAgentLlmProvider;
use crate::infrastructure::llm::presentation::ConfiguredPresentationLlm;
use crate::infrastructure::persistence::mysql::MySqlSessionRepository;
use crate::infrastructure::persistence::sqlite::SqliteSessionRepository;
use crate::infrastructure::presentation_export::PptxPresentationExporter;
use crate::infrastructure::presentation_store::local::LocalPresentationStore;
use crate::ports::repositories::artifact_publication::ArtifactPublicationRepository;
use crate::ports::repositories::session::SessionRepository;

pub struct ApplicationServices {
    pub session: Arc<SessionApplicationService>,
    pub chat: Arc<ChatApplicationService>,
}

pub async fn build_application_services(
    database_url: &str,
    max_connections: u32,
    artifact_root: impl Into<PathBuf>,
    runtime_timeout: std::time::Duration,
) -> anyhow::Result<ApplicationServices> {
    let (session_repository, artifact_repository): (
        Arc<dyn SessionRepository>,
        Arc<dyn ArtifactPublicationRepository>,
    ) = if database_url.starts_with("mysql://") {
        let repository =
            Arc::new(MySqlSessionRepository::connect(database_url, max_connections).await?);
        (repository.clone(), repository)
    } else {
        let repository =
            Arc::new(SqliteSessionRepository::connect(database_url, max_connections).await?);
        (repository.clone(), repository)
    };
    let artifact = Arc::new(ArtifactService::new(
        artifact_repository,
        Arc::new(LocalArtifactStorage::new(artifact_root)),
    ));
    let reconciliation = artifact.reconcile_pending().await?;
    if reconciliation.recovered > 0 || reconciliation.failed > 0 {
        tracing::info!(
            recovered = reconciliation.recovered,
            failed = reconciliation.failed,
            "reconciled pending artifact publications"
        );
    }
    let runtime = Arc::new(AgentRuntime::with_timeout(
        Arc::new(LegacyAgentRuntimeDriver::new(Arc::new(
            ConfiguredAgentLlmProvider,
        ))),
        256,
        runtime_timeout,
    ));
    let presentation = Arc::new(PresentationCapability::new(
        Arc::new(ConfiguredPresentationLlm),
        Arc::new(LocalPresentationStore),
    ));
    crate::agent::tools::register_all_tools(presentation, Arc::new(PptxPresentationExporter)).await;

    Ok(ApplicationServices {
        session: Arc::new(SessionApplicationService::new(session_repository.clone())),
        chat: Arc::new(ChatApplicationService::with_artifact_service(
            session_repository,
            runtime,
            artifact,
            256,
        )),
    })
}
