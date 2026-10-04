use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use super::agent::build_office_agent;
use super::config::AppConfig;
use super::providers::{DefaultChatProviderConfig, LegacySettingsProviderSelector};
use crate::agent::tools::web_search::WebSearchConfig;
use crate::application::artifacts::{ArtifactPublicationRepository, ArtifactService};
use crate::application::assets::{AssetApplicationService, AssetRepository};
use crate::application::chat_service::ChatApplicationService;
use crate::application::conversations::SessionApplicationService;
use crate::application::conversations::SessionRepository;
use crate::application::dashboard::DashboardApplicationService;
use crate::application::identity::IdentityApplicationService;
use crate::application::notifications::{NotificationApplicationService, NotificationRepository};
use crate::application::office_export::OfficeExportApplicationService;
use crate::application::preferences::{PreferenceApplicationService, PreferenceDefaults};
use crate::application::projects::{ProjectApplicationService, ProjectRepository};
use crate::capabilities::presentation::PresentationCapability;
use crate::infrastructure::credentials::migration::{
    PreferenceCredentialService, PreferencePayloadStore,
};
use crate::infrastructure::export::files::LocalExportFileStore;
use crate::infrastructure::export::{
    DocxDocumentExporter, PptxPresentationExporter, XlsxSpreadsheetExporter,
};
use crate::infrastructure::extraction::OfficeAssetContentExtractor;
use crate::infrastructure::filesystem::artifact_storage::LocalArtifactStorage;
use crate::infrastructure::filesystem::assets::LocalAssetStorage;
use crate::infrastructure::persistence::mysql::MySqlSessionRepository;
use crate::infrastructure::persistence::mysql::assets::MySqlAssetRepository;
use crate::infrastructure::persistence::mysql::notifications::MySqlNotificationRepository;
use crate::infrastructure::persistence::mysql::preferences::MySqlPreferencePayloadStore;
use crate::infrastructure::persistence::mysql::projects::MySqlProjectRepository;
use crate::infrastructure::persistence::sqlite::SqliteSessionRepository;
use crate::infrastructure::persistence::sqlite::assets::SqliteAssetRepository;
use crate::infrastructure::persistence::sqlite::notifications::SqliteNotificationRepository;
use crate::infrastructure::persistence::sqlite::preferences::SqlitePreferencePayloadStore;
use crate::infrastructure::persistence::sqlite::projects::SqliteProjectRepository;
use crate::infrastructure::preferences::mcp_probe::HttpMcpConnectionTester;
use crate::infrastructure::presentation_planner::ConfiguredPresentationPlanner;
use crate::infrastructure::presentation_store::local::LocalPresentationStore;
use crate::providers::credentials::CredentialStore;

pub(crate) struct ApplicationGraph {
    pub identity: Arc<IdentityApplicationService>,
    pub sessions: Arc<SessionApplicationService>,
    pub chat: Arc<ChatApplicationService>,
    pub assets: Arc<AssetApplicationService>,
    pub projects: Arc<ProjectApplicationService>,
    pub office_export: Arc<OfficeExportApplicationService>,
    pub preferences: Arc<PreferenceApplicationService>,
    pub notifications: Arc<NotificationApplicationService>,
    pub dashboard: Arc<DashboardApplicationService>,
}

