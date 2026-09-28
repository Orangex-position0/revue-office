use std::sync::Arc;

use async_trait::async_trait;
use serde_json::json;

use crate::agent::tool::{OfficeTool, ToolArtifact, ToolContext, ToolResult};
use crate::capabilities::presentation::PresentationCapability;
use crate::contracts::presentation::{PresentationGenerateRequest, PresentationPlan};
use crate::ports::presentation_export::PresentationExporter;

pub struct PptGenerateTool {
    capability: Arc<PresentationCapability>,
    exporter: Arc<dyn PresentationExporter>,
}

impl PptGenerateTool {
    pub fn new(
        capability: Arc<PresentationCapability>,
        exporter: Arc<dyn PresentationExporter>,
    ) -> Self {
        Self {
            capability,
            exporter,
        }
    }
}

#[async_trait]
impl OfficeTool for PptGenerateTool {
    fn name(&self) -> &str {
        "ppt_generate"
    }

    fn description(&self) -> &str {
        "生成完整 PPT 项目：根据主题和大纲逐页生成幻灯片并持久化。"
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "title": { "type": "string", "description": "PPT 标题" },
                "topic": { "type": "string", "description": "用户原始需求" },
                "theme": { "type": "string", "description": "default/business/tech/warm/minimal" }
            },
            "required": ["title"]
        })
    }

    async fn call(&self, input: serde_json::Value, ctx: &ToolContext) -> ToolResult {
        let title = input
            .get("title")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("未命名演示文稿")
            .to_owned();
        let topic = input
            .get("topic")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(&title)
            .to_owned();
        let theme = input
            .get("theme")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("default")
            .to_owned();
        let plan = ctx
            .scratchpad
            .lock()
            .await
            .get("ppt_plan")
            .cloned()
            .and_then(|value| serde_json::from_value::<PresentationPlan>(value).ok());
        let Some(progress) = ctx.presentation_progress() else {
            return ToolResult::err("PPT 类型化进度通道不可用");
        };
        match self
            .capability
            .generate(
                PresentationGenerateRequest {
                    owner_id: ctx.user_id.clone(),
                    title,
                    topic,
                    theme,
                    preferred_model: ctx.preferred_model.clone(),
                    plan,
                },
                progress.as_ref(),
            )
            .await
        {
            Ok(project) => {
                let slide_count = project.slides.len();
                let project_title = project.title.clone();
                let bytes = match self.exporter.export_pptx(&project).await {
                    Ok(bytes) => bytes,
                    Err(error) => {
                        return ToolResult::err(format!("PPTX 导出失败: {error}"));
                    }
                };
                let content = match serde_json::to_value(project) {
                    Ok(value) => value,
                    Err(error) => {
                        return ToolResult::err(format!("PPT 项目序列化失败: {error}"));
                    }
                };
                ToolResult::ok(
                    format!("已生成 PPT《{project_title}》，共 {slide_count} 页"),
                    vec![ToolArtifact {
                        kind: "ppt".into(),
                        title: project_title,
                        content,
                        extension: "pptx".into(),
                        bytes,
                    }],
                )
            }
            Err(error) => ToolResult::err(format!("PPT 生成失败: {error}")),
        }
    }
}
