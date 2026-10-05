use async_trait::async_trait;
use secrecy::{ExposeSecret, SecretString};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Arc, Weak};
use tokio::sync::Mutex;
use zeroize::Zeroize;

/// Wipe transient legacy strings on every failure path, including partial
/// migrations. Only successfully scrubbed JSON is moved out of this guard.
pub struct SecretDocument(pub Value);
impl Drop for SecretDocument {
    fn drop(&mut self) {
        fn wipe(value: &mut Value) {
            match value {
                Value::String(text) => text.zeroize(),
                Value::Array(values) => {
                    for value in values {
                        wipe(value);
                    }
                }
                Value::Object(values) => {
                    for (mut key, mut value) in std::mem::take(values) {
                        key.zeroize();
                        wipe(&mut value);
                    }
                }
                _ => {}
            }
        }
        wipe(&mut self.0);
    }
}
use super::{
    redaction::{redact_json, split_endpoint},
    startup_scope,
};
use crate::providers::credentials::{
    CredentialError, CredentialPurpose, CredentialScope, CredentialSet, CredentialStore,
};

#[async_trait]
pub trait PreferencePayloadStore: Send + Sync {
    async fn read(&self, actor: &str) -> Result<Option<SecretString>, CredentialError>;
    /// Compare the exact original payload to avoid clearing a concurrent writer.
    /// Only the scrubbed payload may be supplied as `replacement`.
    async fn compare_and_swap(
        &self,
        actor: &str,
        expected: Option<&SecretString>,
        replacement: &str,
    ) -> Result<(), CredentialError>;
    async fn actor_ids(&self) -> Result<Vec<String>, CredentialError> {
        Ok(Vec::new())
    }
}

