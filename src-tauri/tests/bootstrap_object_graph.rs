use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use revue_office_lib::bootstrap::config::RuntimeProfile;

fn source(path: &str) -> String {
    fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join(path),
    )
    .unwrap()
}

#[test]
fn bootstrap_is_the_only_runtime_composition_root() {
    let lib = source("lib.rs");
    assert!(lib.contains("bootstrap::run().await"));
    for legacy in [
        "state::init_db_pool",
        "state::set_db_pool",
        "app::state",
        "routes::build_router",
        "config::initialize",
    ] {
        assert!(!lib.contains(legacy), "lib still uses {legacy}");
    }

    let bootstrap = source("bootstrap.rs");
    assert!(bootstrap.contains("infrastructure::build_credentials"));
    assert!(bootstrap.contains("infrastructure::build_identity"));
    assert!(bootstrap.contains("application::build"));
    assert!(bootstrap.contains("http::build"));
}

#[test]
fn http_state_contains_facades_but_no_persistence_or_registry() {
    let state = source("transport/http/state.rs");
    for facade in [
        "IdentityApplicationService",
        "ChatApplicationService",
        "SessionApplicationService",
        "AssetApplicationService",
        "ProjectApplicationService",
        "PreferenceApplicationService",
        "NotificationApplicationService",
        "DashboardApplicationService",
    ] {
        assert!(state.contains(facade), "missing facade {facade}");
    }
    for forbidden in [
        "sqlx::",
        "DbPool",
        "Repository>",
        "CredentialStore",
        "ToolRegistry",
        "Mutex<",
        "OnceLock",
    ] {
        assert!(!state.contains(forbidden), "HttpState exposes {forbidden}");
    }
}

#[test]
fn router_and_extractors_use_explicit_state_without_extensions_or_globals() {
    let router = source("transport/http/router.rs");
    assert!(router.contains(".with_state(state)"));
    assert!(!router.contains("Extension"));

    let auth = source("transport/http/auth.rs");
    assert!(auth.contains("State::<IdentityState>::from_request_parts"));
    assert!(!auth.contains("Extension"));

    let session = source("transport/http/handlers/session.rs");
    assert!(session.contains("State(service): State<Arc<SessionApplicationService>>"));
    assert!(!session.contains("crate::app::"));
    assert!(!session.contains("crate::state::"));
}

#[test]
fn runtime_profile_inputs_are_independent_and_no_config_singleton_remains() {
    let local = RuntimeProfile::load_and_validate(&|_: &str| None).unwrap();
    let server_values = HashMap::from([
        ("AIPPT_RUNTIME_PROFILE", "server"),
        ("AIPPT_HOST", "127.0.0.1"),
        ("AIPPT_PORT", "9100"),
        ("AIPPT_CORS_ORIGINS", "https://office.example.com"),
        ("AIPPT_IDENTITY_POLICY", "jwt-only"),
        (
            "AIPPT_JWT_SECRET",
            "test-only-0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ",
        ),
        ("LLM_TEXT_API_KEY", "test-text"),
        ("LLM_IMAGE_API_KEY", "test-image"),
        ("LLM_VIDEO_API_KEY", "test-video"),
    ]);
    let server = RuntimeProfile::load_and_validate(&|key: &str| {
        server_values.get(key).map(|value| (*value).to_owned())
    })
    .unwrap();
    assert_ne!(local.bind_addr(), server.bind_addr());

    let config = source("bootstrap/config.rs");
    assert!(!config.contains("OnceLock"));
    assert!(!config.contains("pub fn config()"));
    assert!(!config.contains("pub fn initialize()"));
    assert!(
        !PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src/state.rs")
            .exists()
    );
    assert!(
        !PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src/app/state.rs")
            .exists()
    );
}
