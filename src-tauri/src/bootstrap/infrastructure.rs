use std::sync::Arc;

use crate::application::identity::{
    AccountRepository, IdentityAdapter, IdentityApplicationService,
};
use crate::bootstrap::config::AppConfig;
use crate::infrastructure::identity::{AuthEndpoints, DatabaseUserLookup, JwtIdentityAdapter};
use crate::infrastructure::persistence::mysql::identity::MySqlAccountRepository;
use crate::infrastructure::persistence::sqlite::identity::SqliteAccountRepository;
use crate::providers::credentials::{CredentialPurpose, CredentialStore};

pub(crate) struct IdentityInfrastructure {
    pub adapter: Arc<dyn IdentityAdapter>,
    pub application: Arc<IdentityApplicationService>,
}

pub(crate) async fn build_credentials(
    config: &AppConfig,
) -> anyhow::Result<Arc<dyn CredentialStore>> {
    crate::infrastructure::credentials::build_store(
        config.runtime.credential_backend(),
        &[
            (CredentialPurpose::Chat, config.llm_base_url.as_str()),
            (CredentialPurpose::Image, config.llm_image_base_url.as_str()),
            (CredentialPurpose::Video, config.llm_video_base_url.as_str()),
            (
                CredentialPurpose::WebSearch,
                config.baidu_mcp_sse_endpoint.as_str(),
            ),
        ],
    )
    .await
    .map_err(anyhow::Error::new)
}

pub(crate) async fn build_identity(config: &AppConfig) -> anyhow::Result<IdentityInfrastructure> {
    let accounts: Arc<dyn AccountRepository> = if config.is_mysql() {
        Arc::new(
            MySqlAccountRepository::connect(&config.database_url, config.db_max_connections)
                .await?,
        )
    } else {
        Arc::new(
            SqliteAccountRepository::connect(&config.database_url, config.db_max_connections)
                .await?,
        )
    };
    let adapter = Arc::new(JwtIdentityAdapter::new(
        config.jwt_secret.clone(),
        config.runtime.identity_policy(),
        Arc::new(DatabaseUserLookup(accounts.clone())),
    ));
    let application = Arc::new(IdentityApplicationService::new(Arc::new(
        AuthEndpoints::new(
            accounts,
            config.runtime.identity_policy(),
            config.jwt_secret.clone(),
            config.jwt_expiry_hours,
        ),
    )));
    Ok(IdentityInfrastructure {
        adapter,
        application,
    })
}
