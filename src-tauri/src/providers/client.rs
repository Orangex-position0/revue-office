use async_trait::async_trait;
use tokio::sync::mpsc;

use super::{ChatRequest, ChatResponse, ProviderError, ProviderEvent};

#[async_trait]
pub trait ChatProvider: Send + Sync {
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, ProviderError>;

    async fn stream_chat(
        &self,
        request: ChatRequest,
        events: mpsc::Sender<ProviderEvent>,
    ) -> Result<ChatResponse, ProviderError>;
}
