use async_trait::async_trait;
use revue_office_lib::infrastructure::credentials::environment::EnvironmentCredentialStore;
use revue_office_lib::infrastructure::credentials::memory::{
    MemoryCredentialStore, MemoryFailures,
};
use revue_office_lib::infrastructure::credentials::migration::{
    PreferenceCredentialService, PreferencePayloadStore,
};
use revue_office_lib::infrastructure::credentials::redaction::{
    redacted_header, redacted_url, split_endpoint,
};
use revue_office_lib::infrastructure::persistence::sqlite::preferences::SqlitePreferencePayloadStore;
use revue_office_lib::providers::credentials::{
    CredentialError, CredentialPurpose, CredentialScope, CredentialSet, CredentialStore,
};
use secrecy::{ExposeSecret, SecretString};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

// Entirely synthetic, never sourced from environment or a native credential store.
const SENTINEL: &str = "synthetic-Q7rZ2mV9pK5xN8cT4wL6bH3jF0dS1aE";
const SEARCH: &str = "synthetic-search-C8vK4jX9sP2mZ5rF7dW1nQ6a";
const ENDPOINT: &str = "https://example.invalid/v1";
fn set(key: &str) -> CredentialSet {
    CredentialSet::new([SecretString::new(key.into())])
}
fn legacy() -> Value {
    json!({
        "llm_profiles":[{"id":"default", "name":"Synthetic", "base_url":ENDPOINT,
            "api_keys":[SENTINEL,SENTINEL,""], "api_key":SENTINEL, "models":["model"], "default_model":"model", "has_api_key":true, "api_key_count":0}],
        "active_profile_id":"default", "default_model":"model", "active_model":"model",
        "basic":{"app_name":"Synthetic","workspace_title":"Office","brand_tagline":"Synthetic","default_theme":"default"},
        "mcp_servers":[{"id":"builtin-baidu-ai-search","name":"Search","transport":"sse","endpoint":format!("https://search.invalid/sse?api_key={SEARCH}&region=cn"),"enabled":true}],
        "updated_at":"synthetic", "future_field":{"retain":"unknown non-secret data", (SENTINEL):SENTINEL}
    })
}
fn assert_clean(text: &str) {
    assert!(!text.contains(SENTINEL), "chat secret escaped");
    assert!(!text.contains(SEARCH), "search secret escaped");
}
struct Payload {
    raw: Mutex<Option<SecretString>>,
    fail_write: Mutex<bool>,
    fail_verify: Mutex<bool>,
    persisted: Mutex<bool>,
    normalize_json: Mutex<bool>,
    writes: std::sync::atomic::AtomicUsize,
}
impl Payload {
    fn new(value: Value) -> Self {
        Self {
            raw: Mutex::new(Some(SecretString::new(value.to_string()))),
            fail_write: Mutex::new(false),
            fail_verify: Mutex::new(false),
            persisted: Mutex::new(false),
            normalize_json: Mutex::new(false),
            writes: std::sync::atomic::AtomicUsize::new(0),
        }
    }
    fn text(&self) -> String {
        self.raw
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .expose_secret()
            .to_owned()
    }
}
#[async_trait]
impl PreferencePayloadStore for Payload {
    async fn read(&self, _: &str) -> Result<Option<SecretString>, CredentialError> {
        if *self.persisted.lock().unwrap() && *self.fail_verify.lock().unwrap() {
            return Err(CredentialError::Persistence);
        }
        Ok(self
            .raw
            .lock()
            .unwrap()
            .as_ref()
            .map(|s| SecretString::new(s.expose_secret().to_owned())))
    }
    async fn compare_and_swap(
        &self,
        _: &str,
        expected: Option<&SecretString>,
        replacement: &str,
    ) -> Result<(), CredentialError> {
        if *self.fail_write.lock().unwrap() {
            return Err(CredentialError::Persistence);
        }
        let mut raw = self.raw.lock().unwrap();
        if raw.as_ref().map(ExposeSecret::expose_secret)
            != expected.map(ExposeSecret::expose_secret)
        {
            return Err(CredentialError::Conflict);
        }
        let replacement = if *self.normalize_json.lock().unwrap() {
            serde_json::to_string_pretty(&serde_json::from_str::<Value>(replacement).unwrap())
                .unwrap()
        } else {
            replacement.into()
        };
        *raw = Some(SecretString::new(replacement));
        self.writes
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        *self.persisted.lock().unwrap() = true;
        Ok(())
    }
}
fn service(
    payload: Arc<dyn PreferencePayloadStore>,
    store: Arc<dyn CredentialStore>,
) -> PreferenceCredentialService {
    PreferenceCredentialService::new(
        payload,
        store,
        ENDPOINT.into(),
        "https://search.invalid/sse?region=cn".into(),
    )
}

