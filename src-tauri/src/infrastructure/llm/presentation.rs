use async_trait::async_trait;

use crate::contracts::presentation::{PresentationPlan, PresentationPlanRequest};
use crate::llm::LlmClient;
use crate::models::ChatMessage;
use crate::ports::llm::{PresentationLlm, PresentationLlmError};

pub struct ConfiguredPresentationLlm {
    user_id: String,
}

impl ConfiguredPresentationLlm {
    pub fn new(user_id: impl Into<String>) -> Self {
        Self {
            user_id: user_id.into(),
        }
    }
}

#[async_trait]
impl PresentationLlm for ConfiguredPresentationLlm {
    async fn plan(
        &self,
        request: PresentationPlanRequest,
    ) -> Result<PresentationPlan, PresentationLlmError> {
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
        let messages = vec![
            ChatMessage {
                role: "system".into(),
                content: "你只输出严格 JSON。".into(),
                tool_calls: None,
                tool_call_id: None,
            },
            ChatMessage {
                role: "user".into(),
                content: prompt,
                tool_calls: None,
                tool_call_id: None,
            },
        ];
        let client = LlmClient::for_user(&self.user_id, request.preferred_model.as_deref()).await;
        let response = client
            .chat(&messages, None)
            .await
            .map_err(PresentationLlmError::Unavailable)?;
        let content = response
            .choices
            .first()
            .and_then(|choice| choice.message.content.as_deref())
            .unwrap_or("");
        let value = LlmClient::extract_json(content)
            .map_err(|error| PresentationLlmError::InvalidResponse(error.to_string()))?;
        serde_json::from_value(value)
            .map_err(|error| PresentationLlmError::InvalidResponse(error.to_string()))
    }
}
