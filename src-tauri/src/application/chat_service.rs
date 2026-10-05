use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use futures::Stream;
use tokio::sync::mpsc;

use super::artifacts::{ArtifactDraft, ArtifactService};
use super::error::ChatApplicationError;
use super::event::ApplicationEvent;
use crate::agent::{
    AgentRunner, OfficeAgentEvent, OfficeAgentRequest, OfficeAttachment, OfficeCancellationHandle,
    OfficeFailureKind, OfficeMessage,
};
use crate::application::conversations::SessionRepository;
use crate::application::conversations::model::{ConversationMessage, NewConversation};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatCommand {
    pub actor: super::identity::Actor,
    pub session_id: Option<String>,
    pub project_id: Option<String>,
    pub message: String,
    pub runtime_message: Option<String>,
    pub preferred_model: Option<String>,
    pub attachments: Vec<OfficeAttachment>,
    pub tool_config: Option<serde_json::Value>,
    pub allowed_tools: Option<Vec<String>>,
    pub max_turns: usize,
}

pub struct ChatEventStream {
    receiver: mpsc::Receiver<ApplicationEvent>,
    cancellation: OfficeCancellationHandle,
}

impl ChatEventStream {
    pub async fn recv(&mut self) -> Option<ApplicationEvent> {
        self.receiver.recv().await
    }
}

impl Stream for ChatEventStream {
    type Item = ApplicationEvent;

    fn poll_next(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.receiver.poll_recv(context)
    }
}

impl Drop for ChatEventStream {
    fn drop(&mut self) {
        self.cancellation.cancel();
    }
}

pub struct ChatRunHandle {
    pub session_id: String,
    pub events: ChatEventStream,
    cancellation: OfficeCancellationHandle,
}

impl ChatRunHandle {
    pub fn cancel(&self) {
        self.cancellation.cancel();
    }
}

pub struct ChatApplicationService {
    repository: Arc<dyn SessionRepository>,
    agent: Arc<dyn AgentRunner>,
    artifact_service: Option<Arc<ArtifactService>>,
    event_capacity: usize,
}

impl ChatApplicationService {
    pub fn new(
        repository: Arc<dyn SessionRepository>,
        agent: Arc<dyn AgentRunner>,
        event_capacity: usize,
    ) -> Self {
        Self {
            repository,
            agent,
            artifact_service: None,
            event_capacity: event_capacity.max(1),
        }
    }

    pub fn with_artifact_service(
        repository: Arc<dyn SessionRepository>,
        agent: Arc<dyn AgentRunner>,
        artifact_service: Arc<ArtifactService>,
        event_capacity: usize,
    ) -> Self {
        Self {
            repository,
            agent,
            artifact_service: Some(artifact_service),
            event_capacity: event_capacity.max(1),
        }
    }

