use async_trait::async_trait;

use crate::agent::event::RuntimeEvent;
use crate::agent::runtime::{RuntimeCancellation, RuntimeDriver, RuntimeError, RuntimeEventSink};
use crate::agent::tool::{LegacyToolProgressAdapter, ToolContext};
use crate::agent::{run_agent_loop, AgentConfig, AgentEvent};
use crate::contracts::agent_run::{RuntimeArtifact, RuntimeCompletion, RuntimeRequest};
use crate::models::ChatMessage;
use crate::ports::agent_llm::AgentLlmProvider;

/// Compatibility driver that keeps the existing ReAct loop behind the new runtime boundary.
pub struct LegacyAgentRuntimeDriver {
    llm_provider: std::sync::Arc<dyn AgentLlmProvider>,
}

impl LegacyAgentRuntimeDriver {
    pub fn new(llm_provider: std::sync::Arc<dyn AgentLlmProvider>) -> Self {
        Self { llm_provider }
    }
}

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
        let client = self
            .llm_provider
            .for_user(&request.user_id, request.preferred_model.as_deref())
            .await
            .map_err(|error| RuntimeError::Model(error.to_string()))?;
        let attachments = request
            .attachments
            .into_iter()
            .map(|attachment| crate::models::ChatAttachment {
                id: attachment.id,
                name: attachment.name,
                kind: attachment.kind,
                mime_type: attachment.mime_type,
                size: attachment.size,
                text_content: attachment.text_content,
                data_url: attachment.data_url,
            })
            .collect::<Vec<_>>();
        let (legacy_progress, mut progress_events) = LegacyToolProgressAdapter::bounded(256);
        let mut context = ToolContext::new(
            request.session_id.clone(),
            request.user_id,
            request.project_id,
            request.preferred_model,
            attachments.clone(),
            legacy_progress.callback(),
        );
        if let Some(tool_config) = request.tool_config {
            context = context.with_tool_config(tool_config);
        }
        let mut legacy_events = run_agent_loop(
            history,
            request.user_message,
            attachments,
            context,
            AgentConfig {
                max_turns: request.max_turns,
                system_prompt: String::new(),
                allowed_tools: request.allowed_tools,
            },
            client,
        )
        .await;

        loop {
            let event = tokio::select! {
                _ = cancellation.cancelled() => return Err(RuntimeError::Cancelled),
                progress = progress_events.recv() => {
                    if let Some(progress) = progress {
                        events.emit(RuntimeEvent::LegacyProgress { progress }).await?;
                    }
                    continue;
                }
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
                Some(AgentEvent::PresentationProgress { progress }) => {
                    events
                        .emit(RuntimeEvent::PresentationProgress { progress })
                        .await?;
                }
                Some(AgentEvent::Artifact { artifact }) => {
                    let bytes = if artifact.bytes.is_empty() {
                        match artifact.extension.as_str() {
                            "md" => artifact
                                .content
                                .get("markdown")
                                .and_then(serde_json::Value::as_str)
                                .map(str::as_bytes)
                                .map(ToOwned::to_owned)
                                .ok_or_else(|| {
                                    RuntimeError::Tool(
                                        "markdown artifact has no markdown content".into(),
                                    )
                                })?,
                            "drawio" => artifact
                                .content
                                .get("xml")
                                .and_then(serde_json::Value::as_str)
                                .map(str::as_bytes)
                                .map(ToOwned::to_owned)
                                .ok_or_else(|| {
                                    RuntimeError::Tool("drawio artifact has no XML content".into())
                                })?,
                            _ => serde_json::to_vec_pretty(&artifact.content)
                                .map_err(|error| RuntimeError::Tool(error.to_string()))?,
                        }
                    } else {
                        artifact.bytes
                    };
                    events
                        .emit(RuntimeEvent::ArtifactProduced {
                            artifact: RuntimeArtifact {
                                kind: artifact.kind,
                                title: artifact.title,
                                extension: artifact.extension,
                                content: artifact.content,
                                bytes,
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
                    while let Ok(progress) = progress_events.try_recv() {
                        events
                            .emit(RuntimeEvent::LegacyProgress { progress })
                            .await?;
                    }
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
