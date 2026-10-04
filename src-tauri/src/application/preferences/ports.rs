use async_trait::async_trait;
use serde_json::Value;

use crate::application::identity::ActorId;

use super::{McpConnectionRequest, PreferenceError, PreferenceUpdate};

#[async_trait]
pub trait SecurePreferencePort: Send + Sync {
    async fn load(&self, actor: &ActorId) -> Result<Option<Value>, PreferenceError>;
    async fn save(
        &self,
        actor: &ActorId,
        update: PreferenceUpdate,
    ) -> Result<Value, PreferenceError>;
    async fn chat_credential_count(
        &self,
        actor: &ActorId,
        profile: &str,
        endpoint: &str,
    ) -> Result<usize, PreferenceError>;
    async fn has_search_credentials(
        &self,
        actor: &ActorId,
        profile: &str,
        endpoint: &str,
    ) -> Result<bool, PreferenceError>;
}

#[async_trait]
pub trait McpConnectionTester: Send + Sync {
    async fn test(
        &self,
        actor: &ActorId,
        request: McpConnectionRequest,
    ) -> Result<Option<Vec<Value>>, PreferenceError>;
}
