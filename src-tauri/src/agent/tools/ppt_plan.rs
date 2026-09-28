use std::sync::Arc;

use async_trait::async_trait;
use serde_json::json;

use crate::agent::tool::{OfficeTool, ToolContext, ToolResult};
use crate::capabilities::presentation::PresentationCapability;
use crate::contracts::presentation::PresentationPlanRequest;

pub struct PptPlanTool {
    capability: Arc<PresentationCapability>,
}

impl PptPlanTool {
    pub fn new(capability: Arc<PresentationCapability>) -> Self {
        Self { capability }
    }
}

#[async_trait]
impl OfficeTool for PptPlanTool {
    fn name(&self) -> &str {
        "ppt_plan"
    }

    fn description(&self) -> &str {
        "规划 PPT 大纲：根据用户需求生成页面规划。这是 PPT 生成的第一步。"
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "topic": { "type": "string", "description": "PPT 主题/用户需求" },
                "audience": { "type": "string", "description": "目标听众（可选）" }
            },
            "required": ["topic"]
        })
    }

    fn is_read_only(&self) -> bool {
        true
    }

    fn produces_artifact(&self) -> bool {
        false
    }

    async fn call(&self, input: serde_json::Value, ctx: &ToolContext) -> ToolResult {
        let topic = input
            .get("topic")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .trim();
        if topic.is_empty() {
            return ToolResult::err("topic 不能为空");
        }
        let Some(progress) = ctx.presentation_progress() else {
            return ToolResult::err("PPT 类型化进度通道不可用");
        };
        match self
            .capability
            .plan(
                PresentationPlanRequest {
                    owner_id: ctx.user_id.clone(),
                    topic: topic.into(),
                    audience: input
                        .get("audience")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_owned),
                    preferred_model: ctx.preferred_model.clone(),
                },
                progress.as_ref(),
            )
            .await
        {
            Ok(plan) => {
                let slide_count = plan.slides.len();
                let title = plan.title.clone();
                let value = match serde_json::to_value(plan) {
                    Ok(value) => value,
                    Err(error) => return ToolResult::err(format!("PPT 大纲序列化失败: {error}")),
                };
                ctx.scratchpad
                    .lock()
                    .await
                    .insert("ppt_plan".into(), value.clone());
                ToolResult::ok(
                    format!("已规划 PPT《{title}》，共 {slide_count} 页大纲"),
                    vec![],
                )
                .with_data(value)
            }
            Err(error) => ToolResult::err(format!("PPT 大纲生成失败: {error}")),
        }
    }
}
