use async_trait::async_trait;
use secrecy::{ExposeSecret, SecretString};
use subtle::ConstantTimeEq;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CredentialBackend {
    Native,
    Environment,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum CredentialPurpose {
    Chat,
    Image,
    Video,
    WebSearch,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct CredentialScope {
    pub actor_id: String,
    pub profile_id: String,
    pub provider_id: String,
    pub purpose: CredentialPurpose,
}
impl CredentialScope {
    pub fn chat(actor: &str, profile: &str) -> Self {
        Self {
            actor_id: actor.into(),
            profile_id: profile.into(),
            provider_id: "openai-compatible".into(),
            purpose: CredentialPurpose::Chat,
        }
    }
    pub fn web_search(actor: &str, profile: &str) -> Self {
        Self {
            actor_id: actor.into(),
            profile_id: profile.into(),
            provider_id: "baidu-mcp".into(),
            purpose: CredentialPurpose::WebSearch,
        }
    }
}

/// Not serializable or clonable. Revision is random metadata, never a key hash.
pub struct CredentialSet {
    values: Vec<SecretString>,
    revision: String,
    endpoint_binding: Option<String>,
}
impl CredentialSet {
    pub fn new(values: impl IntoIterator<Item = SecretString>) -> Self {
        let mut normalized: Vec<SecretString> = Vec::new();
        for value in values {
            let trimmed = value.expose_secret().trim();
            if !trimmed.is_empty() && !normalized.iter().any(|v| v.expose_secret() == trimmed) {
                normalized.push(SecretString::new(trimmed.to_owned()));
            }
        }
        Self {
            values: normalized,
            revision: uuid::Uuid::new_v4().to_string(),
            endpoint_binding: None,
        }
    }
    pub fn values(&self) -> &[SecretString] {
        &self.values
    }
    pub fn len(&self) -> usize {
        self.values.len()
    }
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
    pub fn revision(&self) -> &str {
        &self.revision
    }
    pub fn endpoint_binding(&self) -> Option<&str> {
        self.endpoint_binding.as_deref()
    }
    pub fn bind_endpoint(&mut self, endpoint: &str) {
        self.endpoint_binding = Some(endpoint.trim_end_matches('/').to_owned());
    }
    pub fn matches_endpoint(&self, endpoint: &str) -> bool {
        self.endpoint_binding()
            .is_none_or(|bound| bound == endpoint.trim_end_matches('/'))
            && !self
                .values
                .iter()
                .any(|key| endpoint.contains(key.expose_secret().as_str()))
    }
    pub fn same_values(&self, other: &Self) -> bool {
        let mut equal = (self.len() == other.len()) as u8;
        for (a, b) in self.values.iter().zip(&other.values) {
            equal &= a
                .expose_secret()
                .as_bytes()
                .ct_eq(b.expose_secret().as_bytes())
                .unwrap_u8();
        }
        equal == 1
    }
    /// Used only by store adapters to return an independently zeroizing record.
    pub fn copy_for_store(&self) -> Self {
        Self {
            values: self
                .values
                .iter()
                .map(|v| SecretString::new(v.expose_secret().to_owned()))
                .collect(),
            revision: self.revision.clone(),
            endpoint_binding: self.endpoint_binding.clone(),
        }
    }
    pub(crate) fn restored(
        values: Vec<SecretString>,
        revision: String,
        endpoint_binding: Option<String>,
    ) -> Self {
        Self {
            values,
            revision,
            endpoint_binding,
        }
    }
}
impl std::fmt::Debug for CredentialSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CredentialSet")
            .field("values", &format_args!("[REDACTED; {}]", self.len()))
            .finish()
    }
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CredentialError {
    #[error("credential backend unavailable; unlock/configure the selected secure backend")]
    Unavailable,
    #[error("credential backend is deployment-owned and read-only")]
    ReadOnly,
    #[error("credential verification failed; legacy preferences were not cleared")]
    Verification,
    #[error("credential migration could not persist or verify scrubbed preferences; retry safely")]
    Persistence,
    #[error("invalid credential scope or preference input")]
    InvalidInput,
    #[error("credential preference changed concurrently; retry safely")]
    Conflict,
}

#[async_trait]
pub trait CredentialStore: Send + Sync {
    fn read_only(&self) -> bool {
        false
    }
    async fn resolve(
        &self,
        scope: &CredentialScope,
    ) -> Result<Option<CredentialSet>, CredentialError>;
    async fn replace(
        &self,
        scope: &CredentialScope,
        value: CredentialSet,
    ) -> Result<(), CredentialError>;
    async fn delete(&self, scope: &CredentialScope) -> Result<(), CredentialError>;
}
