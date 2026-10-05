pub mod environment;
pub mod memory;
pub mod migration;
pub mod native;
pub mod redaction;

use crate::providers::credentials::{
    CredentialBackend, CredentialError, CredentialPurpose, CredentialScope, CredentialStore,
};
use environment::{EnvironmentCredentialStore, ProcessCredentialEnvironment};
use native::NativeCredentialStore;
use std::sync::Arc;

pub const STARTUP_ACTOR: &str = "__runtime_startup__";

/// Called only by Bootstrap, never by business code. No plaintext fallback.
pub async fn build_store(
    backend: CredentialBackend,
    endpoints: &[(CredentialPurpose, &str)],
) -> Result<Arc<dyn CredentialStore>, CredentialError> {
    let mut environment = EnvironmentCredentialStore::from_source(&ProcessCredentialEnvironment);
    environment.bind_endpoints(endpoints)?;
    if backend == CredentialBackend::Environment {
        return Ok(Arc::new(environment));
    }
    let native: Arc<dyn CredentialStore> =
        Arc::new(NativeCredentialStore::new("revue-office.credentials.v1"));
    verify_backend(native.as_ref()).await?;
    for purpose in [
        CredentialPurpose::Chat,
        CredentialPurpose::Image,
        CredentialPurpose::Video,
        CredentialPurpose::WebSearch,
    ] {
        let scope = startup_scope(purpose);
        if let Some(value) = environment.resolve(&scope).await? {
            let expected = value.copy_for_store();
            native.replace(&scope, value).await?;
            let actual = native
                .resolve(&scope)
                .await?
                .ok_or(CredentialError::Verification)?;
            if !actual.same_values(&expected)
                || actual.endpoint_binding() != expected.endpoint_binding()
            {
                return Err(CredentialError::Verification);
            }
        }
    }
    Ok(native)
}

/// A random, synthetic readiness record distinguishes "no stored key" from a
/// locked/unwritable backend. No user credential is used for this probe.
async fn verify_backend(store: &dyn CredentialStore) -> Result<(), CredentialError> {
    use crate::providers::credentials::CredentialSet;
    let scope = CredentialScope {
        actor_id: "__startup_readiness__".into(),
        profile_id: uuid::Uuid::new_v4().to_string(),
        provider_id: "readiness-probe".into(),
        purpose: CredentialPurpose::Chat,
    };
    let expected =
        CredentialSet::new([secrecy::SecretString::new(uuid::Uuid::new_v4().to_string())]);
    store.replace(&scope, expected.copy_for_store()).await?;
    let actual = store
        .resolve(&scope)
        .await?
        .ok_or(CredentialError::Verification)?;
    if !actual.same_values(&expected) {
        return Err(CredentialError::Verification);
    }
    store.delete(&scope).await?;
    if store.resolve(&scope).await?.is_some() {
        return Err(CredentialError::Verification);
    }
    Ok(())
}

pub fn startup_scope(purpose: CredentialPurpose) -> CredentialScope {
    CredentialScope {
        actor_id: STARTUP_ACTOR.into(),
        profile_id: "default".into(),
        provider_id: if purpose == CredentialPurpose::WebSearch {
            "baidu-mcp"
        } else {
            "openai-compatible"
        }
        .into(),
        purpose,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use memory::{MemoryCredentialStore, MemoryFailures};
    #[tokio::test]
    async fn credential_startup_import_binding_cannot_cross_configured_endpoints() {
        let source = |name: &'static str| {
            (name == "LLM_TEXT_API_KEY")
                .then(|| secrecy::SecretString::new("synthetic-import-binding-key".into()))
        };
        let mut environment = EnvironmentCredentialStore::from_source(&source);
        let endpoint = "https://configured.invalid/v1";
        environment
            .bind_endpoints(&[(CredentialPurpose::Chat, endpoint)])
            .unwrap();
        let value = environment
            .resolve(&startup_scope(CredentialPurpose::Chat))
            .await
            .unwrap()
            .unwrap();
        assert!(value.matches_endpoint(endpoint));
        assert!(!value.matches_endpoint("https://other.invalid/v1"));
        let memory = MemoryCredentialStore::default();
        memory
            .replace(&startup_scope(CredentialPurpose::Chat), value)
            .await
            .unwrap();
        let value = memory
            .resolve(&startup_scope(CredentialPurpose::Chat))
            .await
            .unwrap()
            .unwrap();
        assert!(value.matches_endpoint(endpoint));
        assert!(!value.matches_endpoint("https://other.invalid/v1"));
        let mut invalid = EnvironmentCredentialStore::from_source(&source);
        assert!(
            invalid
                .bind_endpoints(&[(
                    CredentialPurpose::Chat,
                    "https://configured.invalid/synthetic-import-binding-key"
                )])
                .is_err()
        );
    }
    #[tokio::test]
    async fn credential_startup_probe_fails_closed_without_native_or_environment_access() {
        let store = MemoryCredentialStore::default();
        verify_backend(&store).await.unwrap();
        for failure in [
            MemoryFailures {
                read: true,
                ..Default::default()
            },
            MemoryFailures {
                write: true,
                ..Default::default()
            },
            MemoryFailures {
                delete: true,
                ..Default::default()
            },
        ] {
            let store = MemoryCredentialStore::default();
            store.set_failures(failure);
            assert!(verify_backend(&store).await.is_err());
        }
    }
}
