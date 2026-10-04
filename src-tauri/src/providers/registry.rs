use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;

use super::{ChatProvider, ProviderError};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProviderId(String);

impl ProviderId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone)]
pub struct ResolvedChatProvider {
    pub provider: Arc<dyn ChatProvider>,
    pub model: String,
}

#[async_trait]
pub trait ChatProviderResolver: Send + Sync {
    async fn resolve(
        &self,
        user_id: &str,
        preferred_model: Option<&str>,
    ) -> Result<ResolvedChatProvider, ProviderError>;
}

#[derive(Default)]
pub struct ProviderRegistry {
    providers: HashMap<ProviderId, Arc<dyn ChatProvider>>,
}

impl ProviderRegistry {
    pub fn new(providers: impl IntoIterator<Item = (ProviderId, Arc<dyn ChatProvider>)>) -> Self {
        Self {
            providers: providers.into_iter().collect(),
        }
    }

    pub fn get(&self, id: &ProviderId) -> Option<Arc<dyn ChatProvider>> {
        self.providers.get(id).cloned()
    }
}
