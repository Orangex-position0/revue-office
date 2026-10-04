use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, watch};

use crate::agent_core::{AgentCore, AgentEvent, AgentFailureKind, GeneratedOutput, ToolRegistry};
use crate::providers::ChatProviderResolver;

use super::profile::{OfficeAgentRequest, build_agent_request};

#[derive(Debug, Clone, PartialEq)]
pub struct OfficeGeneratedOutput {
    pub kind: String,
    pub title: String,
    pub extension: String,
    pub content: serde_json::Value,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfficeFailureKind {
    Cancelled,
    Timeout,
    Provider,
    Tool,
    MaximumTurns,
    Internal,
}

#[derive(Debug, Clone, PartialEq)]
pub enum OfficeAgentEvent {
    Thinking {
        content: String,
    },
    ToolStarted {
        tool: String,
        input: serde_json::Value,
    },
    ToolProgress {
        tool: String,
        stage: String,
        detail: serde_json::Value,
    },
    ToolFinished {
        tool: String,
        result: serde_json::Value,
    },
    OutputProduced {
        output: OfficeGeneratedOutput,
    },
    MessageProduced {
        content: String,
    },
    TurnFinished {
        turn: usize,
    },
    Completed {
        summary: String,
    },
    Failed {
        kind: OfficeFailureKind,
        message: String,
    },
}

impl OfficeAgentEvent {
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed { .. } | Self::Failed { .. })
    }
}

#[derive(Clone)]
pub struct OfficeCancellationHandle {
    sender: watch::Sender<bool>,
}

impl OfficeCancellationHandle {
    pub fn channel() -> (Self, watch::Receiver<bool>) {
        let (sender, receiver) = watch::channel(false);
        (Self { sender }, receiver)
    }

    pub fn cancel(&self) {
        let _ = self.sender.send(true);
    }
}

pub struct OfficeAgentRunHandle {
    pub run_id: String,
    pub events: mpsc::Receiver<OfficeAgentEvent>,
    pub cancellation: OfficeCancellationHandle,
}

impl OfficeAgentRunHandle {
    pub fn new(
        run_id: impl Into<String>,
        events: mpsc::Receiver<OfficeAgentEvent>,
        cancellation: OfficeCancellationHandle,
    ) -> Self {
        Self {
            run_id: run_id.into(),
            events,
            cancellation,
        }
    }

    pub fn cancel(&self) {
        self.cancellation.cancel();
    }
}

pub trait AgentRunner: Send + Sync {
    fn start(&self, request: OfficeAgentRequest) -> OfficeAgentRunHandle;
}

pub struct OfficeAgent {
    providers: Arc<dyn ChatProviderResolver>,
    tools: ToolRegistry,
    event_capacity: usize,
    timeout: Duration,
}

impl OfficeAgent {
    pub fn new(
        providers: Arc<dyn ChatProviderResolver>,
        tools: ToolRegistry,
        event_capacity: usize,
        timeout: Duration,
    ) -> Self {
        Self {
            providers,
            tools,
            event_capacity: event_capacity.max(1),
            timeout: timeout.max(Duration::from_millis(1)),
        }
    }
}

