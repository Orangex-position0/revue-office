use serde::{Deserialize, Serialize};
use serde_json::Value;
use zeroize::Zeroize;

#[derive(Serialize, Deserialize)]
pub struct LlmProfileConfig {
    pub id: String,
    pub name: String,
    pub base_url: String,
    #[serde(default, serialize_with = "serialize_empty_keys")]
    pub api_keys: Vec<String>,
    pub models: Vec<String>,
    pub default_model: String,
    #[serde(default, skip_serializing)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub has_api_key: bool,
    #[serde(default)]
    pub api_key_count: usize,
}

fn serialize_empty_keys<S: serde::Serializer>(
    _: &[String],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    Vec::<String>::new().serialize(serializer)
}

impl std::fmt::Debug for LlmProfileConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LlmProfileConfig")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("base_url", &"[configured endpoint]")
            .field("models", &self.models)
            .field("default_model", &self.default_model)
            .field("credentials", &"[REDACTED]")
            .finish()
    }
}

impl Drop for LlmProfileConfig {
    fn drop(&mut self) {
        self.api_keys.zeroize();
        self.api_key.zeroize();
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BasicSettings {
    pub app_name: String,
    pub workspace_title: String,
    pub brand_tagline: String,
    pub default_theme: String,
}

#[derive(Serialize, Deserialize)]
pub struct McpServerConfig {
    pub id: String,
    pub name: String,
    pub transport: String,
    #[serde(serialize_with = "serialize_safe_endpoint")]
    pub endpoint: String,
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

fn serialize_safe_endpoint<S: serde::Serializer>(
    raw: &str,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    // Stored/returned endpoints have already passed the secure adapter. Keep a
    // defensive guard so a raw request DTO can never serialize URL credentials.
    let authority = raw.split_once("://").map(|(_, rest)| rest).unwrap_or(raw);
    let authority = authority.split(['/', '?', '#']).next().unwrap_or_default();
    let secret_query = raw.split_once('?').is_some_and(|(_, query)| {
        query.split('&').any(|part| {
            let name = part
                .split('=')
                .next()
                .unwrap_or_default()
                .to_ascii_lowercase();
            matches!(
                name.as_str(),
                "api_key" | "apikey" | "key" | "token" | "access_token" | "secret"
            )
        })
    });
    if authority.contains('@') || secret_query {
        "[REDACTED ENDPOINT]".serialize(serializer)
    } else {
        raw.serialize(serializer)
    }
}

impl Drop for McpServerConfig {
    fn drop(&mut self) {
        self.endpoint.zeroize();
    }
}

impl std::fmt::Debug for McpServerConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpServerConfig")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("transport", &self.transport)
            .field("endpoint", &"[configured endpoint]")
            .field("enabled", &self.enabled)
            .finish()
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AppSettings {
    pub llm_profiles: Vec<LlmProfileConfig>,
    pub active_profile_id: String,
    pub default_model: String,
    pub active_model: String,
    pub basic: BasicSettings,
    #[serde(default)]
    pub mcp_servers: Vec<McpServerConfig>,
    pub updated_at: String,
}

#[derive(Clone)]
pub struct PreferenceDefaults {
    pub app_name: String,
    pub provider_endpoint: String,
    pub provider_model: String,
    pub provider_models: Vec<String>,
    pub search_endpoint: String,
}

impl PreferenceDefaults {
    pub fn new(
        app_name: String,
        provider_endpoint: String,
        provider_model: String,
        provider_models: impl IntoIterator<Item = String>,
        search_endpoint: String,
    ) -> Self {
        let mut models = Vec::new();
        for model in provider_models {
            let model = model.trim();
            if !model.is_empty() && !models.iter().any(|known| known == model) {
                models.push(model.to_owned());
            }
        }
        if models.is_empty() {
            models.push(provider_model.clone());
        }
        Self {
            app_name,
            provider_endpoint,
            provider_model,
            provider_models: models,
            search_endpoint,
        }
    }
}

/// Secret-bearing transport command. It deliberately implements neither
/// `Debug` nor `Serialize`, and recursively wipes owned strings on drop.
pub struct PreferenceUpdate(Value);

impl PreferenceUpdate {
    pub fn new(value: Value) -> Self {
        Self(value)
    }

    pub(crate) fn value_mut(&mut self) -> &mut Value {
        &mut self.0
    }

    pub fn take(&mut self) -> Value {
        std::mem::take(&mut self.0)
    }
}

impl Drop for PreferenceUpdate {
    fn drop(&mut self) {
        fn wipe(value: &mut Value) {
            match value {
                Value::String(text) => text.zeroize(),
                Value::Array(values) => values.iter_mut().for_each(wipe),
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

pub struct McpConnectionRequest {
    pub id: String,
    pub transport: String,
    pub endpoint: String,
}

impl McpConnectionRequest {
    pub fn from_value(value: Value) -> Result<Self, super::PreferenceError> {
        let mut server: McpServerConfig = serde_json::from_value(value)
            .map_err(|_| super::PreferenceError::Invalid("MCP 服务配置格式无效".into()))?;
        Ok(Self {
            id: std::mem::take(&mut server.id),
            transport: std::mem::take(&mut server.transport),
            endpoint: std::mem::take(&mut server.endpoint),
        })
    }
}

impl Drop for McpConnectionRequest {
    fn drop(&mut self) {
        self.endpoint.zeroize();
    }
}

#[derive(Debug, Serialize, PartialEq)]
pub struct McpConnectionResult {
    pub ok: bool,
    pub message: String,
    pub tools: Vec<Value>,
}
