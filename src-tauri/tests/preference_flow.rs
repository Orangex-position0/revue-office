use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use revue_office_lib::application::identity::{Actor, ActorId};
use revue_office_lib::application::preferences::{
    McpConnectionRequest, McpConnectionTester, PreferenceApplicationService, PreferenceDefaults,
    PreferenceError, PreferenceUpdate, SecurePreferencePort,
};
use revue_office_lib::infrastructure::credentials::memory::MemoryCredentialStore;
use revue_office_lib::infrastructure::credentials::migration::{
    PreferenceCredentialService, PreferencePayloadStore,
};
use revue_office_lib::infrastructure::persistence::mysql::preferences::MySqlPreferencePayloadStore;
use revue_office_lib::infrastructure::persistence::sqlite::preferences::SqlitePreferencePayloadStore;
use secrecy::ExposeSecret;
use serde_json::{Value, json};

const ENDPOINT: &str = "https://example.invalid/v1";
const SEARCH_ENDPOINT: &str = "https://search.invalid/sse";

fn defaults() -> PreferenceDefaults {
    PreferenceDefaults::new(
        "Revue Office".into(),
        ENDPOINT.into(),
        "model-a".into(),
        ["model-a".into(), "model-b".into(), "model-a".into()],
        SEARCH_ENDPOINT.into(),
    )
}

fn update(secret: &str) -> Value {
    json!({
        "llm_profiles": [{
            "id": "profile-a", "name": "Primary", "base_url": ENDPOINT,
            "api_keys": [secret], "models": ["model-a", "model-b"],
            "default_model": "model-a", "has_api_key": false, "api_key_count": 0
        }],
        "active_profile_id": "missing-profile",
        "default_model": "missing-model",
        "active_model": "model-b",
        "basic": {"app_name":"", "workspace_title":"Office", "brand_tagline":"Work", "default_theme":"default"},
        "mcp_servers": [],
        "updated_at": "client-value",
        "future_field": {"retained": true}
    })
}

struct FakeSecureStore {
    value: Mutex<Option<Value>>,
    saw_secret: Mutex<bool>,
}

impl FakeSecureStore {
    fn empty() -> Self {
        Self {
            value: Mutex::new(None),
            saw_secret: Mutex::new(false),
        }
    }
}

#[async_trait]
impl SecurePreferencePort for FakeSecureStore {
    async fn load(&self, _: &ActorId) -> Result<Option<Value>, PreferenceError> {
        Ok(self.value.lock().unwrap().clone())
    }

    async fn save(
        &self,
        _: &ActorId,
        mut update: PreferenceUpdate,
    ) -> Result<Value, PreferenceError> {
        let mut value = update.take();
        let profile = value["llm_profiles"][0].as_object_mut().unwrap();
        let secrets = profile.remove("api_keys").unwrap();
        *self.saw_secret.lock().unwrap() =
            secrets.as_array().is_some_and(|values| !values.is_empty());
        profile.remove("api_key");
        profile.insert("api_keys".into(), json!([]));
        profile.insert("has_api_key".into(), json!(true));
        profile.insert("api_key_count".into(), json!(1));
        *self.value.lock().unwrap() = Some(value.clone());
        Ok(value)
    }

    async fn chat_credential_count(
        &self,
        _: &ActorId,
        _: &str,
        _: &str,
    ) -> Result<usize, PreferenceError> {
        Ok(1)
    }

    async fn has_search_credentials(
        &self,
        _: &ActorId,
        _: &str,
        _: &str,
    ) -> Result<bool, PreferenceError> {
        Ok(true)
    }
}

struct FakeMcp;

#[async_trait]
impl McpConnectionTester for FakeMcp {
    async fn test(
        &self,
        _: &ActorId,
        _: McpConnectionRequest,
    ) -> Result<Option<Vec<Value>>, PreferenceError> {
        Ok(Some(vec![json!({"name":"search"})]))
    }
}

#[tokio::test]
async fn application_defaults_normalizes_selection_and_delegates_secrets() {
    let store = Arc::new(FakeSecureStore::empty());
    let service = PreferenceApplicationService::new(store.clone(), Arc::new(FakeMcp), defaults());
    let actor = Actor::user("actor-a");

    let initial = service.get(&actor).await.unwrap();
    assert_eq!(initial.llm_profiles[0].api_key_count, 1);
    assert!(initial.llm_profiles[0].has_api_key);
    assert_eq!(initial.mcp_servers.len(), 1);
    assert_eq!(initial.llm_profiles[0].models, ["model-a", "model-b"]);

    let saved = service
        .save(
            &actor,
            PreferenceUpdate::new(update("synthetic-delegated-key")),
        )
        .await
        .unwrap();
    assert!(*store.saw_secret.lock().unwrap());
    assert_eq!(saved.active_profile_id, "profile-a");
    assert_eq!(saved.default_model, "model-a");
    assert_eq!(saved.active_model, "model-b");
    assert_eq!(saved.basic.app_name, "Revue Office");
    let persisted = store.value.lock().unwrap().clone().unwrap();
    assert_eq!(persisted["future_field"]["retained"], true);
    assert!(!persisted.to_string().contains("synthetic-delegated-key"));

    let probe = service
        .test_mcp(
            &actor,
            McpConnectionRequest {
                id: "search".into(),
                transport: "sse".into(),
                endpoint: SEARCH_ENDPOINT.into(),
            },
        )
        .await
        .unwrap();
    assert!(probe.ok);
    assert_eq!(probe.tools.len(), 1);
}

