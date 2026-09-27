use async_trait::async_trait;

use crate::agent::event::RuntimeEvent;
use crate::agent::runtime::{RuntimeCancellation, RuntimeDriver, RuntimeError, RuntimeEventSink};
use crate::agent::tool::{LegacyToolProgressAdapter, ToolContext};
use crate::agent::{run_agent_loop, AgentConfig, AgentEvent};
use crate::application::artifact_service::ArtifactService;
use crate::contracts::agent_run::{RuntimeArtifact, RuntimeCompletion, RuntimeRequest};
use crate::contracts::artifact::ArtifactDraft;
use crate::models::ChatMessage;

/// Compatibility driver that keeps the existing ReAct loop behind the new runtime boundary.
pub struct LegacyAgentRuntimeDriver {
    artifact_service: std::sync::Arc<ArtifactService>,
}

impl LegacyAgentRuntimeDriver {
    pub fn new(artifact_service: std::sync::Arc<ArtifactService>) -> Self {
        Self { artifact_service }
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
        let session_id = request.session_id.clone();
        let owner_id = request.user_id.clone();
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
        let client = std::sync::Arc::new(
            crate::llm::LlmClient::for_user(&request.user_id, request.preferred_model.as_deref())
                .await,
        );
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
                Some(AgentEvent::Artifact { artifact }) => {
                    let bytes = serde_json::to_vec_pretty(&artifact.content)
                        .map_err(|error| RuntimeError::Tool(error.to_string()))?;
                    let publication = self
                        .artifact_service
                        .publish(ArtifactDraft {
                            session_id: session_id.clone(),
                            owner_id: owner_id.clone(),
                            kind: artifact.kind.clone(),
                            title: artifact.title.clone(),
                            extension: "json".into(),
                            content: artifact.content,
                            bytes,
                        })
                        .await
                        .map_err(|error| RuntimeError::Tool(error.to_string()))?;
                    events
                        .emit(RuntimeEvent::ArtifactProduced {
                            artifact: RuntimeArtifact {
                                kind: publication.kind,
                                title: publication.title,
                                content: publication.content,
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
