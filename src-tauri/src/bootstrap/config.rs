use std::env;
use std::net::{IpAddr, SocketAddr};

use anyhow::Result;
use secrecy::SecretString;

use axum::http::{HeaderValue, Method, header};
use tower_http::cors::CorsLayer;

pub use crate::application::identity::IdentityPolicy;
pub use crate::providers::credentials::CredentialBackend;

pub trait ConfigSource {
    fn get(&self, key: &str) -> Option<String>;
}

impl<F: Fn(&str) -> Option<String>> ConfigSource for F {
    fn get(&self, key: &str) -> Option<String> {
        self(key)
    }
}

pub struct Environment;

impl ConfigSource for Environment {
    fn get(&self, key: &str) -> Option<String> {
        std::env::var(key).ok()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeProfile {
    LocalDesktop,
    Server,
}

/// Contains only validated, non-secret startup settings. Construction is private.
#[derive(Clone, Debug)]
pub struct ValidatedRuntimeConfig {
    profile: RuntimeProfile,
    bind_addr: SocketAddr,
    cors_origins: Vec<HeaderValue>,
    identity_policy: IdentityPolicy,
    credential_backend: CredentialBackend,
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BootstrapError {
    #[error("missing required startup setting: {0}")]
    Missing(&'static str),
    #[error("invalid startup setting: {0}")]
    Invalid(&'static str),
    #[error("startup initialization failed")]
    Startup(#[source] anyhow::Error),
}

fn value(source: &dyn ConfigSource, key: &str) -> Option<String> {
    source.get(key).filter(|v| !v.trim().is_empty())
}

fn required(source: &dyn ConfigSource, key: &'static str) -> Result<String, BootstrapError> {
    value(source, key).ok_or(BootstrapError::Missing(key))
}

impl RuntimeProfile {
    pub fn load_and_validate(
        source: &dyn ConfigSource,
    ) -> Result<ValidatedRuntimeConfig, BootstrapError> {
        let profile = match source.get("AIPPT_RUNTIME_PROFILE").as_deref() {
            None | Some("local") => Self::LocalDesktop,
            Some("server") => Self::Server,
            _ => return Err(BootstrapError::Invalid("AIPPT_RUNTIME_PROFILE")),
        };
        let server = profile == Self::Server;
        let host = if server {
            required(source, "AIPPT_HOST")?
        } else {
            source
                .get("AIPPT_HOST")
                .unwrap_or_else(|| "127.0.0.1".into())
        };
        let ip: IpAddr = host
            .parse()
            .map_err(|_| BootstrapError::Invalid("AIPPT_HOST"))?;
        if !server && !ip.is_loopback() {
            return Err(BootstrapError::Invalid("AIPPT_HOST"));
        }
        let port = if server {
            required(source, "AIPPT_PORT")?
        } else {
            source.get("AIPPT_PORT").unwrap_or_else(|| "8000".into())
        };
        let port: u16 = port
            .parse()
            .map_err(|_| BootstrapError::Invalid("AIPPT_PORT"))?;
        if port == 0 {
            return Err(BootstrapError::Invalid("AIPPT_PORT"));
        }
        let origins = if server {
            required(source, "AIPPT_CORS_ORIGINS")?
        } else {
            source.get("AIPPT_CORS_ORIGINS").unwrap_or_else(|| {
                "http://localhost:1420,http://127.0.0.1:1420,tauri://localhost,http://tauri.localhost,https://tauri.localhost".into()
            })
        };
        let cors_origins = parse_origins(&origins, !server)?;
        let identity_policy = match (server, source.get("AIPPT_IDENTITY_POLICY").as_deref()) {
            (true, Some("jwt-only")) | (false, Some("jwt-only")) => IdentityPolicy::JwtOnly,
            (false, None | Some("local-guest")) => IdentityPolicy::LocalGuest,
            (true, None) => return Err(BootstrapError::Missing("AIPPT_IDENTITY_POLICY")),
            _ => return Err(BootstrapError::Invalid("AIPPT_IDENTITY_POLICY")),
        };
        if server {
            let secret = zeroize::Zeroizing::new(required(source, "AIPPT_JWT_SECRET")?);
            // At least 256 bits of input; reject obvious repeated/default material.
            // Deployments must supply a cryptographically random secret.
            let distinct = secret
                .bytes()
                .collect::<std::collections::HashSet<_>>()
                .len();
            if secret.len() < 32 || distinct < 16 {
                return Err(BootstrapError::Invalid("AIPPT_JWT_SECRET"));
            }
            // Validate fixed deployment mappings without retaining credentials
            // in the runtime config. The selected store consumes them at startup.
            for key in ["LLM_TEXT_API_KEY", "LLM_IMAGE_API_KEY", "LLM_VIDEO_API_KEY"] {
                let keys = zeroize::Zeroizing::new(required(source, key)?);
                if !keys.split(',').any(|key| !key.trim().is_empty()) {
                    return Err(BootstrapError::Invalid(key));
                }
            }
        }
        Ok(ValidatedRuntimeConfig {
            profile,
            bind_addr: SocketAddr::new(ip, port),
            cors_origins,
            identity_policy,
            credential_backend: if server {
                CredentialBackend::Environment
            } else {
                CredentialBackend::Native
            },
        })
    }
}

fn parse_origins(origins: &str, local: bool) -> Result<Vec<HeaderValue>, BootstrapError> {
    let invalid = || BootstrapError::Invalid("AIPPT_CORS_ORIGINS");
    origins
        .split(',')
        .map(|origin| {
            let origin = origin.trim();
            let url = reqwest::Url::parse(origin).map_err(|_| invalid())?;
            let host = url.host_str().ok_or_else(invalid)?;
            if origin.contains('*')
                || !matches!(url.scheme(), "http" | "https" | "tauri")
                || !url.username().is_empty()
                || url.password().is_some()
                || url.query().is_some()
                || url.fragment().is_some()
                || !matches!(url.path(), "" | "/")
                || origin.ends_with('/')
                || (url.scheme() == "tauri" && origin != "tauri://localhost")
            {
                return Err(invalid());
            }
            if local
                && host != "localhost"
                && host != "tauri.localhost"
                && !host
                    .trim_matches(['[', ']'])
                    .parse::<IpAddr>()
                    .is_ok_and(|ip| ip.is_loopback())
            {
                return Err(invalid());
            }
            HeaderValue::from_str(origin).map_err(|_| invalid())
        })
        .collect()
}

impl ValidatedRuntimeConfig {
    pub fn profile(&self) -> RuntimeProfile {
        self.profile
    }
    pub fn bind_addr(&self) -> SocketAddr {
        self.bind_addr
    }
    pub fn cors_origins(&self) -> &[HeaderValue] {
        &self.cors_origins
    }
    pub fn identity_policy(&self) -> IdentityPolicy {
        self.identity_policy
    }
    pub fn credential_backend(&self) -> CredentialBackend {
        self.credential_backend
    }

    pub fn cors_layer(&self) -> CorsLayer {
        CorsLayer::new()
            .allow_origin(self.cors_origins.clone())
            .allow_methods([
                Method::GET,
                Method::POST,
                Method::PUT,
                Method::PATCH,
                Method::DELETE,
                Method::OPTIONS,
            ])
            .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE, header::ACCEPT])
    }
}

pub(crate) struct AppConfig {
    pub runtime: ValidatedRuntimeConfig,
    pub app_name: String,
    pub jwt_secret: SecretString,
    pub jwt_expiry_hours: i64,
    pub llm_base_url: String,
    pub llm_model: String,
    pub llm_text_models: Vec<String>,
    pub llm_image_base_url: String,
    pub llm_image_models: Vec<String>,
    pub llm_video_base_url: String,
    pub llm_video_models: Vec<String>,
    pub llm_provider: String,
    pub llm_tool_timeout_ms: u64,
    pub llm_chat_timeout_ms: u64,
    pub web_search_provider: String,
    pub web_search_endpoint: String,
    pub web_search_timeout_ms: u64,
    pub baidu_mcp_sse_endpoint: String,
    pub data_dir: String,
    pub projects_dir: String,
    pub sessions_dir: String,
    pub render_output_dir: String,
    pub database_url: String,
    pub db_max_connections: u32,
}

impl std::fmt::Debug for AppConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AppConfig")
            .field("runtime", &self.runtime)
            .finish_non_exhaustive()
    }
}

impl AppConfig {
    pub fn load() -> Result<Self> {
        dotenvy::from_path(concat!(env!("CARGO_MANIFEST_DIR"), "/../.env")).ok();
        dotenvy::from_path(concat!(env!("CARGO_MANIFEST_DIR"), "/.env")).ok();
        dotenvy::dotenv().ok();

        let runtime = RuntimeProfile::load_and_validate(&Environment)?;
        let jwt_secret = env::var("AIPPT_JWT_SECRET")
            .ok()
            .filter(|secret| !secret.trim().is_empty())
            .unwrap_or_else(|| format!("{}{}", uuid::Uuid::new_v4(), uuid::Uuid::new_v4()));
        let llm_base_url =
            nonsecret_endpoint("LLM_TEXT_BASE_URL", env_or_required("LLM_TEXT_BASE_URL")?)?;
        let llm_image_base_url =
            nonsecret_endpoint("LLM_IMAGE_BASE_URL", env_or_required("LLM_IMAGE_BASE_URL")?)?;
        let llm_video_base_url =
            nonsecret_endpoint("LLM_VIDEO_BASE_URL", env_or_required("LLM_VIDEO_BASE_URL")?)?;
        let llm_text_models = model_list("LLM_TEXT_MODELS", "LLM_TEXT_MODEL")?;
        let llm_image_models = model_list("LLM_IMAGE_MODELS", "LLM_IMAGE_MODEL")?;
        let llm_video_models = model_list("LLM_VIDEO_MODELS", "LLM_VIDEO_MODEL")?;
        let llm_model = default_model("LLM_TEXT_MODELS_DEFAULT", &llm_text_models);
        let data_dir = env_or("AIPPT_DATA_DIR", "data");
        let database_url = env_or(
            "DATABASE_URL",
            &format!("sqlite://{data_dir}/revueOffice.db?mode=rwc"),
        );

        Ok(Self {
            runtime,
            app_name: env_or("AIPPT_APP_NAME", "revueOffice"),
            jwt_secret: SecretString::new(jwt_secret),
            jwt_expiry_hours: env_or("AIPPT_JWT_EXPIRY_HOURS", "24").parse().unwrap_or(24),
            llm_base_url,
            llm_model,
            llm_text_models,
            llm_image_base_url,
            llm_image_models,
            llm_video_base_url,
            llm_video_models,
            llm_provider: env_or("AIPPT_LLM_PROVIDER", "glm-gateway"),
            llm_tool_timeout_ms: env_or("AIPPT_LLM_TOOL_TIMEOUT_MS", "1800000")
                .parse()
                .unwrap_or(1_800_000),
            llm_chat_timeout_ms: env_or("AIPPT_LLM_CHAT_TIMEOUT_MS", "1800000")
                .parse()
                .unwrap_or(1_800_000),
            web_search_provider: env_or("AIPPT_WEB_SEARCH_PROVIDER", "auto"),
            web_search_endpoint: env_or("AIPPT_WEB_SEARCH_ENDPOINT", "http://127.0.0.1:8080"),
            web_search_timeout_ms: env_or("AIPPT_WEB_SEARCH_TIMEOUT_MS", "20000")
                .parse()
                .unwrap_or(20_000),
            baidu_mcp_sse_endpoint: nonsecret_endpoint(
                "AIPPT_BAIDU_MCP_SSE_ENDPOINT",
                env_or(
                    "AIPPT_BAIDU_MCP_SSE_ENDPOINT",
                    "http://appbuilder.baidu.com/v2/ai_search/mcp/sse",
                ),
            )?,
            projects_dir: env_or("AIPPT_PROJECTS_DIR", "data/projects"),
            sessions_dir: env_or("AIPPT_SESSIONS_DIR", "data/sessions"),
            render_output_dir: env_or("AIPPT_RENDER_OUTPUT_DIR", "outputs"),
            database_url,
            db_max_connections: env_or("DB_MAX_CONNECTIONS", "8").parse().unwrap_or(8),
            data_dir,
        })
    }

    pub fn ensure_dirs(&self) -> Result<()> {
        for directory in [
            &self.data_dir,
            &self.projects_dir,
            &self.sessions_dir,
            &self.render_output_dir,
        ] {
            std::fs::create_dir_all(directory)?;
        }
        Ok(())
    }

    pub fn is_mysql(&self) -> bool {
        self.database_url.starts_with("mysql://")
    }
}

fn nonsecret_endpoint(name: &'static str, raw: String) -> Result<String> {
    let raw = zeroize::Zeroizing::new(raw);
    let url = reqwest::Url::parse(&raw)
        .map_err(|_| anyhow::anyhow!("Invalid endpoint setting: {name}"))?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(anyhow::anyhow!(
            "Endpoint setting must not contain credentials or query/fragment: {name}"
        ));
    }
    Ok(url.to_string().trim_end_matches('/').to_owned())
}

fn model_list(list_key: &'static str, fallback_key: &'static str) -> Result<Vec<String>> {
    match env::var(list_key) {
        Ok(value) if !value.trim().is_empty() => Ok(split_env_list(&value)),
        _ => Ok(vec![env_or_required(fallback_key)?]),
    }
}

fn default_model(key: &str, models: &[String]) -> String {
    let configured = env_or(key, "");
    if configured.trim().is_empty() {
        models.first().cloned().unwrap_or_default()
    } else {
        configured
    }
}

fn env_or(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_owned())
}

fn env_or_required(key: &'static str) -> Result<String> {
    env::var(key)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("Missing required environment variable: {key}"))
}

fn split_env_list(value: &str) -> Vec<String> {
    value
        .split([',', ';', '\n'])
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_owned)
        .collect()
}
