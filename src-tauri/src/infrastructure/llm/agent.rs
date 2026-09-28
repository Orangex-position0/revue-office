use std::sync::Arc;

use async_trait::async_trait;

use crate::llm::{ChatCompletionResponse, FunctionDef, LlmClient};
use crate::models::{ChatAttachment, ChatMessage};
use crate::ports::agent_llm::{AgentLlm, AgentLlmError, AgentLlmProvider};

pub struct ConfiguredAgentLlmProvider;

struct ConfiguredAgentLlm {
    client: LlmClient,
}

#[async_trait]
impl AgentLlmProvider for ConfiguredAgentLlmProvider {
    async fn for_user(
        &self,
        user_id: &str,
        preferred_model: Option<&str>,
    ) -> Result<Arc<dyn AgentLlm>, AgentLlmError> {
        Ok(Arc::new(ConfiguredAgentLlm {
            client: LlmClient::for_user(user_id, preferred_model).await,
        }))
    }
}

#[async_trait]
impl AgentLlm for ConfiguredAgentLlm {
    async fn chat(
        &self,
        messages: &[ChatMessage],
        tools: Option<&[FunctionDef]>,
    ) -> Result<ChatCompletionResponse, AgentLlmError> {
        self.client
            .chat(messages, tools)
            .await
            .map_err(AgentLlmError::Unavailable)
    }

    async fn chat_with_attachments(
        &self,
        messages: &[ChatMessage],
        tools: Option<&[FunctionDef]>,
        attachments: Option<&[ChatAttachment]>,
    ) -> Result<ChatCompletionResponse, AgentLlmError> {
        self.client
            .chat_with_attachments(messages, tools, attachments)
            .await
            .map_err(AgentLlmError::Unavailable)
    }
}