#[test]
fn secret_bearing_models_never_serialize_or_debug_plaintext() {
    let secret = "synthetic-model-secret";
    let profile: revue_office_lib::application::preferences::LlmProfileConfig =
        serde_json::from_value(json!({
            "id":"default", "name":"Synthetic", "base_url":ENDPOINT,
            "api_keys":[secret], "api_key":secret, "models":["model-a"],
            "default_model":"model-a"
        }))
        .unwrap();
    assert!(!serde_json::to_string(&profile).unwrap().contains(secret));
    assert!(!format!("{profile:?}").contains(secret));
    let server: revue_office_lib::application::preferences::McpServerConfig =
        serde_json::from_value(json!({
            "id":"search", "name":"Search", "transport":"sse",
            "endpoint":format!("https://user:{secret}@example.invalid/sse?api_key={secret}"),
            "enabled":true
        }))
        .unwrap();
    assert!(!serde_json::to_string(&server).unwrap().contains(secret));
    assert!(!format!("{server:?}").contains(secret));
}

fn sqlite_url() -> (std::path::PathBuf, String) {
    let path = std::env::temp_dir().join(format!("revue-preferences-{}.db", uuid::Uuid::new_v4()));
    (
        path.clone(),
        format!("sqlite://{}?mode=rwc", path.display()),
    )
}

#[tokio::test]
async fn sqlite_preferences_survive_restart_without_persisting_credentials() {
    let (path, url) = sqlite_url();
    let payloads = Arc::new(
        SqlitePreferencePayloadStore::connect(&url, 1)
            .await
            .unwrap(),
    );
    let pool = sqlx::SqlitePool::connect(&url).await.unwrap();
    let actor_id = "preference-owner";
    let now = "2026-10-03T00:00:00Z";
    sqlx::query("INSERT INTO users (id, username, password_hash, created_at, updated_at) VALUES (?, ?, ?, ?, ?)")
        .bind(actor_id).bind("preference-owner").bind("test-only").bind(now).bind(now)
        .execute(&pool).await.unwrap();
    pool.close().await;

    let credentials = Arc::new(MemoryCredentialStore::default());
    let secure = Arc::new(PreferenceCredentialService::new(
        payloads.clone(),
        credentials.clone(),
        ENDPOINT.into(),
        SEARCH_ENDPOINT.into(),
    ));
    let service = PreferenceApplicationService::new(secure, Arc::new(FakeMcp), defaults());
    let actor = Actor::user(actor_id);
    service
        .save(
            &actor,
            PreferenceUpdate::new(update("synthetic-sqlite-secret")),
        )
        .await
        .unwrap();
    let raw = payloads.read(actor_id).await.unwrap().unwrap();
    assert!(!raw.expose_secret().contains("synthetic-sqlite-secret"));
    assert!(raw.expose_secret().contains("future_field"));

    drop(service);
    let restarted_payloads = Arc::new(
        SqlitePreferencePayloadStore::connect(&url, 1)
            .await
            .unwrap(),
    );
    let restarted_secure = Arc::new(PreferenceCredentialService::new(
        restarted_payloads,
        credentials,
        ENDPOINT.into(),
        SEARCH_ENDPOINT.into(),
    ));
    let restarted =
        PreferenceApplicationService::new(restarted_secure, Arc::new(FakeMcp), defaults());
    let settings = restarted.get(&actor).await.unwrap();
    assert_eq!(settings.active_profile_id, "profile-a");
    assert_eq!(settings.active_model, "model-b");
    let _ = tokio::fs::remove_file(path).await;
}

#[tokio::test]
#[ignore = "requires REVUE_ALLOW_MYSQL_TEST=1 and isolated MYSQL_TEST_DATABASE_URL"]
async fn mysql_preferences_use_the_same_secure_application_contract() {
    assert_eq!(std::env::var("REVUE_ALLOW_MYSQL_TEST").as_deref(), Ok("1"));
    let url = std::env::var("MYSQL_TEST_DATABASE_URL").unwrap();
    let database = url
        .split('?')
        .next()
        .unwrap_or(&url)
        .rsplit('/')
        .next()
        .unwrap_or("");
    assert!(
        url.starts_with("mysql://")
            && (database.starts_with("test_") || database.ends_with("_test"))
    );
    let actor_id = uuid::Uuid::new_v4().to_string();
    let payloads = Arc::new(MySqlPreferencePayloadStore::connect(&url, 1).await.unwrap());
    let secure = Arc::new(PreferenceCredentialService::new(
        payloads.clone(),
        Arc::new(MemoryCredentialStore::default()),
        ENDPOINT.into(),
        SEARCH_ENDPOINT.into(),
    ));
    let service = PreferenceApplicationService::new(secure, Arc::new(FakeMcp), defaults());
    let actor = Actor::user(&actor_id);
    service
        .save(
            &actor,
            PreferenceUpdate::new(update("synthetic-mysql-secret")),
        )
        .await
        .unwrap();
    let raw = payloads.read(&actor_id).await.unwrap().unwrap();
    assert!(!raw.expose_secret().contains("synthetic-mysql-secret"));
    assert_eq!(service.get(&actor).await.unwrap().active_model, "model-b");
}
