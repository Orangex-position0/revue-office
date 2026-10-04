use std::sync::Arc;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};

use super::super::profile::OfficeToolMetadata;
use crate::agent_core::{
    AgentTool, GeneratedFile, GeneratedOutput, ToolContext, ToolError, ToolEventSink, ToolOutput,
};
use crate::capabilities::presentation::{
    PresentationCapability, PresentationGenerateRequest, PresentationPlan, PresentationPlanRequest,
    PresentationProgress, PresentationProgressError, PresentationProgressSink,
};
use crate::providers::ToolDefinition;

const PLAN_SCRATCHPAD_KEY: &str = "ppt_plan";

pub struct PresentationPlanTool {
    capability: Arc<PresentationCapability>,
}

impl PresentationPlanTool {
    pub fn new(capability: Arc<PresentationCapability>) -> Self {
        Self { capability }
    }
}

#[derive(Deserialize)]
struct PlanInput {
    topic: String,
    #[serde(default)]
    audience: Option<String>,
}

#[async_trait]
impl AgentTool for PresentationPlanTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "ppt_plan".into(),
            description: "规划 PPT 大纲：根据用户需求生成页面规划。这是 PPT 生成的第一步。".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "topic": { "type": "string", "description": "PPT 主题/用户需求" },
                    "audience": { "type": "string", "description": "目标听众（可选）" }
                },
                "required": ["topic"]
            }),
        }
    }

    async fn call(
        &self,
        input: Value,
        context: &ToolContext,
        events: ToolEventSink,
    ) -> Result<ToolOutput, ToolError> {
        let input: PlanInput = serde_json::from_value(input)
            .map_err(|error| ToolError::InvalidInput(error.to_string()))?;
        if input.topic.trim().is_empty() {
            return Err(ToolError::InvalidInput("topic 不能为空".into()));
        }
        let metadata = metadata(context)?;
        let progress = PresentationToolProgressSink { events };
        let plan = self
            .capability
            .plan(
                PresentationPlanRequest {
                    owner_id: metadata.user_id,
                    topic: input.topic.trim().to_owned(),
                    audience: input.audience,
                    preferred_model: metadata.preferred_model,
                },
                &progress,
            )
            .await
            .map_err(|error| ToolError::Execution(format!("PPT 大纲生成失败: {error}")))?;
        let value = serde_json::to_value(&plan)
            .map_err(|error| ToolError::Execution(format!("PPT 大纲序列化失败: {error}")))?;
        context.insert(PLAN_SCRATCHPAD_KEY, value.clone()).await;
        Ok(ToolOutput::new(format!(
            "已规划 PPT《{}》，共 {} 页大纲",
            plan.title,
            plan.slides.len()
        ))
        .with_data(value))
    }
}

pub struct PresentationGenerateTool {
    capability: Arc<PresentationCapability>,
}

impl PresentationGenerateTool {
    pub fn new(capability: Arc<PresentationCapability>) -> Self {
        Self { capability }
    }
}

#[derive(Deserialize)]
struct GenerateInput {
    title: String,
    #[serde(default)]
    topic: Option<String>,
    #[serde(default)]
    theme: Option<String>,
}

#[async_trait]
impl AgentTool for PresentationGenerateTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "ppt_generate".into(),
            description: "生成完整 PPT 项目：根据主题和大纲逐页生成幻灯片并持久化。".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "title": { "type": "string", "description": "PPT 标题" },
                    "topic": { "type": "string", "description": "用户原始需求" },
                    "theme": { "type": "string", "description": "default/business/tech/warm/minimal" }
                },
                "required": ["title"]
            }),
        }
    }

    async fn call(
        &self,
        input: Value,
        context: &ToolContext,
        events: ToolEventSink,
    ) -> Result<ToolOutput, ToolError> {
        let input: GenerateInput = serde_json::from_value(input)
            .map_err(|error| ToolError::InvalidInput(error.to_string()))?;
        let title = input.title.trim();
        if title.is_empty() {
            return Err(ToolError::InvalidInput("title 不能为空".into()));
        }
        let metadata = metadata(context)?;
        let plan = context
            .get(PLAN_SCRATCHPAD_KEY)
            .await
            .map(serde_json::from_value::<PresentationPlan>)
            .transpose()
            .map_err(|error| ToolError::InvalidInput(format!("PPT 大纲无效: {error}")))?;
        let progress = PresentationToolProgressSink { events };
        let output = self
            .capability
            .generate(
                PresentationGenerateRequest {
                    owner_id: metadata.user_id,
                    title: title.to_owned(),
                    topic: input
                        .topic
                        .filter(|topic| !topic.trim().is_empty())
                        .unwrap_or_else(|| title.to_owned()),
                    theme: input.theme.unwrap_or_else(|| "default".into()),
                    preferred_model: metadata.preferred_model,
                    plan,
                },
                &progress,
            )
            .await
            .map_err(|error| ToolError::Execution(format!("PPT 生成失败: {error}")))?;
        let slide_count = output.project.slides.len();
        let project_title = output.project.title.clone();
        let content = serde_json::to_value(output.project)
            .map_err(|error| ToolError::Execution(format!("PPT 项目序列化失败: {error}")))?;
        Ok(ToolOutput::new(format!(
            "已生成 PPT《{project_title}》，共 {slide_count} 页"
        ))
        .with_outputs(vec![GeneratedOutput {
            kind: "ppt".into(),
            title: project_title,
            content,
            file: Some(GeneratedFile {
                extension: output.format,
                bytes: output.bytes,
            }),
        }]))
    }
}

fn metadata(context: &ToolContext) -> Result<OfficeToolMetadata, ToolError> {
    serde_json::from_value(context.metadata.clone())
        .map_err(|error| ToolError::InvalidInput(format!("invalid Office tool context: {error}")))
}

struct PresentationToolProgressSink {
    events: ToolEventSink,
}

#[async_trait]
impl PresentationProgressSink for PresentationToolProgressSink {
    async fn emit(&self, progress: PresentationProgress) -> Result<(), PresentationProgressError> {
        let result = match progress {
            PresentationProgress::Planning => {
                self.events
                    .progress("presentation.planning", json!({}))
                    .await
            }
            PresentationProgress::ProjectCreated { project } => {
                self.events
                    .progress(
                        "presentation.project_created",
                        serde_json::to_value(project).unwrap_or(Value::Null),
                    )
                    .await
            }
            PresentationProgress::SlideGenerated {
                project,
                current_index,
                total_slides,
            } => {
                let mut detail = serde_json::to_value(project).unwrap_or(Value::Null);
                if let Some(object) = detail.as_object_mut() {
                    object.insert("current_index".into(), current_index.into());
                    object.insert("total_slides".into(), total_slides.into());
                    object.insert("slide_count".into(), (current_index + 1).into());
                }
                self.events
                    .progress("presentation.slide_generated", detail)
                    .await
            }
            PresentationProgress::GenerationCompleted { project } => {
                self.events
                    .progress("presentation.generated", json!({"project_id": project.id}))
                    .await
            }
        };
        result.map_err(|_| PresentationProgressError)
    }
}
