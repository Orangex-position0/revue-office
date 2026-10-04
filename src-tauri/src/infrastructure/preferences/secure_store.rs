use async_trait::async_trait;

use crate::application::identity::ActorId;
use crate::application::preferences::{PreferenceError, PreferenceUpdate, SecurePreferencePort};
use crate::infrastructure::credentials::migration::PreferenceCredentialService;
use crate::providers::credentials::CredentialError;

pub(crate) fn map_credential_error(error: CredentialError) -> PreferenceError {
    match error {
        CredentialError::ReadOnly => PreferenceError::Forbidden,
        CredentialError::InvalidInput => PreferenceError::Invalid("设置或凭据格式无效".into()),
        CredentialError::Conflict => PreferenceError::Conflict,
        _ => PreferenceError::Unavailable,
    }
}

#[async_trait]
impl SecurePreferencePort for PreferenceCredentialService {
    async fn load(&self, actor: &ActorId) -> Result<Option<serde_json::Value>, PreferenceError> {
        PreferenceCredentialService::read(self, &actor.0)
            .await
            .map_err(map_credential_error)
    }

    async fn save(
        &self,
        actor: &ActorId,
        mut update: PreferenceUpdate,
    ) -> Result<serde_json::Value, PreferenceError> {
        PreferenceCredentialService::save(self, &actor.0, update.take())
            .await
            .map_err(map_credential_error)
    }

    async fn chat_credential_count(
        &self,
        actor: &ActorId,
        profile: &str,
        endpoint: &str,
    ) -> Result<usize, PreferenceError> {
        PreferenceCredentialService::chat_credentials(self, &actor.0, profile, endpoint)
            .await
            .map(|credentials| {
                credentials
                    .map(|credentials| credentials.len())
                    .unwrap_or(0)
            })
            .map_err(map_credential_error)
    }

    async fn has_search_credentials(
        &self,
        actor: &ActorId,
        profile: &str,
        endpoint: &str,
    ) -> Result<bool, PreferenceError> {
        PreferenceCredentialService::search_credentials(self, &actor.0, profile, endpoint)
            .await
            .map(|credentials| credentials.is_some_and(|credentials| !credentials.is_empty()))
            .map_err(map_credential_error)
    }
}