impl AgentRunner for OfficeAgent {
    fn start(&self, request: OfficeAgentRequest) -> OfficeAgentRunHandle {
        let run_id = request.run_id.clone();
        let user_id = request.user_id.clone();
        let preferred_model = request.preferred_model.clone();
        let providers = Arc::clone(&self.providers);
        let tools = self.tools.clone();
        let event_capacity = self.event_capacity;
        let timeout = self.timeout;
        let (sender, events) = mpsc::channel(event_capacity);
        let (cancel_sender, mut cancel_receiver) = watch::channel(false);

        tokio::spawn(async move {
            let resolved = tokio::select! {
                biased;
                _ = wait_for_cancellation(&mut cancel_receiver) => {
                    send_terminal(&sender, OfficeAgentEvent::Failed {
                        kind: OfficeFailureKind::Cancelled,
                        message: "agent run was cancelled".into(),
                    }).await;
                    return;
                }
                resolved = providers.resolve(&user_id, preferred_model.as_deref()) => resolved,
            };
            let resolved = match resolved {
                Ok(resolved) => resolved,
                Err(error) => {
                    send_terminal(
                        &sender,
                        OfficeAgentEvent::Failed {
                            kind: OfficeFailureKind::Provider,
                            message: error.to_string(),
                        },
                    )
                    .await;
                    return;
                }
            };

            let core = AgentCore::new(resolved.provider, tools, event_capacity, timeout);
            let mut run = core.start(build_agent_request(request, resolved.model));
            let core_cancellation = run.cancellation_handle();
            let cancellation_task = tokio::spawn(async move {
                wait_for_cancellation(&mut cancel_receiver).await;
                core_cancellation.cancel();
            });

            while let Some(event) = run.events.recv().await {
                let terminal = event.is_terminal();
                for event in map_event(event) {
                    if sender.send(event).await.is_err() {
                        run.cancel();
                        cancellation_task.abort();
                        return;
                    }
                }
                if terminal {
                    break;
                }
            }
            cancellation_task.abort();
        });

        OfficeAgentRunHandle {
            run_id,
            events,
            cancellation: OfficeCancellationHandle {
                sender: cancel_sender,
            },
        }
    }
}

async fn wait_for_cancellation(receiver: &mut watch::Receiver<bool>) {
    if !*receiver.borrow() {
        let _ = receiver.changed().await;
    }
}

async fn send_terminal(sender: &mpsc::Sender<OfficeAgentEvent>, event: OfficeAgentEvent) {
    debug_assert!(event.is_terminal());
    let _ = sender.send(event).await;
}

fn map_event(event: AgentEvent) -> Vec<OfficeAgentEvent> {
    match event {
        AgentEvent::Thinking { content } => vec![OfficeAgentEvent::Thinking { content }],
        AgentEvent::ToolStarted { tool, input } => {
            vec![OfficeAgentEvent::ToolStarted { tool, input }]
        }
        AgentEvent::ToolProgress {
            tool,
            stage,
            detail,
        } => {
            vec![OfficeAgentEvent::ToolProgress {
                tool,
                stage,
                detail,
            }]
        }
        AgentEvent::ToolFinished { tool, result } => {
            vec![OfficeAgentEvent::ToolFinished { tool, result }]
        }
        AgentEvent::OutputProduced { output } => vec![OfficeAgentEvent::OutputProduced {
            output: map_generated_output(output),
        }],
        AgentEvent::MessageProduced { content } => {
            vec![OfficeAgentEvent::MessageProduced { content }]
        }
        AgentEvent::TurnFinished { turn } => vec![OfficeAgentEvent::TurnFinished { turn }],
        AgentEvent::Completed { summary, .. } => vec![OfficeAgentEvent::Completed { summary }],
        AgentEvent::Failed { kind, message } => vec![OfficeAgentEvent::Failed {
            kind: map_failure(kind),
            message,
        }],
    }
}

fn map_generated_output(output: GeneratedOutput) -> OfficeGeneratedOutput {
    let (extension, bytes) = output
        .file
        .map(|file| (file.extension, file.bytes))
        .unwrap_or_else(|| ("json".into(), Vec::new()));
    OfficeGeneratedOutput {
        kind: output.kind,
        title: output.title,
        extension,
        content: output.content,
        bytes,
    }
}

fn map_failure(kind: AgentFailureKind) -> OfficeFailureKind {
    match kind {
        AgentFailureKind::Cancelled => OfficeFailureKind::Cancelled,
        AgentFailureKind::Timeout => OfficeFailureKind::Timeout,
        AgentFailureKind::Provider => OfficeFailureKind::Provider,
        AgentFailureKind::Tool => OfficeFailureKind::Tool,
        AgentFailureKind::MaximumTurns => OfficeFailureKind::MaximumTurns,
        AgentFailureKind::Internal => OfficeFailureKind::Internal,
    }
}