    pub async fn start_chat(
        &self,
        command: ChatCommand,
    ) -> Result<ChatRunHandle, ChatApplicationError> {
        let message = command.message.trim().to_owned();
        if message.is_empty() {
            return Err(ChatApplicationError::EmptyMessage);
        }

        let conversation = if let Some(session_id) = command.session_id.as_deref() {
            let conversation = self
                .repository
                .find_by_id(session_id)
                .await?
                .ok_or(ChatApplicationError::NotFound)?;
            if !command.actor.owns(&conversation.owner_id) {
                return Err(ChatApplicationError::Forbidden);
            }
            conversation
        } else {
            self.repository
                .create(NewConversation {
                    owner_id: command.actor.id.0.clone(),
                    project_id: command.project_id.clone(),
                    tool_kind: Some("general".into()),
                    title: conversation_title(&message),
                })
                .await?
        };

        let history = self.repository.history(&conversation.id, 100).await?;
        self.repository
            .append_message(
                &conversation.id,
                ConversationMessage {
                    role: "user".into(),
                    content: message.clone(),
                    tool_calls: None,
                    tool_call_id: None,
                    created_at: chrono::Utc::now().to_rfc3339(),
                },
            )
            .await?;

        let runtime_handle = self.agent.start(OfficeAgentRequest {
            run_id: uuid::Uuid::new_v4().to_string(),
            session_id: conversation.id.clone(),
            user_id: command.actor.id.0,
            project_id: command.project_id,
            preferred_model: command.preferred_model,
            attachments: command.attachments,
            tool_config: command.tool_config,
            allowed_tools: command.allowed_tools,
            history: history
                .into_iter()
                .map(|message| OfficeMessage {
                    role: message.role,
                    content: message.content,
                })
                .collect(),
            user_message: command.runtime_message.unwrap_or(message),
            max_turns: command.max_turns.max(1),
        });
        let cancellation = runtime_handle.cancellation.clone();
        let mut runtime_events = runtime_handle.events;
        let repository = self.repository.clone();
        let artifact_service = self.artifact_service.clone();
        let owner_id = conversation.owner_id.clone();
        let session_id = conversation.id.clone();
        let (application_sender, application_events) = mpsc::channel(self.event_capacity);
        let task_cancellation = cancellation.clone();

        tokio::spawn(async move {
            let mut published_artifacts = Vec::new();
            let mut terminal_sent = false;
            while let Some(event) = runtime_events.recv().await {
                let application_event = match event {
                    OfficeAgentEvent::Thinking { content } => ApplicationEvent::StateChanged {
                        state: "thinking".into(),
                        detail: serde_json::json!({"content": content}),
                    },
                    OfficeAgentEvent::ToolStarted { tool, input } => {
                        ApplicationEvent::StateChanged {
                            state: "tool_started".into(),
                            detail: serde_json::json!({"tool": tool, "input": input}),
                        }
                    }
                    OfficeAgentEvent::ToolProgress {
                        tool,
                        stage,
                        detail,
                    } => match map_tool_progress(tool, stage, detail) {
                        Some(event) => event,
                        None => continue,
                    },
                    OfficeAgentEvent::ToolFinished { tool, result } => {
                        ApplicationEvent::ToolResult {
                            tool,
                            success: true,
                            result,
                        }
                    }
                    OfficeAgentEvent::OutputProduced { output } => {
                        match artifact_service.as_ref() {
                            Some(artifact_service) => match artifact_service
                                .publish(ArtifactDraft {
                                    session_id: session_id.clone(),
                                    owner_id: owner_id.clone(),
                                    kind: output.kind,
                                    title: output.title,
                                    extension: output.extension,
                                    content: output.content,
                                    bytes: output.bytes,
                                })
                                .await
                            {
                                Ok(publication) => {
                                    debug_assert!(publication.is_ready());
                                    published_artifacts.push(publication.clone());
                                    ApplicationEvent::ArtifactUpdated {
                                        artifact: Box::new(publication),
                                        artifacts: published_artifacts.clone(),
                                    }
                                }
                                Err(error) => {
                                    task_cancellation.cancel();
                                    ApplicationEvent::Failed {
                                        code: "artifact_publication_failed".into(),
                                        message: error.to_string(),
                                    }
                                }
                            },
                            None => {
                                task_cancellation.cancel();
                                ApplicationEvent::Failed {
                                    code: "artifact_service_unavailable".into(),
                                    message: "Artifact publication is unavailable".into(),
                                }
                            }
                        }
                    }
                    OfficeAgentEvent::MessageProduced { content } => {
                        match repository
                            .append_message(
                                &session_id,
                                ConversationMessage {
                                    role: "assistant".into(),
                                    content: content.clone(),
                                    tool_calls: None,
                                    tool_call_id: None,
                                    created_at: chrono::Utc::now().to_rfc3339(),
                                },
                            )
                            .await
                        {
                            Ok(()) => ApplicationEvent::Message { content },
                            Err(_) => {
                                task_cancellation.cancel();
                                ApplicationEvent::Failed {
                                    code: "message_persistence_failed".into(),
                                    message: "Unable to persist the assistant message".into(),
                                }
                            }
                        }
                    }
                    OfficeAgentEvent::TurnFinished { turn } => ApplicationEvent::StateChanged {
                        state: "turn_finished".into(),
                        detail: serde_json::json!({"turn": turn}),
                    },
                    OfficeAgentEvent::Completed { summary } => {
                        if repository
                            .update_summary(&session_id, &summary)
                            .await
                            .is_err()
                        {
                            ApplicationEvent::Failed {
                                code: "summary_persistence_failed".into(),
                                message: "Unable to persist the conversation summary".into(),
                            }
                        } else {
                            ApplicationEvent::Completed {
                                summary,
                                artifacts: published_artifacts.clone(),
                                new_artifacts: published_artifacts.clone(),
                            }
                        }
                    }
                    OfficeAgentEvent::Failed { kind, message } => ApplicationEvent::Failed {
                        code: failure_code(kind).into(),
                        message,
                    },
                };
                let terminal = application_event.is_terminal();
                if application_sender.send(application_event).await.is_err() {
                    task_cancellation.cancel();
                    return;
                }
                if terminal {
                    terminal_sent = true;
                    break;
                }
            }
            if !terminal_sent {
                task_cancellation.cancel();
                let _ = application_sender
                    .send(ApplicationEvent::Failed {
                        code: "runtime_internal".into(),
                        message: "Agent event stream ended without a terminal event".into(),
                    })
                    .await;
            }
        });

        Ok(ChatRunHandle {
            session_id: conversation.id,
            events: ChatEventStream {
                receiver: application_events,
                cancellation: cancellation.clone(),
            },
            cancellation,
        })
    }
}

fn map_tool_progress(
    tool: String,
    stage: String,
    detail: serde_json::Value,
) -> Option<ApplicationEvent> {
    if stage == "legacy" {
        return Some(ApplicationEvent::LegacyToolProgress {
            event: detail
                .get("event")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("state_update")
                .to_owned(),
            data: detail
                .get("data")
                .cloned()
                .unwrap_or(serde_json::Value::Null),
        });
    }
    match stage.as_str() {
        "presentation.planning" => {
            return Some(ApplicationEvent::StateChanged {
                state: "规划 PPT 大纲".into(),
                detail: serde_json::json!("正在规划演示文稿结构..."),
            });
        }
        "presentation.project_created" => {
            return Some(ApplicationEvent::ProjectUpdated { project: detail });
        }
        "presentation.slide_generated" => {
            return Some(ApplicationEvent::SlideUpdated { slide: detail });
        }
        "presentation.generated" => return None,
        _ => {}
    }
    Some(ApplicationEvent::StateChanged {
        state: "tool_progress".into(),
        detail: serde_json::json!({"tool": tool, "stage": stage, "detail": detail}),
    })
}

fn failure_code(kind: OfficeFailureKind) -> &'static str {
    match kind {
        OfficeFailureKind::Cancelled => "runtime_cancelled",
        OfficeFailureKind::Timeout => "runtime_timeout",
        OfficeFailureKind::Provider => "runtime_model",
        OfficeFailureKind::Tool => "runtime_tool",
        OfficeFailureKind::MaximumTurns => "runtime_maximumturns",
        OfficeFailureKind::Internal => "runtime_internal",
    }
}

fn conversation_title(message: &str) -> String {
    const MAX_CHARS: usize = 60;
    let mut title: String = message.chars().take(MAX_CHARS).collect();
    if message.chars().count() > MAX_CHARS {
        title.push('…');
    }
    title
}
