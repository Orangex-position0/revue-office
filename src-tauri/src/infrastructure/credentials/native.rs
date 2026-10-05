use crate::providers::credentials::{
    CredentialError, CredentialScope, CredentialSet, CredentialStore,
};
use async_trait::async_trait;
use secrecy::{ExposeSecret, SecretString};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

pub struct NativeCredentialStore {
    service: String,
}
impl NativeCredentialStore {
    pub fn new(service: impl Into<String>) -> Self {
        Self {
            service: service.into(),
        }
    }
    fn account(scope: &CredentialScope) -> String {
        let mut hash = Sha256::new();
        for field in [&scope.actor_id, &scope.profile_id, &scope.provider_id] {
            hash.update((field.len() as u64).to_be_bytes());
            hash.update(field.as_bytes());
        }
        hash.update([match scope.purpose {
            crate::providers::credentials::CredentialPurpose::Chat => 0,
            crate::providers::credentials::CredentialPurpose::Image => 1,
            crate::providers::credentials::CredentialPurpose::Video => 2,
            crate::providers::credentials::CredentialPurpose::WebSearch => 3,
        }]);
        format!("{:x}", hash.finalize())
    }
}

/// Only the native-store adapter encodes/decodes its versioned secret payload.
/// CredentialSet itself intentionally has no Serialize implementation.
fn encode(value: &CredentialSet) -> Result<Zeroizing<String>, CredentialError> {
    let values: Vec<&str> = value
        .values()
        .iter()
        .map(ExposeSecret::expose_secret)
        .map(String::as_str)
        .collect();
    serde_json::to_string(&(1u8, value.revision(), value.endpoint_binding(), values))
        .map(Zeroizing::new)
        .map_err(|_| CredentialError::Unavailable)
}
fn decode(raw: String) -> Result<CredentialSet, CredentialError> {
    let raw = Zeroizing::new(raw);
    let (version, revision, binding, values): (u8, String, Option<String>, Vec<String>) =
        serde_json::from_str(&raw).map_err(|_| CredentialError::Unavailable)?;
    let values = values
        .into_iter()
        .map(SecretString::new)
        .collect::<Vec<_>>();
    if version != 1 || revision.is_empty() {
        return Err(CredentialError::Unavailable);
    }
    Ok(CredentialSet::restored(values, revision, binding))
}
#[async_trait]
impl CredentialStore for NativeCredentialStore {
    async fn resolve(
        &self,
        scope: &CredentialScope,
    ) -> Result<Option<CredentialSet>, CredentialError> {
        let service = self.service.clone();
        let account = Self::account(scope);
        tokio::task::spawn_blocking(move || {
            match keyring::Entry::new(&service, &account).get_password() {
                Ok(raw) => decode(raw).map(Some),
                Err(keyring::Error::NoEntry) => Ok(None),
                Err(_) => Err(CredentialError::Unavailable),
            }
        })
        .await
        .map_err(|_| CredentialError::Unavailable)?
    }
    async fn replace(
        &self,
        scope: &CredentialScope,
        value: CredentialSet,
    ) -> Result<(), CredentialError> {
        let service = self.service.clone();
        let account = Self::account(scope);
        let payload = encode(&value)?;
        tokio::task::spawn_blocking(move || {
            keyring::Entry::new(&service, &account)
                .set_password(&payload)
                .map_err(|_| CredentialError::Unavailable)
        })
        .await
        .map_err(|_| CredentialError::Unavailable)?
    }
    async fn delete(&self, scope: &CredentialScope) -> Result<(), CredentialError> {
        let service = self.service.clone();
        let account = Self::account(scope);
        tokio::task::spawn_blocking(move || {
            match keyring::Entry::new(&service, &account).delete_password() {
                Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
                Err(_) => Err(CredentialError::Unavailable),
            }
        })
        .await
        .map_err(|_| CredentialError::Unavailable)?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn credential_native_payload_roundtrip_and_scope_digest_need_no_native_access() {
        let scope = CredentialScope::chat("actor", "profile");
        let mut value = CredentialSet::new([SecretString::new("synthetic-native-payload".into())]);
        value.bind_endpoint("https://example.invalid/v1");
        let payload = encode(&value).unwrap();
        let restored = decode(payload.to_string()).unwrap();
        assert!(restored.same_values(&value));
        assert!(restored.revision() == value.revision());
        assert!(restored.matches_endpoint("https://example.invalid/v1"));
        assert!(!restored.matches_endpoint("https://other.invalid/v1"));
        assert!(
            NativeCredentialStore::account(&scope)
                != NativeCredentialStore::account(&CredentialScope::chat("other", "profile"))
        );
        assert!(decode("invalid synthetic payload".into()).is_err());
    }
}
