use std::sync::Arc;

use serde_json::{Value, json};
use tokio::sync::mpsc;

use crate::providers::{
    ChatMessage, ChatProvider, ChatRequest, ChatRole, ContentPart, ProviderError,
};

use super::{AgentCompletion, AgentError, AgentEvent, AgentRequest, ToolEventSink, ToolRegistry};

pub(crate) struct AgentLoop {
    provider: Arc<dyn ChatProvider>,
    tools: ToolRegistry,
}

impl AgentLoop {
    pub(crate) fn new(provider: Arc<dyn ChatProvider>, tools: ToolRegistry) -> Self {
        Self { provider, tools }
    }

    pub(crate) async fn execute(
        &self,
        request: AgentRequest,
        events: mpsc::Sender<AgentEvent>,
    ) -> Result<AgentCompletion, AgentError> {
        let max_turns = request.max_turns.max(1);
        let definitions = self.tools.definitions(request.allowed_tools.as_deref());
        let mut messages = request.messages;
        let mut outputs = Vec::new();

        for turn in 1..=max_turns {
            let response = self
                .provider
                .chat(ChatRequest {
                    model: request.model.clone(),
                    messages: messages.clone(),
                    tools: definitions.clone(),
                    temperature: request.temperature,
                })
                .await
                .map_err(map_provider_error)?;
            let assistant = response.message;
            let content = assistant.text_content();

            if assistant.tool_calls.is_empty() {
                let summary = if content.trim().is_empty() {
                    "Agent completed without a text response.".to_owned()
                } else {
                    content
                };
                emit(
                    &events,
                    AgentEvent::MessageProduced {
                        content: summary.clone(),
                    },
                )
                .await?;
                return Ok(AgentCompletion { summary, outputs });
            }

            if !content.is_empty() {
                emit(&events, AgentEvent::Thinking { content }).await?;
            }
            let tool_calls = assistant.tool_calls.clone();
            messages.push(assistant);

            for call in tool_calls {
                let input = serde_json::from_str::<Value>(&call.arguments).map_err(|error| {
                    AgentError::Tool {
                        tool: call.name.clone(),
                        message: format!("invalid JSON arguments: {error}"),
                    }
                })?;
                emit(
                    &events,
                    AgentEvent::ToolStarted {
                        tool: call.name.clone(),
                        input: input.clone(),
                    },
                )
                .await?;

                let tool = self.tools.get(&call.name).ok_or_else(|| AgentError::Tool {
                    tool: call.name.clone(),
                    message: "tool is not registered for this agent".to_owned(),
                })?;
                let result = tool
                    .call(
                        input,
                        &request.tool_context,
                        ToolEventSink::new(call.name.clone(), events.clone()),
                    )
                    .await
                    .map_err(|error| map_tool_error(&call.name, error))?;

                emit(
                    &events,
                    AgentEvent::ToolFinished {
                        tool: call.name.clone(),
                        result: result.data.clone(),
                    },
                )
                .await?;
                for output in result.outputs {
                    outputs.push(output.clone());
                    emit(&events, AgentEvent::OutputProduced { output }).await?;
                }
                messages.push(ChatMessage {
                    role: ChatRole::Tool,
                    content: vec![ContentPart::Text(
                        json!({
                            "observation": result.observation,
                            "data": result.data,
                        })
                        .to_string(),
                    )],
                    tool_calls: Vec::new(),
                    tool_call_id: Some(call.id),
                });
            }
            emit(&events, AgentEvent::TurnFinished { turn }).await?;
        }

        Err(AgentError::MaximumTurns(max_turns))
    }
}

async fn emit(events: &mpsc::Sender<AgentEvent>, event: AgentEvent) -> Result<(), AgentError> {
    if event.is_terminal() {
        return Err(AgentError::TerminalEventOwnedByRuntime);
    }
    events
        .send(event)
        .await
        .map_err(|_| AgentError::EventReceiverClosed)
}

fn map_provider_error(error: ProviderError) -> AgentError {
    match error {
        ProviderError::Cancelled => AgentError::Cancelled,
        ProviderError::Timeout => AgentError::Timeout,
        error => AgentError::Provider(error.to_string()),
    }
}

fn map_tool_error(tool: &str, error: super::ToolError) -> AgentError {
    match error {
        super::ToolError::Cancelled => AgentError::Cancelled,
        error => AgentError::Tool {
            tool: tool.to_owned(),
            message: error.to_string(),
        },
    }
}