pub(crate) async fn build(
    config: &AppConfig,
    identity: Arc<IdentityApplicationService>,
    credential_store: Arc<dyn CredentialStore>,
) -> anyhow::Result<ApplicationGraph> {
    let preference_defaults = PreferenceDefaults::new(
        config.app_name.clone(),
        config.llm_base_url.clone(),
        config.llm_model.clone(),
        config
            .llm_text_models
            .iter()
            .chain(&config.llm_image_models)
            .chain(&config.llm_video_models)
            .cloned(),
        config.baidu_mcp_sse_endpoint.clone(),
    );
    let provider_defaults = DefaultChatProviderConfig {
        endpoint: preference_defaults.provider_endpoint.clone(),
        model: preference_defaults.provider_model.clone(),
        timeout: Duration::from_millis(config.llm_chat_timeout_ms),
    };
    let (session_repository, artifact_repository): (
        Arc<dyn SessionRepository>,
        Arc<dyn ArtifactPublicationRepository>,
    ) = if config.is_mysql() {
        let repository = Arc::new(
            MySqlSessionRepository::connect(&config.database_url, config.db_max_connections)
                .await?,
        );
        (repository.clone(), repository)
    } else {
        let repository = Arc::new(
            SqliteSessionRepository::connect(&config.database_url, config.db_max_connections)
                .await?,
        );
        (repository.clone(), repository)
    };
    let artifact = Arc::new(ArtifactService::new(
        artifact_repository,
        Arc::new(LocalArtifactStorage::new(
            Path::new(&config.data_dir).join("artifacts"),
        )),
    ));
    let reconciliation = artifact.reconcile_pending().await?;
    if reconciliation.recovered > 0 || reconciliation.failed > 0 {
        tracing::info!(
            recovered = reconciliation.recovered,
            failed = reconciliation.failed,
            "reconciled pending artifact publications"
        );
    }

    let sessions = Arc::new(SessionApplicationService::new(session_repository.clone()));
    let asset_repository: Arc<dyn AssetRepository> = if config.is_mysql() {
        Arc::new(
            MySqlAssetRepository::connect(&config.database_url, config.db_max_connections).await?,
        )
    } else {
        Arc::new(
            SqliteAssetRepository::connect(&config.database_url, config.db_max_connections).await?,
        )
    };
    let assets = Arc::new(AssetApplicationService::new(
        asset_repository,
        Arc::new(LocalAssetStorage::new(
            Path::new(&config.data_dir).join("files"),
        )),
        Arc::new(OfficeAssetContentExtractor::new()),
    ));
    let project_repository: Arc<dyn ProjectRepository> = if config.is_mysql() {
        Arc::new(
            MySqlProjectRepository::connect(&config.database_url, config.db_max_connections)
                .await?,
        )
    } else {
        Arc::new(
            SqliteProjectRepository::connect(&config.database_url, config.db_max_connections)
                .await?,
        )
    };
    let preference_payloads: Arc<dyn PreferencePayloadStore> = if config.is_mysql() {
        Arc::new(
            MySqlPreferencePayloadStore::connect(&config.database_url, config.db_max_connections)
                .await?,
        )
    } else {
        Arc::new(
            SqlitePreferencePayloadStore::connect(&config.database_url, config.db_max_connections)
                .await?,
        )
    };
    let credentials = Arc::new(PreferenceCredentialService::new(
        preference_payloads,
        credential_store.clone(),
        preference_defaults.provider_endpoint.clone(),
        preference_defaults.search_endpoint.clone(),
    ));
    credentials.migrate_all().await?;
    let preferences = Arc::new(PreferenceApplicationService::new(
        credentials.clone(),
        Arc::new(HttpMcpConnectionTester::new(credentials.clone())),
        preference_defaults,
    ));
    let provider_selector = Arc::new(LegacySettingsProviderSelector::new(
        credentials,
        provider_defaults,
    ));
    let presentation = Arc::new(PresentationCapability::new(
        Arc::new(ConfiguredPresentationPlanner::new(
            provider_selector.clone(),
        )),
        Arc::new(LocalPresentationStore::new(PathBuf::from(
            &config.projects_dir,
        ))),
        Arc::new(PptxPresentationExporter),
    ));
    let projects = Arc::new(ProjectApplicationService::new(
        project_repository,
        session_repository.clone(),
        presentation.clone(),
    ));
    let notification_repository: Arc<dyn NotificationRepository> = if config.is_mysql() {
        Arc::new(
            MySqlNotificationRepository::connect(&config.database_url, config.db_max_connections)
                .await?,
        )
    } else {
        Arc::new(
            SqliteNotificationRepository::connect(&config.database_url, config.db_max_connections)
                .await?,
        )
    };
    let notifications = Arc::new(NotificationApplicationService::new(notification_repository));
    let dashboard = Arc::new(DashboardApplicationService::new(
        projects.clone(),
        sessions.clone(),
        assets.clone(),
        notifications.clone(),
    ));
    let office_export = Arc::new(OfficeExportApplicationService::new(
        Arc::new(DocxDocumentExporter),
        Arc::new(XlsxSpreadsheetExporter),
        Arc::new(LocalExportFileStore::new(PathBuf::from(
            &config.render_output_dir,
        ))),
    ));
    let agent = build_office_agent(
        presentation,
        provider_selector,
        credential_store,
        WebSearchConfig {
            provider: config.web_search_provider.clone(),
            endpoint: config.web_search_endpoint.clone(),
            baidu_mcp_sse_endpoint: config.baidu_mcp_sse_endpoint.clone(),
            timeout: Duration::from_millis(config.web_search_timeout_ms),
        },
        Duration::from_millis(config.llm_tool_timeout_ms),
    )?;
    let chat = Arc::new(ChatApplicationService::with_artifact_service(
        session_repository,
        agent,
        artifact,
        256,
    ));

    Ok(ApplicationGraph {
        identity,
        sessions,
        chat,
        assets,
        projects,
        office_export,
        preferences,
        notifications,
        dashboard,
    })
}
