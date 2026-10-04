use crate::providers::api::openai_chat::OpenAiChatProvider;
use crate::providers::credentials::CredentialSet;
use crate::providers::{ChatProvider, ProviderError};
use std::sync::Arc;
use std::time::Duration;

pub struct OpenAiChatConfig {
    pub endpoint: String,
    pub credentials: CredentialSet,
    pub timeout: Duration,
}
impl std::fmt::Debug for OpenAiChatConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenAiChatConfig")
            .field("endpoint", &"[configured endpoint]")
            .field("credentials", &self.credentials)
            .field("timeout", &self.timeout)
            .finish()
    }
}

pub fn openai_compatible(config: OpenAiChatConfig) -> Result<Arc<dyn ChatProvider>, ProviderError> {
    Ok(Arc::new(OpenAiChatProvider::new(config)?))
}