#[tokio::test]
async fn credential_copy_verify_scrub_is_idempotent_and_preserves_unknown_preferences() {
    let payload = Arc::new(Payload::new(legacy()));
    let store = Arc::new(MemoryCredentialStore::default());
    let service = service(payload.clone(), store.clone());
    let value = service.read("actor").await.unwrap().unwrap();
    assert_clean(&payload.text());
    assert_clean(&value.to_string());
    assert!(value["future_field"]["retain"] == "unknown non-secret data");
    assert!(value["llm_profiles"][0]["api_keys"] == json!([]));
    assert!(value["llm_profiles"][0].get("api_key").is_none());
    assert!(value["llm_profiles"][0]["api_key_count"] == 1);
    assert!(value["llm_profiles"][0]["has_api_key"] == true);
    let keys = store
        .resolve(&CredentialScope::chat("actor", "default"))
        .await
        .unwrap()
        .unwrap();
    assert!(keys.same_values(&set(SENTINEL)));
    assert_clean(&format!("{keys:?}"));
    let revision = keys.revision().to_owned();
    service.read("actor").await.unwrap();
    assert!(
        store
            .resolve(&CredentialScope::chat("actor", "default"))
            .await
            .unwrap()
            .unwrap()
            .revision()
            == revision
    );
    assert!(
        store
            .resolve(&CredentialScope::web_search(
                "actor",
                "builtin-baidu-ai-search"
            ))
            .await
            .unwrap()
            .unwrap()
            .same_values(&set(SEARCH))
    );
    assert!(
        store
            .resolve(&CredentialScope::chat("other", "default"))
            .await
            .unwrap()
            .is_none()
    );
}
#[tokio::test]
async fn credential_write_read_and_verification_failures_never_clear_legacy_row() {
    for failures in [
        MemoryFailures {
            write: true,
            ..Default::default()
        },
        MemoryFailures {
            read: true,
            ..Default::default()
        },
        MemoryFailures {
            corrupt_read: true,
            ..Default::default()
        },
    ] {
        let payload = Arc::new(Payload::new(legacy()));
        let original = payload.text();
        let store = Arc::new(MemoryCredentialStore::default());
        store.set_failures(failures);
        let error = service(payload.clone(), store)
            .read("actor")
            .await
            .err()
            .unwrap();
        assert!(
            payload.text() == original,
            "unverified legacy row was changed"
        );
        assert_clean(&format!("{error:?} {error}"));
    }
}
#[tokio::test]
async fn credential_scrub_failure_retains_secure_copy_and_retry_succeeds() {
    let payload = Arc::new(Payload::new(legacy()));
    let original = payload.text();
    *payload.fail_write.lock().unwrap() = true;
    let store = Arc::new(MemoryCredentialStore::default());
    let service = service(payload.clone(), store.clone());
    assert!(service.read("actor").await.is_err());
    assert!(payload.text() == original);
    assert!(
        store
            .resolve(&CredentialScope::chat("actor", "default"))
            .await
            .unwrap()
            .unwrap()
            .same_values(&set(SENTINEL))
    );
    *payload.fail_write.lock().unwrap() = false;
    service.read("actor").await.unwrap();
    assert_clean(&payload.text());
}
#[tokio::test]
async fn credential_post_scrub_reread_failure_is_not_reported_as_success() {
    let payload = Arc::new(Payload::new(legacy()));
    *payload.fail_verify.lock().unwrap() = true;
    let store = Arc::new(MemoryCredentialStore::default());
    let service = service(payload.clone(), store.clone());
    assert!(service.read("actor").await.is_err());
    assert!(
        store
            .resolve(&CredentialScope::chat("actor", "default"))
            .await
            .unwrap()
            .is_some()
    );
    *payload.fail_verify.lock().unwrap() = false;
    service.read("actor").await.unwrap();
    assert_clean(&payload.text());
}
#[tokio::test]
async fn credential_settings_updates_preserve_rotate_and_clear_secure_keys_without_plaintext() {
    let payload = Arc::new(Payload::new(legacy()));
    let store = Arc::new(MemoryCredentialStore::default());
    let service = service(payload.clone(), store.clone());
    let mut value = service.read("actor").await.unwrap().unwrap();
    let previous = store
        .resolve(&CredentialScope::chat("actor", "default"))
        .await
        .unwrap()
        .unwrap()
        .revision()
        .to_owned();
    service.save("actor", value.clone()).await.unwrap();
    assert!(
        store
            .resolve(&CredentialScope::chat("actor", "default"))
            .await
            .unwrap()
            .unwrap()
            .revision()
            == previous
    );
    value["llm_profiles"][0]["api_keys"] = json!(["synthetic-new-replacement"]);
    service.save("actor", value).await.unwrap();
    assert_clean(&payload.text());
    assert!(!payload.text().contains("synthetic-new-replacement"));
    let replacement = store
        .resolve(&CredentialScope::chat("actor", "default"))
        .await
        .unwrap()
        .unwrap();
    assert!(replacement.revision() != previous);
    assert!(replacement.same_values(&set("synthetic-new-replacement")));
    let mut value = service.read("actor").await.unwrap().unwrap();
    value["llm_profiles"][0]["has_api_key"] = json!(false);
    service.save("actor", value).await.unwrap();
    assert!(
        store
            .resolve(&CredentialScope::chat("actor", "default"))
            .await
            .unwrap()
            .unwrap()
            .is_empty()
    );
    assert!(
        !service.read("actor").await.unwrap().unwrap()["llm_profiles"][0]["has_api_key"]
            .as_bool()
            .unwrap()
    );
}
#[tokio::test]
async fn credential_secure_values_cannot_reenter_preference_metadata_or_endpoints() {
    let payload = Arc::new(Payload::new(legacy()));
    let store = Arc::new(MemoryCredentialStore::default());
    let service = service(payload.clone(), store);
    let mut value = service.read("actor").await.unwrap().unwrap();
    value["basic"]["brand_tagline"] = json!(SENTINEL);
    let value = service.save("actor", value).await.unwrap();
    assert_clean(&payload.text());
    assert_clean(&value.to_string());
    let mut value = value;
    value["llm_profiles"][0]["base_url"] = json!(format!("https://example.invalid/{SENTINEL}"));
    assert!(matches!(
        service.save("actor", value).await,
        Err(CredentialError::InvalidInput)
    ));
    assert_clean(&payload.text());
}

