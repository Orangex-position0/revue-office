use std::collections::HashMap;

use axum::{
    Router,
    body::Body,
    http::{Request, header},
    routing::get,
};
use revue_office_lib::bootstrap::config::{
    BootstrapError, CredentialBackend, IdentityPolicy, RuntimeProfile, ValidatedRuntimeConfig,
};
use tower::ServiceExt;

fn load(values: &HashMap<&str, &str>) -> Result<ValidatedRuntimeConfig, BootstrapError> {
    RuntimeProfile::load_and_validate(&|key: &str| values.get(key).map(|v| (*v).to_owned()))
}

fn server() -> HashMap<&'static str, &'static str> {
    HashMap::from([
        ("AIPPT_RUNTIME_PROFILE", "server"),
        ("AIPPT_HOST", "0.0.0.0"),
        ("AIPPT_PORT", "8000"),
        ("AIPPT_CORS_ORIGINS", "https://office.example.com"),
        ("AIPPT_IDENTITY_POLICY", "jwt-only"),
        (
            "AIPPT_JWT_SECRET",
            "test-only-0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ",
        ),
        ("LLM_TEXT_API_KEY", "test-text"),
        ("LLM_IMAGE_API_KEY", "test-image"),
        ("LLM_VIDEO_API_KEY", "test-video"),
    ])
}

#[test]
fn desktop_defaults_are_loopback_and_restricted() {
    let cfg = load(&HashMap::new()).unwrap();
    assert_eq!(cfg.profile(), RuntimeProfile::LocalDesktop);
    assert_eq!(cfg.bind_addr().to_string(), "127.0.0.1:8000");
    assert_eq!(cfg.identity_policy(), IdentityPolicy::LocalGuest);
    assert_eq!(cfg.credential_backend(), CredentialBackend::Native);
    assert!(cfg.cors_origins().iter().any(|v| v == "tauri://localhost"));
    assert!(cfg.cors_origins().iter().all(|v| v != "*"));
}

#[test]
fn desktop_rejects_non_loopback_bind_and_remote_origins() {
    for host in ["0.0.0.0", "::", "192.168.1.5", "localhost", ""] {
        assert!(load(&HashMap::from([("AIPPT_HOST", host)])).is_err());
    }
    for host in ["127.0.0.1", "::1"] {
        assert!(load(&HashMap::from([("AIPPT_HOST", host)])).is_ok());
    }
    assert!(
        load(&HashMap::from([(
            "AIPPT_CORS_ORIGINS",
            "https://evil.example"
        )]))
        .is_err()
    );
}

#[test]
fn server_requires_every_security_input_without_local_fallback() {
    for key in server()
        .keys()
        .filter(|key| **key != "AIPPT_RUNTIME_PROFILE")
    {
        for missing in [None, Some(""), Some("   ")] {
            let mut values = server();
            match missing {
                None => {
                    values.remove(key);
                }
                Some(value) => {
                    values.insert(key, value);
                }
            }
            assert!(load(&values).is_err(), "missing setting {key} was accepted");
        }
    }
}

#[test]
fn server_has_explicit_policy_and_environment_backend() {
    let cfg = load(&server()).unwrap();
    assert_eq!(cfg.profile(), RuntimeProfile::Server);
    assert_eq!(cfg.bind_addr().to_string(), "0.0.0.0:8000");
    assert_eq!(cfg.identity_policy(), IdentityPolicy::JwtOnly);
    assert_eq!(cfg.credential_backend(), CredentialBackend::Environment);
    assert_eq!(cfg.cors_origins().len(), 1);
}

#[test]
fn rejects_invalid_profile_port_identity_and_secrets_without_echoing_values() {
    for (key, value) in [
        ("AIPPT_RUNTIME_PROFILE", "unknown"),
        ("AIPPT_PORT", "bad"),
        ("AIPPT_PORT", "65536"),
        ("AIPPT_PORT", "0"),
        ("AIPPT_IDENTITY_POLICY", "local-guest"),
        ("AIPPT_JWT_SECRET", "short-secret-sentinel"),
        (
            "AIPPT_JWT_SECRET",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ),
        ("LLM_TEXT_API_KEY", ", ,"),
    ] {
        let mut values = server();
        values.insert(key, value);
        let error = load(&values).unwrap_err();
        assert!(!format!("{error:?} {error}").contains(value));
    }
}

#[test]
fn rejects_wildcard_opaque_and_malformed_origins() {
    for origin in [
        "*",
        "https://*.example.com",
        "null",
        "https://example.com/path",
        "https://example.com/",
        "https://user:secret@example.com",
        "https://example.com?token=secret",
        "https://example.com#fragment",
        "ftp://example.com",
        "https://example.com,",
        "https://example.com\r\nX-Secret: value",
    ] {
        let mut values = server();
        values.insert("AIPPT_CORS_ORIGINS", origin);
        assert!(load(&values).is_err(), "malformed origin was accepted");
    }
}

async fn preflight(
    cfg: &ValidatedRuntimeConfig,
    origin: &str,
    method: &str,
    headers: &str,
) -> axum::http::Response<Body> {
    Router::new()
        .route("/probe", get(|| async { "ok" }))
        .layer(cfg.cors_layer())
        .oneshot(
            Request::builder()
                .method("OPTIONS")
                .uri("/probe")
                .header(header::ORIGIN, origin)
                .header(header::ACCESS_CONTROL_REQUEST_METHOD, method)
                .header(header::ACCESS_CONTROL_REQUEST_HEADERS, headers)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn cors_allows_desktop_and_server_allowlists_but_not_arbitrary_origins() {
    for (cfg, origin) in [
        (load(&HashMap::new()).unwrap(), "http://localhost:1420"),
        (load(&server()).unwrap(), "https://office.example.com"),
    ] {
        let response = preflight(&cfg, origin, "POST", "authorization,content-type").await;
        assert_eq!(
            response.headers()[header::ACCESS_CONTROL_ALLOW_ORIGIN],
            origin
        );
        assert_ne!(
            response.headers()[header::ACCESS_CONTROL_ALLOW_METHODS],
            "*"
        );
        assert_ne!(
            response.headers()[header::ACCESS_CONTROL_ALLOW_HEADERS],
            "*"
        );
        let rejected = preflight(&cfg, "https://evil.example", "POST", "authorization").await;
        assert!(
            !rejected
                .headers()
                .contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN)
        );
        let unknown = preflight(&cfg, origin, "TRACE", "x-untrusted").await;
        assert!(
            !unknown.headers()[header::ACCESS_CONTROL_ALLOW_METHODS]
                .to_str()
                .unwrap()
                .contains("TRACE")
        );
        assert!(
            !unknown.headers()[header::ACCESS_CONTROL_ALLOW_HEADERS]
                .to_str()
                .unwrap()
                .contains("x-untrusted")
        );
    }
}

#[test]
fn validated_debug_contains_no_secret_material() {
    let values = server();
    let debug = format!("{:?}", load(&values).unwrap());
    for key in [
        "AIPPT_JWT_SECRET",
        "LLM_TEXT_API_KEY",
        "LLM_IMAGE_API_KEY",
        "LLM_VIDEO_API_KEY",
    ] {
        assert!(!debug.contains(values[key]));
    }
}
