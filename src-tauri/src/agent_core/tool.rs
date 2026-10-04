use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::mpsc;

use crate::providers::ToolDefinition;

use super::{AgentError, AgentEvent, ToolContext, ToolError, ToolOutput};

#[derive(Clone)]
pub struct ToolEventSink {
    tool: String,
    sender: mpsc::Sender<AgentEvent>,
}

impl ToolEventSink {
    pub(crate) fn new(tool: String, sender: mpsc::Sender<AgentEvent>) -> Self {
        Self { tool, sender }
    }

    pub async fn progress(
        &self,
        stage: impl Into<String>,
        detail: Value,
    ) -> Result<(), AgentError> {
        self.sender
            .send(AgentEvent::ToolProgress {
                tool: self.tool.clone(),
                stage: stage.into(),
                detail,
            })
            .await
            .map_err(|_| AgentError::EventReceiverClosed)
    }
}

#[async_trait]
pub trait AgentTool: Send + Sync {
    fn definition(&self) -> ToolDefinition;

    async fn call(
        &self,
        input: Value,
        context: &ToolContext,
        events: ToolEventSink,
    ) -> Result<ToolOutput, ToolError>;
}
