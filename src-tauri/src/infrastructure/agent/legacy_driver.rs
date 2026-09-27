use async_trait::async_trait;

use crate::agent::event::RuntimeEvent;
use crate::agent::runtime::{RuntimeCancellation, RuntimeDriver, RuntimeError, RuntimeEventSink};
use crate::agent::tool::ToolContext;
use crate::agent::{run_agent_loop, AgentConfig, AgentEvent};
use crate::contracts::agent_run::{RuntimeArtifact, RuntimeCompletion, RuntimeRequest};
use crate::models::ChatMessage;

/// Compatibility driver that keeps the existing ReAct loop behind the new runtime boundary.
pub struct LegacyAgentRuntimeDriver;

#[async_trait]
impl RuntimeDriver for LegacyAgentRuntimeDriver {
    async fn execute(
        &self,
        request: RuntimeRequest,
        events: RuntimeEventSink,
        mut cancellation: RuntimeCancellation,
    ) -> Result<RuntimeCompletion, RuntimeError> {
        let history = request
            .history
            .into_iter()
            .map(|message| ChatMessage {
                role: message.role,
                content: message.content,
                tool_calls: None,
                tool_call_id: None,
            })
            .collect();
        let client =
            std::sync::Arc::new(crate::llm::LlmClient::for_user(&request.user_id, None).await);
        let context = ToolContext::new(
            request.session_id.clone(),
            request.user_id,
            None,
            None,
            Vec::new(),
            |_event, _data| {},
        );
        let mut legacy_events = run_agent_loop(
            history,
            request.user_message,
            Vec::new(),
            context,
            AgentConfig {
                max_turns: request.max_turns,
                system_prompt: String::new(),
                allowed_tools: None,
            },
            client,
        )
        .await;

        loop {
            let event = tokio::select! {
                _ = cancellation.cancelled() => return Err(RuntimeError::Cancelled),
                event = legacy_events.recv() => event,
            };
            match event {
                Some(AgentEvent::Thinking { content }) => {
                    events.emit(RuntimeEvent::Thinking { content }).await?;
                }
                Some(AgentEvent::ToolCall { tool, input }) => {
                    events
                        .emit(RuntimeEvent::ToolStarted { tool, input })
                        .await?;
                }
                Some(AgentEvent::ToolResult {
                    tool,
                    success,
                    result,
                    error,
                }) => {
                    events
                        .emit(RuntimeEvent::ToolFinished {
                            tool,
                            success,
                            result: serde_json::json!({"result": result, "error": error}),
                        })
                        .await?;
                }
                Some(AgentEvent::Artifact { artifact }) => {
                    events
                        .emit(RuntimeEvent::ArtifactProduced {
                            artifact: RuntimeArtifact {
                                kind: artifact.kind,
                                title: artifact.title,
                                content: artifact.content,
                            },
                        })
                        .await?;
                }
                Some(AgentEvent::Message { content }) => {
                    events
                        .emit(RuntimeEvent::MessageProduced { content })
                        .await?;
                }
                Some(AgentEvent::TurnEnd { turn }) => {
                    events.emit(RuntimeEvent::TurnFinished { turn }).await?;
                }
                Some(AgentEvent::Done { summary, .. }) => {
                    return Ok(RuntimeCompletion { summary });
                }
                Some(AgentEvent::Error { message }) => {
                    return Err(RuntimeError::Model(message));
                }
                None => {
                    return Err(RuntimeError::Internal(
                        "legacy agent event stream ended without a terminal event".into(),
                    ));
                }
            }
        }
    }
}
