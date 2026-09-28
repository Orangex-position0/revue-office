use std::sync::Arc;

use async_trait::async_trait;
use thiserror::Error;

use crate::llm::{ChatCompletionResponse, FunctionDef};
use crate::models::{ChatAttachment, ChatMessage};

#[derive(Debug, Error)]
pub enum AgentLlmError {
    #[error("agent model is unavailable")]
    Unavailable(#[source] anyhow::Error),
}

#[async_trait]
pub trait AgentLlm: Send + Sync {
    async fn chat(
        &self,
        messages: &[ChatMessage],
        tools: Option<&[FunctionDef]>,
    ) -> Result<ChatCompletionResponse, AgentLlmError>;

    async fn chat_with_attachments(
        &self,
        messages: &[ChatMessage],
        tools: Option<&[FunctionDef]>,
        attachments: Option<&[ChatAttachment]>,
    ) -> Result<ChatCompletionResponse, AgentLlmError>;
}

#[async_trait]
pub trait AgentLlmProvider: Send + Sync {
    async fn for_user(
        &self,
        user_id: &str,
        preferred_model: Option<&str>,
    ) -> Result<Arc<dyn AgentLlm>, AgentLlmError>;
}
