use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolAttachment {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub mime_type: String,
    #[serde(default)]
    pub size: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_url: Option<String>,
}

pub type ToolProgressEmitter = Arc<dyn Fn(&str, serde_json::Value) + Send + Sync>;

/// 工具执行上下文
#[derive(Clone)]
pub struct ToolContext {
    provider_resolver: Option<Arc<dyn crate::providers::ChatProviderResolver>>,
    pub session_id: String,
    pub user_id: String,
    pub project_id: Option<String>,
    pub preferred_model: Option<String>,
    pub attachments: Vec<ToolAttachment>,
    /// SSE 推送回调（向前端发送实时进度）
    pub emit: ToolProgressEmitter,
    /// 共享上下文（跨工具传递，如 PPT 大纲规划）
    pub scratchpad: Arc<Mutex<HashMap<String, serde_json::Value>>>,
    /// 用户工具配置（前端传入，如视频时长、宽高比等）
    pub tool_config: Option<serde_json::Value>,
}

impl ToolContext {
    pub fn new(
        session_id: String,
        user_id: String,
        project_id: Option<String>,
        preferred_model: Option<String>,
        attachments: Vec<ToolAttachment>,
        emit: impl Fn(&str, serde_json::Value) + Send + Sync + 'static,
    ) -> Self {
        Self {
            provider_resolver: None,
            session_id,
            user_id,
            project_id,
            preferred_model,
            attachments,
            emit: Arc::new(emit),
            scratchpad: Arc::new(Mutex::new(HashMap::new())),
            tool_config: None,
        }
    }

    pub fn with_provider_resolver(
        mut self,
        resolver: Arc<dyn crate::providers::ChatProviderResolver>,
    ) -> Self {
        self.provider_resolver = Some(resolver);
        self
    }
    pub fn provider_resolver(&self) -> Option<&Arc<dyn crate::providers::ChatProviderResolver>> {
        self.provider_resolver.as_ref()
    }

    pub fn with_tool_config(mut self, config: serde_json::Value) -> Self {
        self.tool_config = Some(config);
        self
    }

    pub fn with_scratchpad(
        mut self,
        scratchpad: Arc<Mutex<HashMap<String, serde_json::Value>>>,
    ) -> Self {
        self.scratchpad = scratchpad;
        self
    }

    /// 获取工具配置中的某个字段值
    pub fn get_config<T: serde::de::DeserializeOwned>(&self, key: &str) -> Option<T> {
        self.tool_config
            .as_ref()
            .and_then(|cfg| cfg.get(key))
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    pub fn send(&self, event: &str, data: serde_json::Value) {
        (self.emit)(event, data);
    }
}

/// 工具产生的产物
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolArtifact {
    pub kind: String, // document | ppt | drawio | sheet | image | code | mixed
    pub title: String,
    pub content: serde_json::Value,
    #[serde(default = "default_artifact_extension")]
    pub extension: String,
    #[serde(default)]
    pub bytes: Vec<u8>,
}

fn default_artifact_extension() -> String {
    "json".into()
}

/// 工具结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifacts: Option<Vec<ToolArtifact>>,
    /// 给 LLM 的观察文本（ReAct Observation）
    pub observation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub continue_loop: Option<bool>,
}

impl ToolResult {
    pub fn ok(observation: impl Into<String>, artifacts: Vec<ToolArtifact>) -> Self {
        Self {
            success: true,
            data: None,
            error: None,
            artifacts: Some(artifacts),
            observation: observation.into(),
            continue_loop: None,
        }
    }

    pub fn err(observation: impl Into<String>) -> Self {
        let obs = observation.into();
        Self {
            success: false,
            data: None,
            error: Some(obs.clone()),
            artifacts: None,
            observation: obs,
            continue_loop: None,
        }
    }
}

/// 工具定义 trait
#[async_trait]
pub trait OfficeTool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    /// JSON Schema 参数定义
    fn parameters(&self) -> serde_json::Value;
    fn is_read_only(&self) -> bool {
        false
    }
    fn produces_artifact(&self) -> bool {
        true
    }

    async fn call(&self, input: serde_json::Value, ctx: &ToolContext) -> ToolResult;
}

/// 便利类型别名
pub type DynTool = Arc<dyn OfficeTool>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegacyToolProgress {
    pub event: String,
    pub data: serde_json::Value,
}

#[derive(Clone)]
pub struct LegacyToolProgressAdapter {
    sender: tokio::sync::mpsc::Sender<LegacyToolProgress>,
}

impl LegacyToolProgressAdapter {
    pub fn bounded(capacity: usize) -> (Self, tokio::sync::mpsc::Receiver<LegacyToolProgress>) {
        let (sender, receiver) = tokio::sync::mpsc::channel(capacity.max(1));
        (Self { sender }, receiver)
    }

    pub fn callback(&self) -> impl Fn(&str, serde_json::Value) + Send + Sync + 'static + use<> {
        let sender = self.sender.clone();
        move |event, data| {
            let progress = LegacyToolProgress {
                event: event.to_owned(),
                data,
            };
            match sender.try_send(progress) {
                Ok(()) | Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {}
                Err(tokio::sync::mpsc::error::TrySendError::Full(progress)) => {
                    let sender = sender.clone();
                    tokio::spawn(async move {
                        let _ = sender.send(progress).await;
                    });
                }
            }
        }
    }
}

impl ToolResult {
    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.data = Some(data);
        self
    }
}
