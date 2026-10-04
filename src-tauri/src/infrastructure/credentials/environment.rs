use crate::providers::credentials::{
    CredentialError, CredentialPurpose, CredentialScope, CredentialSet, CredentialStore,
};
use async_trait::async_trait;
use secrecy::SecretString;
use std::collections::HashMap;

pub trait CredentialEnvironment {
    fn secret(&self, name: &'static str) -> Option<SecretString>;
}
impl<F: Fn(&'static str) -> Option<SecretString>> CredentialEnvironment for F {
    fn secret(&self, name: &'static str) -> Option<SecretString> {
        self(name)
    }
}
pub struct ProcessCredentialEnvironment;
impl CredentialEnvironment for ProcessCredentialEnvironment {
    fn secret(&self, name: &'static str) -> Option<SecretString> {
        std::env::var(name).ok().map(SecretString::new)
    }
}

/// Immutable deployment snapshot. Only explicitly mapped startup profiles use it.
pub struct EnvironmentCredentialStore {
    values: HashMap<CredentialPurpose, CredentialSet>,
}
impl EnvironmentCredentialStore {
    pub(crate) fn bind_endpoints(
        &mut self,
        endpoints: &[(CredentialPurpose, &str)],
    ) -> Result<(), CredentialError> {
        for (purpose, endpoint) in endpoints {
            if let Some(value) = self.values.get_mut(purpose) {
                if !value.matches_endpoint(endpoint) {
                    return Err(CredentialError::InvalidInput);
                }
                value.bind_endpoint(endpoint);
            }
        }
        Ok(())
    }
    pub fn from_source(source: &dyn CredentialEnvironment) -> Self {
        use secrecy::ExposeSecret;
        let mut values = HashMap::new();
        for (purpose, variable) in [
            (CredentialPurpose::Chat, "LLM_TEXT_API_KEY"),
            (CredentialPurpose::Image, "LLM_IMAGE_API_KEY"),
            (CredentialPurpose::Video, "LLM_VIDEO_API_KEY"),
            (CredentialPurpose::WebSearch, "AIPPT_BAIDU_MCP_API_KEY"),
        ] {
            if let Some(raw) = source.secret(variable) {
                let set = CredentialSet::new(
                    raw.expose_secret()
                        .split(',')
                        .map(|s| SecretString::new(s.to_owned())),
                );
                if !set.is_empty() {
                    values.insert(purpose, set);
                }
            }
        }
        Self { values }
    }
}
#[async_trait]
impl CredentialStore for EnvironmentCredentialStore {
    fn read_only(&self) -> bool {
        true
    }
    async fn resolve(
        &self,
        scope: &CredentialScope,
    ) -> Result<Option<CredentialSet>, CredentialError> {
        let provider = if scope.purpose == CredentialPurpose::WebSearch {
            "baidu-mcp"
        } else {
            "openai-compatible"
        };
        let mapped_profile = matches!(scope.profile_id.as_str(), "default" | "startup-env")
            || (scope.purpose == CredentialPurpose::WebSearch
                && scope.profile_id == "builtin-baidu-ai-search");
        if !mapped_profile || scope.provider_id != provider {
            return Ok(None);
        }
        Ok(self
            .values
            .get(&scope.purpose)
            .map(CredentialSet::copy_for_store))
    }
    async fn replace(&self, _: &CredentialScope, _: CredentialSet) -> Result<(), CredentialError> {
        Err(CredentialError::ReadOnly)
    }
    async fn delete(&self, _: &CredentialScope) -> Result<(), CredentialError> {
        Err(CredentialError::ReadOnly)
    }
}
