use std::sync::Arc;

use async_trait::async_trait;

use crate::capabilities::presentation::{
    PresentationPlan, PresentationPlanRequest, PresentationPlanner, PresentationPlannerError,
};
use crate::providers::{ChatMessage, ChatProviderResolver, ChatRequest, ChatRole};

pub struct ConfiguredPresentationPlanner {
    resolver: Arc<dyn ChatProviderResolver>,
}

impl ConfiguredPresentationPlanner {
    pub fn new(resolver: Arc<dyn ChatProviderResolver>) -> Self {
        Self { resolver }
    }
}

#[async_trait]
impl PresentationPlanner for ConfiguredPresentationPlanner {
    async fn plan(
        &self,
        request: PresentationPlanRequest,
    ) -> Result<PresentationPlan, PresentationPlannerError> {
        let audience = request
            .audience
            .filter(|value| !value.trim().is_empty())
            .map(|value| format!("目标听众：{value}"))
            .unwrap_or_default();
        let prompt = format!(
            r#"你是资深演示文稿策划。请规划一份可直接用于正式汇报的 PPT。
主题：{}
{}
只返回严格 JSON：{{"title":"标题","slides":[{{"title":"页标题","layout":"title|content|section","goal":"本页目标","visual":"视觉建议","points":["要点"]}}]}}
要求：5-10 页；含封面和结尾；叙事完整；每页 2-4 个简洁要点。"#,
            request.topic, audience
        );
        let resolved = self
            .resolver
            .resolve(&request.owner_id, request.preferred_model.as_deref())
            .await
            .map_err(|error| PresentationPlannerError::Unavailable(anyhow::Error::new(error)))?;
        let response = resolved
            .provider
            .chat(ChatRequest {
                model: resolved.model,
                messages: vec![
                    ChatMessage::text(ChatRole::System, "你只输出严格 JSON。"),
                    ChatMessage::text(ChatRole::User, prompt),
                ],
                tools: Vec::new(),
                temperature: Some(0.7),
            })
            .await
            .map_err(|error| PresentationPlannerError::Unavailable(anyhow::Error::new(error)))?;
        let value = extract_json(&response.message.text_content())?;
        serde_json::from_value(value)
            .map_err(|error| PresentationPlannerError::InvalidResponse(error.to_string()))
    }
}

fn extract_json(text: &str) -> Result<serde_json::Value, PresentationPlannerError> {
    let cleaned = text
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    if let Ok(value) = serde_json::from_str(cleaned) {
        return Ok(value);
    }
    if let (Some(start), Some(end)) = (cleaned.find('{'), cleaned.rfind('}'))
        && end > start
        && let Ok(value) = serde_json::from_str(&cleaned[start..=end])
    {
        return Ok(value);
    }
    Err(PresentationPlannerError::InvalidResponse(
        "model did not return a JSON object".into(),
    ))
}
