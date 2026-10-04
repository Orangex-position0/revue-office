use std::net::SocketAddr;
use std::path::PathBuf;

use axum::Router;

use super::application::ApplicationGraph;
use super::config::AppConfig;
use super::infrastructure::IdentityInfrastructure;
use crate::transport::http::HttpState;
use crate::transport::http::auth::IdentityState;
use crate::transport::http::handlers::health::HealthInfo;
use crate::transport::http::router::{self, HttpConfig};

pub(crate) struct BootstrappedHttpServer {
    pub router: Router,
    pub bind_addr: SocketAddr,
}

pub(crate) fn build(
    config: &AppConfig,
    identity: IdentityInfrastructure,
    applications: ApplicationGraph,
) -> BootstrappedHttpServer {
    let state = HttpState::new(
        IdentityState(identity.adapter),
        applications.identity,
        applications.chat,
        applications.sessions,
        applications.assets,
        applications.projects,
        applications.office_export,
        applications.preferences,
        applications.notifications,
        applications.dashboard,
    );
    let router = router::build(
        state,
        HttpConfig {
            output_root: PathBuf::from(&config.render_output_dir),
            health: HealthInfo {
                app_name: config.app_name.clone(),
                llm_model: config.llm_model.clone(),
                llm_provider: config.llm_provider.clone(),
            },
            cors: config.runtime.cors_layer(),
        },
    );
    BootstrappedHttpServer {
        router,
        bind_addr: config.runtime.bind_addr(),
    }
}
