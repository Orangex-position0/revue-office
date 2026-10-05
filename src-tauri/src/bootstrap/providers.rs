use crate::application::preferences::{AppSettings, LlmProfileConfig};
use crate::infrastructure::credentials::migration::PreferenceCredentialService;
use crate::providers::{
    ChatProviderResolver, OpenAiChatConfig, ProviderError, ResolvedChatProvider, openai_compatible,
};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub(crate) struct DefaultChatProviderConfig {
    pub endpoint: String,
    pub model: String,
    pub timeout: Duration,
}
struct CachedProvider {
    endpoint: String,
    revision: String,
    provider: Arc<dyn crate::providers::ChatProvider>,
}
pub(crate) struct LegacySettingsProviderSelector {
    preferences: Arc<PreferenceCredentialService>,
    defaults: DefaultChatProviderConfig,
    providers: Mutex<HashMap<(String, String), CachedProvider>>,
}
impl LegacySettingsProviderSelector {
    pub(crate) fn new(
        preferences: Arc<PreferenceCredentialService>,
        defaults: DefaultChatProviderConfig,
    ) -> Self {
        Self {
            preferences,
            defaults,
            providers: Mutex::new(HashMap::new()),
        }
    }
}
#[async_trait]
impl ChatProviderResolver for LegacySettingsProviderSelector {
    async fn resolve(
        &self,
        user_id: &str,
        preferred_model: Option<&str>,
    ) -> Result<ResolvedChatProvider, ProviderError> {
        // Migration errors are not ignored and never fall back to plaintext.
        let settings = self
            .preferences
            .read(user_id)
            .await
            .map_err(|_| ProviderError::Unavailable)?
            .map(serde_json::from_value::<AppSettings>)
            .transpose()
            .map_err(|_| ProviderError::InvalidConfiguration)?;
        let (profile_id, endpoint, model) = settings
            .as_ref()
            .and_then(|settings| {
                settings
                    .llm_profiles
                    .iter()
                    .find(|p| p.id == settings.active_profile_id)
                    .map(|p| {
                        let endpoint = if p.base_url.trim().is_empty() {
                            self.defaults.endpoint.clone()
                        } else {
                            p.base_url.clone()
                        };
                        (
                            p.id.clone(),
                            endpoint,
                            select_model(p, preferred_model, Some(&settings.active_model)),
                        )
                    })
            })
            .unwrap_or_else(|| {
                (
                    "default".into(),
                    self.defaults.endpoint.clone(),
                    preferred_model
                        .filter(|m| is_chat_compatible_model(m))
                        .unwrap_or(&self.defaults.model)
                        .to_owned(),
                )
            });
        let credentials = self
            .preferences
            .chat_credentials(user_id, &profile_id, &endpoint)
            .await
            .map_err(|_| ProviderError::Unavailable)?
            .ok_or(ProviderError::InvalidConfiguration)?;
        let revision = credentials.revision().to_owned();
        let mut cache = self
            .providers
            .lock()
            .map_err(|_| ProviderError::Unavailable)?;
        let key = (user_id.to_owned(), profile_id);
        let provider = match cache.get(&key) {
            Some(cached) if cached.endpoint == endpoint && cached.revision == revision => {
                cached.provider.clone()
            }
            _ => {
                let provider = openai_compatible(OpenAiChatConfig {
                    endpoint: endpoint.clone(),
                    credentials,
                    timeout: self.defaults.timeout,
                })?;
                cache.insert(
                    key,
                    CachedProvider {
                        endpoint,
                        revision,
                        provider: provider.clone(),
                    },
                );
                provider
            }
        };
        Ok(ResolvedChatProvider { provider, model })
    }
}
fn select_model(
    profile: &LlmProfileConfig,
    preferred: Option<&str>,
    active: Option<&str>,
) -> String {
    preferred
        .filter(|m| profile.models.iter().any(|v| v == *m) && is_chat_compatible_model(m))
        .or_else(|| {
            active.filter(|m| profile.models.iter().any(|v| v == *m) && is_chat_compatible_model(m))
        })
        .map(str::to_owned)
        .or_else(|| {
            is_chat_compatible_model(&profile.default_model).then(|| profile.default_model.clone())
        })
        .or_else(|| {
            profile
                .models
                .iter()
                .find(|m| is_chat_compatible_model(m))
                .cloned()
        })
        .unwrap_or_else(|| profile.default_model.clone())
}
fn is_chat_compatible_model(model: &str) -> bool {
    let model = model.trim().to_lowercase();
    !(model.starts_with("agnes-image-") || model.starts_with("agnes-video-"))
}

#[cfg(test)]
mod credential_tests {
    use super::*;
    use crate::infrastructure::credentials::{
        memory::MemoryCredentialStore, migration::PreferencePayloadStore, startup_scope,
    };
    use crate::providers::credentials::{
        CredentialError, CredentialPurpose, CredentialSet, CredentialStore,
    };
    use secrecy::SecretString;
    use std::sync::atomic::{AtomicBool, Ordering};
    struct EmptyPreferences(AtomicBool);
    #[async_trait]
    impl PreferencePayloadStore for EmptyPreferences {
        async fn read(&self, _: &str) -> Result<Option<SecretString>, CredentialError> {
            if self.0.load(Ordering::Relaxed) {
                Err(CredentialError::Persistence)
            } else {
                Ok(None)
            }
        }
        async fn compare_and_swap(
            &self,
            _: &str,
            _: Option<&SecretString>,
            _: &str,
        ) -> Result<(), CredentialError> {
            Ok(())
        }
    }
    #[tokio::test]
    async fn credential_provider_cache_uses_revision_and_migration_errors_never_fallback() {
        let store = Arc::new(MemoryCredentialStore::default());
        let scope = startup_scope(CredentialPurpose::Chat);
        store
            .replace(
                &scope,
                CredentialSet::new([SecretString::new("synthetic-cache-first".into())]),
            )
            .await
            .unwrap();
        let source = Arc::new(EmptyPreferences(AtomicBool::new(false)));
        let preferences = Arc::new(PreferenceCredentialService::new(
            source.clone(),
            store.clone(),
            "https://example.invalid/v1".into(),
            "https://search.invalid/sse".into(),
        ));
        let selector = LegacySettingsProviderSelector::new(
            preferences,
            DefaultChatProviderConfig {
                endpoint: "https://example.invalid/v1".into(),
                model: "model".into(),
                timeout: Duration::from_secs(2),
            },
        );
        let first = selector.resolve("actor", None).await.unwrap();
        let same = selector
            .resolve("actor", Some("other-model"))
            .await
            .unwrap();
        assert!(Arc::ptr_eq(&first.provider, &same.provider));
        store
            .replace(
                &scope,
                CredentialSet::new([SecretString::new("synthetic-cache-second".into())]),
            )
            .await
            .unwrap();
        let rotated = selector.resolve("actor", None).await.unwrap();
        assert!(!Arc::ptr_eq(&first.provider, &rotated.provider));
        source.0.store(true, Ordering::Relaxed);
        assert!(matches!(
            selector.resolve("actor", None).await,
            Err(ProviderError::Unavailable)
        ));
    }
}
