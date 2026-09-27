use std::sync::Arc;

use async_trait::async_trait;
use serde_json::json;

use crate::agent::tool::{OfficeTool, PresentationToolProgressAdapter, ToolContext, ToolResult};
use crate::capabilities::presentation::PresentationCapability;
use crate::contracts::presentation::PresentationPlanRequest;
use crate::infrastructure::llm::presentation::ConfiguredPresentationLlm;
use crate::infrastructure::presentation_store::local::LocalPresentationStore;

pub struct PptPlanTool;

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
        let capability = PresentationCapability::new(
            Arc::new(ConfiguredPresentationLlm::new(&ctx.user_id)),
            Arc::new(LocalPresentationStore),
        );
        let progress = PresentationToolProgressAdapter::new(ctx.emit.clone());
        match capability
            .plan(
                PresentationPlanRequest {
                    topic: topic.into(),
                    audience: input
                        .get("audience")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_owned),
                    preferred_model: ctx.preferred_model.clone(),
                },
                &progress,
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
