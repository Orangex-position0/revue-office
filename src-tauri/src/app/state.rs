use std::sync::{Arc, OnceLock};

use crate::application::artifact_service::ArtifactService;
use crate::application::chat_service::ChatApplicationService;
use crate::application::session_service::SessionApplicationService;

static SESSION_SERVICE: OnceLock<Arc<SessionApplicationService>> = OnceLock::new();
static CHAT_SERVICE: OnceLock<Arc<ChatApplicationService>> = OnceLock::new();
static ARTIFACT_SERVICE: OnceLock<Arc<ArtifactService>> = OnceLock::new();

pub fn set_services(
    session: Arc<SessionApplicationService>,
    chat: Arc<ChatApplicationService>,
    artifact: Arc<ArtifactService>,
) -> anyhow::Result<()> {
    SESSION_SERVICE
        .set(session)
        .map_err(|_| anyhow::anyhow!("session service already initialized"))?;
    CHAT_SERVICE
        .set(chat)
        .map_err(|_| anyhow::anyhow!("chat service already initialized"))?;
    ARTIFACT_SERVICE
        .set(artifact)
        .map_err(|_| anyhow::anyhow!("artifact service already initialized"))
}

pub fn session_service() -> Arc<SessionApplicationService> {
    SESSION_SERVICE
        .get()
        .expect("session service not initialized")
        .clone()
}

pub fn chat_service() -> Arc<ChatApplicationService> {
    CHAT_SERVICE
        .get()
        .expect("chat service not initialized")
        .clone()
}

pub fn artifact_service() -> Arc<ArtifactService> {
    ARTIFACT_SERVICE
        .get()
        .expect("artifact service not initialized")
        .clone()
}
