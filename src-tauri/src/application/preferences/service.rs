use std::sync::Arc;

use serde_json::{Value, json};

use crate::application::identity::Actor;

use super::{
    AppSettings, McpConnectionRequest, McpConnectionResult, McpConnectionTester,
    PreferenceDefaults, PreferenceError, PreferenceUpdate, SecurePreferencePort,
};

pub struct PreferenceApplicationService {
    store: Arc<dyn SecurePreferencePort>,
    mcp: Arc<dyn McpConnectionTester>,
    defaults: PreferenceDefaults,
}

impl PreferenceApplicationService {
    pub fn new(
        store: Arc<dyn SecurePreferencePort>,
        mcp: Arc<dyn McpConnectionTester>,
        defaults: PreferenceDefaults,
    ) -> Self {
        Self {
            store,
            mcp,
            defaults,
        }
    }

    pub async fn get(&self, actor: &Actor) -> Result<AppSettings, PreferenceError> {
        let value = match self.store.load(&actor.id).await? {
            Some(value) => value,
            None => self.default_document(actor).await?,
        };
        serde_json::from_value(value).map_err(|_| PreferenceError::Invalid("设置格式无效".into()))
    }

    pub async fn save(
        &self,
        actor: &Actor,
        mut update: PreferenceUpdate,
    ) -> Result<AppSettings, PreferenceError> {
        self.normalize(update.value_mut())?;
        let value = self.store.save(&actor.id, update).await?;
        serde_json::from_value(value).map_err(|_| PreferenceError::Invalid("设置格式无效".into()))
    }

    pub async fn test_mcp(
        &self,
        actor: &Actor,
        request: McpConnectionRequest,
    ) -> Result<McpConnectionResult, PreferenceError> {
        if !matches!(request.transport.as_str(), "http" | "sse") {
            return Err(PreferenceError::Invalid("仅支持 HTTP/SSE MCP 服务".into()));
        }
        match self.mcp.test(&actor.id, request).await? {
            Some(tools) => Ok(McpConnectionResult {
                ok: true,
                message: format!("MCP 服务连接成功，共发现 {} 个工具", tools.len()),
                tools,
            }),
            None => Ok(McpConnectionResult {
                ok: false,
                message: "MCP 服务连接或协议验证失败".into(),
                tools: Vec::new(),
            }),
        }
    }

    async fn default_document(&self, actor: &Actor) -> Result<Value, PreferenceError> {
        let count = self
            .store
            .chat_credential_count(&actor.id, "default", &self.defaults.provider_endpoint)
            .await?;
        let mut servers = Vec::new();
        if self
            .store
            .has_search_credentials(
                &actor.id,
                "builtin-baidu-ai-search",
                &self.defaults.search_endpoint,
            )
            .await?
        {
            servers.push(json!({
                "id": "builtin-baidu-ai-search",
                "name": "百度 AI Search MCP",
                "transport": "sse",
                "endpoint": self.defaults.search_endpoint,
                "enabled": true
            }));
        }
        Ok(json!({
            "llm_profiles": [{
                "id": "default",
                "name": "默认模型服务",
                "base_url": self.defaults.provider_endpoint,
                "api_keys": [],
                "models": self.defaults.provider_models,
                "default_model": self.defaults.provider_model,
                "has_api_key": count > 0,
                "api_key_count": count
            }],
            "active_profile_id": "default",
            "default_model": self.defaults.provider_model,
            "active_model": self.defaults.provider_model,
            "basic": {
                "app_name": self.defaults.app_name,
                "workspace_title": "智能办公助手",
                "brand_tagline": "打开即用，专注办公创作",
                "default_theme": "default"
            },
            "mcp_servers": servers,
            "updated_at": chrono::Utc::now().to_rfc3339()
        }))
    }

    fn normalize(&self, value: &mut Value) -> Result<(), PreferenceError> {
        let invalid = || PreferenceError::Invalid("模型服务配置无效".into());
        if !value.is_object() || !value["basic"].is_object() {
            return Err(invalid());
        }
        let active_id = value["active_profile_id"].clone();
        let profiles = value
            .get_mut("llm_profiles")
            .and_then(Value::as_array_mut)
            .filter(|profiles| !profiles.is_empty())
            .ok_or_else(invalid)?;
        for profile in profiles.iter_mut() {
            if !profile.is_object() {
                return Err(invalid());
            }
            if profile["id"].as_str().is_none_or(|id| id.trim().is_empty()) {
                profile["id"] = json!(uuid::Uuid::new_v4().to_string());
            }
            if profile["name"]
                .as_str()
                .is_none_or(|name| name.trim().is_empty())
            {
                profile["name"] = json!("未命名模型服务");
            }
            if profile["base_url"]
                .as_str()
                .is_none_or(|endpoint| endpoint.trim().is_empty())
            {
                return Err(invalid());
            }
            let requested_default = profile["default_model"].clone();
            let models = profile
                .get_mut("models")
                .and_then(Value::as_array_mut)
                .ok_or_else(invalid)?;
            for model in models.iter_mut() {
                let normalized = model.as_str().ok_or_else(invalid)?.trim().to_owned();
                *model = json!(normalized);
            }
            models.retain(|model| model.as_str().is_some_and(|model| !model.is_empty()));
            if models.is_empty() {
                return Err(invalid());
            }
            profile["default_model"] = if models.contains(&requested_default) {
                requested_default
            } else {
                models[0].clone()
            };
        }
        value["active_profile_id"] = if profiles.iter().any(|profile| profile["id"] == active_id) {
            active_id
        } else {
            profiles[0]["id"].clone()
        };
        let active = value["llm_profiles"]
            .as_array()
            .and_then(|profiles| {
                profiles
                    .iter()
                    .find(|profile| profile["id"] == value["active_profile_id"])
            })
            .ok_or_else(invalid)?;
        let models = active["models"].as_array().ok_or_else(invalid)?.clone();
        let profile_default = active["default_model"].clone();
        let requested_default = value["default_model"].clone();
        let requested_active = value["active_model"].clone();
        let selected_default = if models.contains(&requested_default) {
            requested_default
        } else {
            profile_default
        };
        value["default_model"] = selected_default.clone();
        value["active_model"] = if models.contains(&requested_active) {
            requested_active
        } else {
            selected_default
        };
        for (field, default) in [
            ("app_name", self.defaults.app_name.as_str()),
            ("workspace_title", "智能办公助手"),
            ("brand_tagline", "打开即用，专注办公创作"),
            ("default_theme", "default"),
        ] {
            if value["basic"][field]
                .as_str()
                .is_none_or(|field| field.trim().is_empty())
            {
                value["basic"][field] = json!(default);
            }
        }
        if value.get("mcp_servers").is_none() {
            value["mcp_servers"] = json!([]);
        }
        value["updated_at"] = json!(chrono::Utc::now().to_rfc3339());
        Ok(())
    }
}