/// One Bootstrap-owned instance serializes reads, migration and settings updates
/// per Actor. No SQL transaction is held while interacting with a secure store.
pub struct PreferenceCredentialService {
    preferences: Arc<dyn PreferencePayloadStore>,
    store: Arc<dyn CredentialStore>,
    locks: Mutex<HashMap<String, Weak<Mutex<()>>>>,
    startup_endpoint: String,
    startup_search_endpoint: String,
}
impl PreferenceCredentialService {
    pub fn new(
        preferences: Arc<dyn PreferencePayloadStore>,
        store: Arc<dyn CredentialStore>,
        startup_endpoint: String,
        startup_search_endpoint: String,
    ) -> Self {
        Self {
            preferences,
            store,
            locks: Mutex::new(HashMap::new()),
            startup_endpoint,
            startup_search_endpoint,
        }
    }
    async fn actor_lock(&self, actor: &str) -> Arc<Mutex<()>> {
        let mut locks = self.locks.lock().await;
        locks.retain(|_, v| v.strong_count() > 0);
        if let Some(lock) = locks.get(actor).and_then(Weak::upgrade) {
            return lock;
        }
        let lock = Arc::new(Mutex::new(()));
        locks.insert(actor.into(), Arc::downgrade(&lock));
        lock
    }
    pub async fn read(&self, actor: &str) -> Result<Option<Value>, CredentialError> {
        let lock = self.actor_lock(actor).await;
        let _guard = lock.lock().await;
        self.read_locked(actor).await
    }
    pub async fn migrate_all(&self) -> Result<(), CredentialError> {
        for actor in self.preferences.actor_ids().await? {
            self.read(&actor).await?;
        }
        Ok(())
    }
    async fn read_locked(&self, actor: &str) -> Result<Option<Value>, CredentialError> {
        let Some(original) = self.preferences.read(actor).await? else {
            return Ok(None);
        };
        let mut document = SecretDocument(
            serde_json::from_str(original.expose_secret())
                .map_err(|_| CredentialError::InvalidInput)?,
        );
        self.secure_document(actor, &mut document.0, false).await?;
        let scrubbed =
            serde_json::to_string(&document.0).map_err(|_| CredentialError::InvalidInput)?;
        let original_document = SecretDocument(
            serde_json::from_str(original.expose_secret())
                .map_err(|_| CredentialError::InvalidInput)?,
        );
        if document.0 != original_document.0 {
            self.preferences
                .compare_and_swap(actor, Some(&original), &scrubbed)
                .await?;
            self.verify_scrub(actor, &scrubbed).await?;
        }
        Ok(Some(std::mem::take(&mut document.0)))
    }
    pub async fn save(&self, actor: &str, value: Value) -> Result<Value, CredentialError> {
        let mut document = SecretDocument(value);
        let lock = self.actor_lock(actor).await;
        let _guard = lock.lock().await;
        // Migrate old secrets before accepting any new settings. Failure never
        // overwrites an original row with unverified/empty credential metadata.
        self.read_locked(actor).await?;
        let original = self.preferences.read(actor).await?;
        self.secure_document(actor, &mut document.0, true).await?;
        // Validate the full public DTO only after all legacy secret fields have
        // been removed; ordinary serialization/deserialization never sees keys.
        serde_json::from_value::<crate::application::preferences::AppSettings>(document.0.clone())
            .map_err(|_| CredentialError::InvalidInput)?;
        let scrubbed =
            serde_json::to_string(&document.0).map_err(|_| CredentialError::InvalidInput)?;
        self.preferences
            .compare_and_swap(actor, original.as_ref(), &scrubbed)
            .await?;
        self.verify_scrub(actor, &scrubbed).await?;
        Ok(std::mem::take(&mut document.0))
    }
    async fn verify_scrub(&self, actor: &str, expected: &str) -> Result<(), CredentialError> {
        let actual = self
            .preferences
            .read(actor)
            .await?
            .ok_or(CredentialError::Persistence)?;
        // MySQL JSON normalizes whitespace/key order; compare semantic values,
        // not serialized bytes. CAS still compares the exact original row.
        let actual = SecretDocument(
            serde_json::from_str(actual.expose_secret())
                .map_err(|_| CredentialError::Persistence)?,
        );
        let expected: Value =
            serde_json::from_str(expected).map_err(|_| CredentialError::Persistence)?;
        if actual.0 != expected {
            return Err(CredentialError::Conflict);
        }
        Ok(())
    }
    async fn copy_verify(
        &self,
        scope: &CredentialScope,
        values: CredentialSet,
        updating: bool,
    ) -> Result<usize, CredentialError> {
        let expected = values.copy_for_store();
        // Idempotent migration: a matching existing secure copy need not be
        // rewritten (also enables safe Server migration to a read-only mapping).
        if !updating
            && let Some(existing) = self.store.resolve(scope).await?
            && existing.same_values(&expected)
            && ((self.store.read_only() && existing.endpoint_binding().is_none())
                || existing.endpoint_binding() == expected.endpoint_binding())
        {
            return Ok(existing.len());
        }
        self.store.replace(scope, values).await?;
        let actual = self
            .store
            .resolve(scope)
            .await?
            .ok_or(CredentialError::Verification)?;
        if !actual.same_values(&expected)
            || actual.endpoint_binding() != expected.endpoint_binding()
        {
            return Err(CredentialError::Verification);
        }
        Ok(actual.len())
    }
    async fn credential_count(
        &self,
        actor: &str,
        profile: &str,
        endpoint: &str,
    ) -> Result<usize, CredentialError> {
        if let Some(value) = self
            .store
            .resolve(&CredentialScope::chat(actor, profile))
            .await?
        {
            return Ok(value.len());
        }
        if matches!(profile, "default" | "startup-env")
            && endpoint.trim_end_matches('/') == self.startup_endpoint.trim_end_matches('/')
        {
            return Ok(self
                .store
                .resolve(&startup_scope(CredentialPurpose::Chat))
                .await?
                .map(|v| v.len())
                .unwrap_or(0));
        }
        Ok(0)
    }
    async fn collect_known(
        &self,
        scope: &CredentialScope,
        startup: Option<CredentialPurpose>,
        known: &mut Vec<SecretString>,
    ) -> Result<(), CredentialError> {
        if let Some(value) = self.store.resolve(scope).await? {
            known.extend(
                value
                    .values()
                    .iter()
                    .map(|s| SecretString::new(s.expose_secret().to_owned())),
            );
        }
        if let Some(purpose) = startup
            && let Some(value) = self.store.resolve(&startup_scope(purpose)).await?
        {
            known.extend(
                value
                    .values()
                    .iter()
                    .map(|s| SecretString::new(s.expose_secret().to_owned())),
            );
        }
        Ok(())
    }
    fn checked_credentials(
        value: Option<CredentialSet>,
        endpoint: &str,
    ) -> Result<Option<CredentialSet>, CredentialError> {
        if value
            .as_ref()
            .is_some_and(|v| !v.matches_endpoint(endpoint))
        {
            return Err(CredentialError::InvalidInput);
        }
        Ok(value)
    }
    pub async fn chat_credentials(
        &self,
        actor: &str,
        profile: &str,
        endpoint: &str,
    ) -> Result<Option<CredentialSet>, CredentialError> {
        if self.store.read_only()
            && endpoint.trim_end_matches('/') != self.startup_endpoint.trim_end_matches('/')
        {
            return Err(CredentialError::InvalidInput);
        }
        let value = self
            .store
            .resolve(&CredentialScope::chat(actor, profile))
            .await?;
        if let Some(value) = value {
            if !value.matches_endpoint(endpoint) {
                return Err(CredentialError::InvalidInput);
            }
            return Ok(Some(value));
        }
        if matches!(profile, "default" | "startup-env")
            && endpoint.trim_end_matches('/') == self.startup_endpoint.trim_end_matches('/')
        {
            return Self::checked_credentials(
                self.store
                    .resolve(&startup_scope(CredentialPurpose::Chat))
                    .await?,
                endpoint,
            );
        }
        Ok(None)
    }
    pub async fn search_credentials(
        &self,
        actor: &str,
        profile: &str,
        endpoint: &str,
    ) -> Result<Option<CredentialSet>, CredentialError> {
        if self.store.read_only()
            && endpoint.trim_end_matches('/') != self.startup_search_endpoint.trim_end_matches('/')
        {
            return Err(CredentialError::InvalidInput);
        }
        let value = self
            .store
            .resolve(&CredentialScope::web_search(actor, profile))
            .await?;
        if let Some(value) = value {
            if !value.matches_endpoint(endpoint) {
                return Err(CredentialError::InvalidInput);
            }
            return Ok(Some(value));
        }
        if profile == "builtin-baidu-ai-search"
            && endpoint.trim_end_matches('/') == self.startup_search_endpoint.trim_end_matches('/')
        {
            return Self::checked_credentials(
                self.store
                    .resolve(&startup_scope(CredentialPurpose::WebSearch))
                    .await?,
                endpoint,
            );
        }
        Ok(None)
    }
    async fn secure_document(
        &self,
        actor: &str,
        value: &mut Value,
        updating: bool,
    ) -> Result<(), CredentialError> {
        let profiles = value
            .get_mut("llm_profiles")
            .and_then(Value::as_array_mut)
            .ok_or(CredentialError::InvalidInput)?;
        let mut known = Vec::new();
        let mut profile_ids = std::collections::HashSet::new();
        for profile in profiles {
            let id = profile
                .get("id")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .ok_or(CredentialError::InvalidInput)?
                .to_owned();
            if !profile_ids.insert(id.clone()) {
                return Err(CredentialError::InvalidInput);
            }
            let endpoint = zeroize::Zeroizing::new(
                profile
                    .get("base_url")
                    .and_then(Value::as_str)
                    .ok_or(CredentialError::InvalidInput)?
                    .to_owned(),
            );
            let (endpoint, mut secrets) = split_endpoint(&endpoint)?;
            let object = profile
                .as_object_mut()
                .ok_or(CredentialError::InvalidInput)?;
            if let Some(raw) = object.remove("api_keys") {
                let Value::Array(values) = raw else {
                    return Err(CredentialError::InvalidInput);
                };
                for raw in values {
                    let Value::String(raw) = raw else {
                        return Err(CredentialError::InvalidInput);
                    };
                    secrets.push(SecretString::new(raw));
                }
            }
            if let Some(raw) = object.remove("api_key") {
                match raw {
                    Value::String(raw) => secrets.push(SecretString::new(raw)),
                    Value::Null => {}
                    _ => return Err(CredentialError::InvalidInput),
                }
            }
            let mut secrets = CredentialSet::new(secrets);
            if secrets.values().iter().any(|s| {
                endpoint.contains(s.expose_secret().as_str())
                    || id.contains(s.expose_secret().as_str())
            }) {
                return Err(CredentialError::InvalidInput);
            }
            let scope = CredentialScope::chat(actor, &id);
            self.collect_known(
                &scope,
                matches!(id.as_str(), "default" | "startup-env").then_some(CredentialPurpose::Chat),
                &mut known,
            )
            .await?;
            if known.iter().any(|s| {
                endpoint.contains(s.expose_secret().as_str())
                    || id.contains(s.expose_secret().as_str())
            }) {
                return Err(CredentialError::InvalidInput);
            }
            secrets.bind_endpoint(&endpoint);
            known.extend(
                secrets
                    .values()
                    .iter()
                    .map(|s| SecretString::new(s.expose_secret().to_owned())),
            );
            let count = if !secrets.is_empty() {
                self.copy_verify(&scope, secrets, updating).await?
            } else if updating && object.get("has_api_key").and_then(Value::as_bool) == Some(false)
            {
                if self.credential_count(actor, &id, &endpoint).await? > 0 {
                    self.copy_verify(&scope, secrets, true).await?
                } else {
                    0
                }
            } else {
                self.credential_count(actor, &id, &endpoint).await?
            };
            if let Some(Value::String(mut raw)) = object.remove("base_url") {
                raw.zeroize();
            }
            object.insert("base_url".into(), Value::String(endpoint));
            object.insert("api_keys".into(), json!([]));
            object.insert("has_api_key".into(), json!(count > 0));
            object.insert("api_key_count".into(), json!(count));
        }
        if let Some(servers) = value.get_mut("mcp_servers").and_then(Value::as_array_mut) {
            let mut server_ids = std::collections::HashSet::new();
            for server in servers {
                let id = server
                    .get("id")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .ok_or(CredentialError::InvalidInput)?
                    .to_owned();
                if !server_ids.insert(id.clone()) {
                    return Err(CredentialError::InvalidInput);
                }
                let endpoint = zeroize::Zeroizing::new(
                    server
                        .get("endpoint")
                        .and_then(Value::as_str)
                        .ok_or(CredentialError::InvalidInput)?
                        .to_owned(),
                );
                let (endpoint, secrets) = split_endpoint(&endpoint)?;
                let mut secrets = CredentialSet::new(secrets);
                if secrets.values().iter().any(|s| {
                    endpoint.contains(s.expose_secret().as_str())
                        || id.contains(s.expose_secret().as_str())
                }) {
                    return Err(CredentialError::InvalidInput);
                }
                self.collect_known(
                    &CredentialScope::web_search(actor, &id),
                    (id == "builtin-baidu-ai-search").then_some(CredentialPurpose::WebSearch),
                    &mut known,
                )
                .await?;
                if known.iter().any(|s| {
                    endpoint.contains(s.expose_secret().as_str())
                        || id.contains(s.expose_secret().as_str())
                }) {
                    return Err(CredentialError::InvalidInput);
                }
                secrets.bind_endpoint(&endpoint);
                known.extend(
                    secrets
                        .values()
                        .iter()
                        .map(|s| SecretString::new(s.expose_secret().to_owned())),
                );
                if !secrets.is_empty() {
                    self.copy_verify(&CredentialScope::web_search(actor, &id), secrets, updating)
                        .await?;
                }
                if let Some(Value::String(mut raw)) = server
                    .as_object_mut()
                    .ok_or(CredentialError::InvalidInput)?
                    .remove("endpoint")
                {
                    raw.zeroize();
                }
                server["endpoint"] = Value::String(endpoint);
            }
        }
        redact_json(value, &known);
        Ok(())
    }
}