#[tokio::test]
async fn credential_concurrent_actor_migrations_use_one_idempotent_path() {
    let payload = Arc::new(Payload::new(legacy()));
    let store = Arc::new(MemoryCredentialStore::default());
    let service = Arc::new(service(payload.clone(), store));
    let mut tasks = Vec::new();
    for _ in 0..8 {
        let service = service.clone();
        tasks.push(tokio::spawn(
            async move { service.read("actor").await.is_ok() },
        ));
    }
    for task in tasks {
        assert!(task.await.unwrap());
    }
    assert_clean(&payload.text());
}
fn environment() -> EnvironmentCredentialStore {
    EnvironmentCredentialStore::from_source(&|name: &'static str| match name {
        "LLM_TEXT_API_KEY" => Some(SecretString::new(SENTINEL.into())),
        "AIPPT_BAIDU_MCP_API_KEY" => Some(SecretString::new(SEARCH.into())),
        _ => None,
    })
}
#[tokio::test]
async fn credential_environment_is_replaceable_read_only_and_scope_mapped() {
    let env = environment();
    let memory = MemoryCredentialStore::default();
    let scope = CredentialScope::chat("actor", "default");
    memory.replace(&scope, set(SENTINEL)).await.unwrap();
    assert!(
        env.resolve(&scope)
            .await
            .unwrap()
            .unwrap()
            .same_values(&memory.resolve(&scope).await.unwrap().unwrap())
    );
    assert!(matches!(
        env.replace(&scope, set(SENTINEL)).await,
        Err(CredentialError::ReadOnly)
    ));
    assert!(matches!(
        env.delete(&scope).await,
        Err(CredentialError::ReadOnly)
    ));
    assert!(
        env.resolve(&CredentialScope::chat("actor", "custom-profile"))
            .await
            .unwrap()
            .is_none()
    );
    let mut other_purpose = scope.clone();
    other_purpose.purpose = CredentialPurpose::Image;
    assert!(env.resolve(&other_purpose).await.unwrap().is_none());
}
#[tokio::test]
async fn credential_server_scrubs_only_verified_deployment_copy_and_rejects_updates() {
    let payload = Arc::new(Payload::new(legacy()));
    let service = service(payload.clone(), Arc::new(environment()));
    let mut value = service.read("actor").await.unwrap().unwrap();
    assert_clean(&payload.text());
    assert!(
        service
            .chat_credentials("actor", "default", "https://untrusted.invalid/v1")
            .await
            .is_err()
    );
    value["llm_profiles"][0]["api_keys"] = json!(["synthetic-unauthorized-update"]);
    assert!(matches!(
        service.save("actor", value).await,
        Err(CredentialError::ReadOnly)
    ));
    assert!(!payload.text().contains("synthetic-unauthorized-update"));
    let mut mismatched = legacy();
    mismatched["llm_profiles"][0]["api_keys"] = json!(["synthetic-not-deployed"]);
    mismatched["llm_profiles"][0]
        .as_object_mut()
        .unwrap()
        .remove("api_key");
    let payload = Arc::new(Payload::new(mismatched));
    let original = payload.text();
    assert!(
        crate::service(payload.clone(), Arc::new(environment()))
            .read("actor")
            .await
            .is_err()
    );
    assert!(payload.text() == original);
}
#[tokio::test]
async fn credential_normalized_database_json_is_semantically_verified_and_not_rewritten() {
    let payload = Arc::new(Payload::new(legacy()));
    *payload.normalize_json.lock().unwrap() = true;
    let service = service(payload.clone(), Arc::new(MemoryCredentialStore::default()));
    service.read("actor").await.unwrap();
    service.read("actor").await.unwrap();
    assert!(payload.writes.load(std::sync::atomic::Ordering::Relaxed) == 1);
    assert_clean(&payload.text());
}
#[tokio::test]
async fn credential_failed_endpoint_change_cannot_send_new_key_to_old_endpoint_and_retry_recovers()
{
    let payload = Arc::new(Payload::new(legacy()));
    let store = Arc::new(MemoryCredentialStore::default());
    let service = service(payload.clone(), store);
    let mut replacement = service.read("actor").await.unwrap().unwrap();
    replacement["llm_profiles"][0]["base_url"] = json!("https://new.invalid/v1");
    replacement["llm_profiles"][0]["api_keys"] = json!(["synthetic-new-endpoint-key"]);
    *payload.fail_write.lock().unwrap() = true;
    assert!(service.save("actor", replacement.clone()).await.is_err());
    assert!(
        service
            .chat_credentials("actor", "default", ENDPOINT)
            .await
            .is_err()
    );
    assert!(service.read("actor").await.is_ok()); // Safe preferences remain repairable.
    *payload.fail_write.lock().unwrap() = false;
    service.save("actor", replacement).await.unwrap();
    assert!(
        service
            .chat_credentials("actor", "default", "https://new.invalid/v1")
            .await
            .unwrap()
            .is_some()
    );
    assert!(!payload.text().contains("synthetic-new-endpoint-key"));
}
#[test]
fn credential_header_url_and_legacy_dto_diagnostics_are_redacted() {
    for name in [
        "Authorization",
        "PROXY-AUTHORIZATION",
        "X-API-KEY",
        "api-key",
        "cookie",
        "set-cookie",
    ] {
        assert_clean(&redacted_header(name, SENTINEL));
    }
    let raw = format!(
        "https://user:{SENTINEL}@example.invalid/path?key={SENTINEL}&TOKEN={SEARCH}&region=cn#fragment"
    );
    let redacted = redacted_url(&raw);
    assert_clean(&redacted);
    assert!(!redacted.contains("user:"));
    assert!(redacted.contains("region=cn"));
    let (clean, values) = split_endpoint(&format!(
        "https://example.invalid/sse?api_key={SEARCH}&region=cn"
    ))
    .unwrap();
    assert_clean(&clean);
    assert!(values.len() == 1);
    assert!(split_endpoint(&raw).is_err());
}
#[tokio::test]
async fn credential_sqlite_cas_scrubs_actual_persisted_json_and_preserves_other_actor() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::query("CREATE TABLE user_settings (id TEXT PRIMARY KEY, user_id TEXT UNIQUE, payload TEXT, created_at TEXT, updated_at TEXT)").execute(&pool).await.unwrap();
    for actor in ["actor", "other"] {
        sqlx::query("INSERT INTO user_settings VALUES (?, ?, ?, '', '')")
            .bind(actor)
            .bind(actor)
            .bind(legacy().to_string())
            .execute(&pool)
            .await
            .unwrap();
    }
    let adapter = Arc::new(SqlitePreferencePayloadStore::new(pool.clone()));
    let service = service(adapter.clone(), Arc::new(MemoryCredentialStore::default()));
    service.read("actor").await.unwrap();
    assert_clean(
        adapter
            .read("actor")
            .await
            .unwrap()
            .unwrap()
            .expose_secret(),
    );
    assert!(
        adapter
            .read("other")
            .await
            .unwrap()
            .unwrap()
            .expose_secret()
            .contains(SENTINEL)
    );
    let stale = SecretString::new("synthetic-stale-payload".into());
    assert!(matches!(
        adapter.compare_and_swap("actor", Some(&stale), "{}").await,
        Err(CredentialError::Conflict)
    ));
    assert_clean(
        adapter
            .read("actor")
            .await
            .unwrap()
            .unwrap()
            .expose_secret(),
    );
    pool.close().await;
}
