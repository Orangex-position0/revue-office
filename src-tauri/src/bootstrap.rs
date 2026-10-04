mod agent;
mod application;
pub mod config;
mod http;
mod infrastructure;
mod providers;

use config::{AppConfig, BootstrapError};
use http::BootstrappedHttpServer;

pub(crate) async fn build(config: AppConfig) -> Result<BootstrappedHttpServer, BootstrapError> {
    config.ensure_dirs().map_err(BootstrapError::Startup)?;
    let credentials = infrastructure::build_credentials(&config)
        .await
        .map_err(BootstrapError::Startup)?;
    let identity = infrastructure::build_identity(&config)
        .await
        .map_err(BootstrapError::Startup)?;
    let applications = application::build(&config, identity.application.clone(), credentials)
        .await
        .map_err(BootstrapError::Startup)?;
    Ok(http::build(&config, identity, applications))
}

pub async fn run() -> Result<(), BootstrapError> {
    let config = AppConfig::load().map_err(BootstrapError::Startup)?;
    tracing::info!(
        profile = ?config.runtime.profile(),
        bind = %config.runtime.bind_addr(),
        origin_count = config.runtime.cors_origins().len(),
        identity = ?config.runtime.identity_policy(),
        credentials = ?config.runtime.credential_backend(),
        "runtime profile validated"
    );
    tracing::info!(
        "🚀 {} API running at http://{}",
        config.app_name,
        config.runtime.bind_addr()
    );
    tracing::info!("📝 LLM Provider configured");
    tracing::info!("📂 Projects: {}", config.projects_dir);
    tracing::info!(
        "🗄️ Database: {}",
        if config.is_mysql() { "MySQL" } else { "SQLite" }
    );

    let server = build(config).await?;
    let listener = tokio::net::TcpListener::bind(server.bind_addr)
        .await
        .map_err(|error| BootstrapError::Startup(error.into()))?;
    axum::serve(listener, server.router)
        .await
        .map_err(|error| BootstrapError::Startup(error.into()))?;
    Ok(())
}
