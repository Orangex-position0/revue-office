use std::sync::{Arc, OnceLock};

use crate::application::session_service::SessionApplicationService;

static SESSION_SERVICE: OnceLock<Arc<SessionApplicationService>> = OnceLock::new();

pub fn set_session_service(service: Arc<SessionApplicationService>) -> anyhow::Result<()> {
    SESSION_SERVICE
        .set(service)
        .map_err(|_| anyhow::anyhow!("session service already initialized"))
}

pub fn session_service() -> Arc<SessionApplicationService> {
    SESSION_SERVICE
        .get()
        .expect("session service not initialized")
        .clone()
}
