use std::sync::Arc;

use tokio::sync::mpsc;

use super::error::ChatApplicationError;
use super::event::ApplicationEvent;
use crate::agent::event::RuntimeEvent;
use crate::agent::runtime::{AgentRuntime, RuntimeCancellationHandle};
use crate::contracts::agent_run::{RuntimeAttachment, RuntimeMessage, RuntimeRequest};
use crate::contracts::conversation::{ConversationMessage, NewConversation};
use crate::ports::repositories::session::SessionRepository;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatCommand {
    pub owner_id: String,
    pub session_id: Option<String>,
    pub project_id: Option<String>,
    pub message: String,
    pub runtime_message: Option<String>,
    pub preferred_model: Option<String>,
    pub attachments: Vec<RuntimeAttachment>,
    pub tool_config: Option<serde_json::Value>,
    pub allowed_tools: Option<Vec<String>>,
    pub max_turns: usize,
}

pub struct ChatRunHandle {
    pub session_id: String,
    pub events: mpsc::Receiver<ApplicationEvent>,
    cancellation: RuntimeCancellationHandle,
}

impl ChatRunHandle {
    pub fn cancel(&self) {
        self.cancellation.cancel();
    }
}

pub struct ChatApplicationService {
    repository: Arc<dyn SessionRepository>,
    runtime: Arc<AgentRuntime>,
    event_capacity: usize,
}

impl ChatApplicationService {
    pub fn new(
        repository: Arc<dyn SessionRepository>,
        runtime: Arc<AgentRuntime>,
        event_capacity: usize,
    ) -> Self {
        Self {
            repository,
            runtime,
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
            if conversation.owner_id != command.owner_id {
                return Err(ChatApplicationError::Forbidden);
            }
            conversation
        } else {
            self.repository
                .create(NewConversation {
                    owner_id: command.owner_id.clone(),
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

        let runtime_handle = self.runtime.start(RuntimeRequest {
            run_id: uuid::Uuid::new_v4().to_string(),
            session_id: conversation.id.clone(),
            user_id: command.owner_id,
            project_id: command.project_id,
            preferred_model: command.preferred_model,
            attachments: command.attachments,
            tool_config: command.tool_config,
            allowed_tools: command.allowed_tools,
            history: history
                .into_iter()
                .map(|message| RuntimeMessage {
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
        let session_id = conversation.id.clone();
        let (application_sender, application_events) = mpsc::channel(self.event_capacity);
        let task_cancellation = cancellation.clone();

        tokio::spawn(async move {
            while let Some(event) = runtime_events.recv().await {
                let application_event = match event {
                    RuntimeEvent::Thinking { content } => ApplicationEvent::StateChanged {
                        state: "thinking".into(),
                        detail: serde_json::json!({"content": content}),
                    },
                    RuntimeEvent::ToolStarted { tool, input } => ApplicationEvent::StateChanged {
                        state: "tool_started".into(),
                        detail: serde_json::json!({"tool": tool, "input": input}),
                    },
                    RuntimeEvent::ToolProgress {
                        tool,
                        stage,
                        detail,
                    } => ApplicationEvent::StateChanged {
                        state: "tool_progress".into(),
                        detail: serde_json::json!({
                            "tool": tool,
                            "stage": stage,
                            "detail": detail,
                        }),
                    },
                    RuntimeEvent::ToolFinished {
                        tool,
                        success,
                        result,
                    } => ApplicationEvent::ToolResult {
                        tool,
                        success,
                        result,
                    },
                    RuntimeEvent::ArtifactProduced { artifact } => ApplicationEvent::StateChanged {
                        state: "artifact_pending".into(),
                        detail: serde_json::json!({"artifact": artifact}),
                    },
                    RuntimeEvent::MessageProduced { content } => {
                        if repository
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
                            .is_err()
                        {
                            let _ = application_sender
                                .send(ApplicationEvent::Failed {
                                    code: "message_persistence_failed".into(),
                                    message: "Unable to persist the assistant message".into(),
                                })
                                .await;
                            task_cancellation.cancel();
                            break;
                        }
                        ApplicationEvent::Message { content }
                    }
                    RuntimeEvent::TurnFinished { turn } => ApplicationEvent::StateChanged {
                        state: "turn_finished".into(),
                        detail: serde_json::json!({"turn": turn}),
                    },
                    RuntimeEvent::LegacyProgress { progress } => {
                        ApplicationEvent::LegacyToolProgress {
                            event: progress.event,
                            data: progress.data,
                        }
                    }
                    RuntimeEvent::Completed { summary } => {
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
                            ApplicationEvent::Completed { summary }
                        }
                    }
                    RuntimeEvent::Failed { kind, message } => ApplicationEvent::Failed {
                        code: format!("runtime_{kind:?}").to_lowercase(),
                        message,
                    },
                };
                let terminal = application_event.is_terminal();
                if application_sender.send(application_event).await.is_err() {
                    task_cancellation.cancel();
                    break;
                }
                if terminal {
                    break;
                }
            }
        });

        Ok(ChatRunHandle {
            session_id: conversation.id,
            events: application_events,
            cancellation,
        })
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
