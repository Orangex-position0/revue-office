use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::contracts::presentation::PresentationProgress;
use crate::models::ChatAttachment;
use crate::ports::presentation_progress::{PresentationProgressError, PresentationProgressSink};

/// 工具执行上下文
#[derive(Clone)]
pub struct ToolContext {
    pub session_id: String,
    pub user_id: String,
    pub project_id: Option<String>,
    pub preferred_model: Option<String>,
    pub attachments: Vec<ChatAttachment>,
    /// SSE 推送回调（向前端发送实时进度）
    pub emit: Arc<dyn Fn(&str, serde_json::Value) + Send + Sync>,
    /// 共享上下文（跨工具传递，如 PPT 大纲规划）
    pub scratchpad: Arc<Mutex<HashMap<String, serde_json::Value>>>,
    /// 已迁移演示文稿工具使用的类型化进度通道。
    presentation_progress: Option<Arc<dyn PresentationProgressSink>>,
    /// 用户工具配置（前端传入，如视频时长、宽高比等）
    pub tool_config: Option<serde_json::Value>,
}

impl ToolContext {
    pub fn new(
        session_id: String,
        user_id: String,
        project_id: Option<String>,
        preferred_model: Option<String>,
        attachments: Vec<ChatAttachment>,
        emit: impl Fn(&str, serde_json::Value) + Send + Sync + 'static,
    ) -> Self {
        Self {
            session_id,
            user_id,
            project_id,
            preferred_model,
            attachments,
            emit: Arc::new(emit),
            scratchpad: Arc::new(Mutex::new(HashMap::new())),
            presentation_progress: None,
            tool_config: None,
        }
    }

    pub fn with_tool_config(mut self, config: serde_json::Value) -> Self {
        self.tool_config = Some(config);
        self
    }

    pub fn with_presentation_progress(
        mut self,
        progress: Arc<dyn PresentationProgressSink>,
    ) -> Self {
        self.presentation_progress = Some(progress);
        self
    }

    pub fn presentation_progress(&self) -> Option<Arc<dyn PresentationProgressSink>> {
        self.presentation_progress.clone()
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

    pub fn callback(&self) -> impl Fn(&str, serde_json::Value) + Send + Sync + 'static {
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

/// Typed, backpressured progress channel for migrated tools.
#[derive(Clone)]
pub struct ToolProgressSink {
    tool: String,
    events: super::runtime::RuntimeEventSink,
}

impl ToolProgressSink {
    pub(crate) fn new(tool: String, events: super::runtime::RuntimeEventSink) -> Self {
        Self { tool, events }
    }

    pub async fn started(
        &self,
        input: serde_json::Value,
    ) -> Result<(), super::runtime::RuntimeError> {
        self.events
            .emit(super::event::RuntimeEvent::ToolStarted {
                tool: self.tool.clone(),
                input,
            })
            .await
    }

    pub async fn progress(
        &self,
        stage: impl Into<String>,
        detail: serde_json::Value,
    ) -> Result<(), super::runtime::RuntimeError> {
        self.events
            .emit(super::event::RuntimeEvent::ToolProgress {
                tool: self.tool.clone(),
                stage: stage.into(),
                detail,
            })
            .await
    }

    pub async fn finished(
        &self,
        success: bool,
        result: serde_json::Value,
    ) -> Result<(), super::runtime::RuntimeError> {
        self.events
            .emit(super::event::RuntimeEvent::ToolFinished {
                tool: self.tool.clone(),
                success,
                result,
            })
            .await
    }
}

pub struct PresentationToolProgressAdapter {
    sender: tokio::sync::mpsc::Sender<PresentationProgress>,
}

impl PresentationToolProgressAdapter {
    pub fn new(sender: tokio::sync::mpsc::Sender<PresentationProgress>) -> Self {
        Self { sender }
    }
}

#[async_trait]
impl PresentationProgressSink for PresentationToolProgressAdapter {
    async fn emit(&self, progress: PresentationProgress) -> Result<(), PresentationProgressError> {
        self.sender
            .send(progress)
            .await
            .map_err(|_| PresentationProgressError)
    }
}

impl ToolResult {
    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.data = Some(data);
        self
    }
}
