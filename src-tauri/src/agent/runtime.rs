use std::sync::Arc;

use async_trait::async_trait;
use thiserror::Error;
use tokio::sync::{mpsc, watch};

use super::event::{RuntimeEvent, RuntimeFailureKind};
use super::tool::ToolProgressSink;
use crate::contracts::agent_run::{RuntimeCompletion, RuntimeRequest};

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("runtime was cancelled")]
    Cancelled,
    #[error("model failed: {0}")]
    Model(String),
    #[error("tool failed: {0}")]
    Tool(String),
    #[error("runtime event stream closed")]
    EventStreamClosed,
    #[error("runtime drivers cannot emit terminal events")]
    TerminalEventOwnedByRuntime,
    #[error("runtime failed: {0}")]
    Internal(String),
}

impl RuntimeError {
    fn failure_kind(&self) -> RuntimeFailureKind {
        match self {
            Self::Cancelled => RuntimeFailureKind::Cancelled,
            Self::Model(_) => RuntimeFailureKind::Model,
            Self::Tool(_) => RuntimeFailureKind::Tool,
            Self::EventStreamClosed | Self::TerminalEventOwnedByRuntime | Self::Internal(_) => {
                RuntimeFailureKind::Internal
            }
        }
    }
}

#[derive(Clone)]
pub struct RuntimeCancellation {
    receiver: watch::Receiver<bool>,
}

impl RuntimeCancellation {
    pub fn is_cancelled(&self) -> bool {
        *self.receiver.borrow()
    }

    pub async fn cancelled(&mut self) {
        if !self.is_cancelled() {
            let _ = self.receiver.changed().await;
        }
    }
}

#[derive(Clone)]
pub struct RuntimeEventSink {
    sender: mpsc::Sender<RuntimeEvent>,
}

impl RuntimeEventSink {
    pub async fn emit(&self, event: RuntimeEvent) -> Result<(), RuntimeError> {
        if event.is_terminal() {
            return Err(RuntimeError::TerminalEventOwnedByRuntime);
        }
        self.sender
            .send(event)
            .await
            .map_err(|_| RuntimeError::EventStreamClosed)
    }

    pub fn tool_progress(&self, tool: impl Into<String>) -> ToolProgressSink {
        ToolProgressSink::new(tool.into(), self.clone())
    }
}

#[async_trait]
pub trait RuntimeDriver: Send + Sync {
    async fn execute(
        &self,
        request: RuntimeRequest,
        events: RuntimeEventSink,
        cancellation: RuntimeCancellation,
    ) -> Result<RuntimeCompletion, RuntimeError>;
}

#[derive(Clone)]
pub struct RuntimeCancellationHandle {
    sender: watch::Sender<bool>,
}

impl RuntimeCancellationHandle {
    pub fn cancel(&self) {
        let _ = self.sender.send(true);
    }
}

pub struct AgentRunHandle {
    pub run_id: String,
    pub events: mpsc::Receiver<RuntimeEvent>,
    pub cancellation: RuntimeCancellationHandle,
}

impl AgentRunHandle {
    pub fn cancel(&self) {
        self.cancellation.cancel();
    }
}

pub struct AgentRuntime {
    driver: Arc<dyn RuntimeDriver>,
    event_capacity: usize,
}

impl AgentRuntime {
    pub fn new(driver: Arc<dyn RuntimeDriver>, event_capacity: usize) -> Self {
        Self {
            driver,
            event_capacity: event_capacity.max(1),
        }
    }

    pub fn start(&self, request: RuntimeRequest) -> AgentRunHandle {
        let run_id = request.run_id.clone();
        let (sender, receiver) = mpsc::channel(self.event_capacity);
        let (cancel_sender, cancel_receiver) = watch::channel(false);
        let driver = self.driver.clone();
        let driver_events = RuntimeEventSink {
            sender: sender.clone(),
        };
        let mut runtime_cancellation = RuntimeCancellation {
            receiver: cancel_receiver.clone(),
        };
        let driver_cancellation = RuntimeCancellation {
            receiver: cancel_receiver,
        };

        tokio::spawn(async move {
            let outcome = tokio::select! {
                biased;
                _ = runtime_cancellation.cancelled() => Err(RuntimeError::Cancelled),
                outcome = driver.execute(request, driver_events, driver_cancellation) => outcome,
            };
            let terminal = match outcome {
                Ok(completion) => RuntimeEvent::Completed {
                    summary: completion.summary,
                },
                Err(error) => RuntimeEvent::Failed {
                    kind: error.failure_kind(),
                    message: error.to_string(),
                },
            };
            let _ = sender.send(terminal).await;
        });

        AgentRunHandle {
            run_id,
            events: receiver,
            cancellation: RuntimeCancellationHandle {
                sender: cancel_sender,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::event::RuntimeEvent;
    use crate::contracts::agent_run::RuntimeMessage;
    use tokio::time::{timeout, Duration};

    fn request() -> RuntimeRequest {
        RuntimeRequest {
            run_id: "run-1".into(),
            session_id: "session-1".into(),
            user_id: "user-1".into(),
            history: vec![RuntimeMessage {
                role: "user".into(),
                content: "earlier".into(),
            }],
            user_message: "hello".into(),
            max_turns: 2,
        }
    }

    struct SuccessfulFakeDriver;

    #[async_trait]
    impl RuntimeDriver for SuccessfulFakeDriver {
        async fn execute(
            &self,
            _request: RuntimeRequest,
            events: RuntimeEventSink,
            _cancellation: RuntimeCancellation,
        ) -> Result<RuntimeCompletion, RuntimeError> {
            events
                .emit(RuntimeEvent::Thinking {
                    content: "thinking".into(),
                })
                .await?;
            let progress = events.tool_progress("fake_tool");
            progress
                .started(serde_json::json!({"topic": "demo"}))
                .await?;
            progress
                .progress("working", serde_json::json!({"percent": 50}))
                .await?;
            progress
                .finished(true, serde_json::json!({"answer": 42}))
                .await?;
            events
                .emit(RuntimeEvent::MessageProduced {
                    content: "done".into(),
                })
                .await?;
            Ok(RuntimeCompletion {
                summary: "complete".into(),
            })
        }
    }

    #[tokio::test]
    async fn agent_runtime_events_are_ordered_and_backpressured() {
        let runtime = AgentRuntime::new(Arc::new(SuccessfulFakeDriver), 1);
        let mut handle = runtime.start(request());
        assert_eq!(handle.run_id, "run-1");

        let mut events = Vec::new();
        while let Some(event) = timeout(Duration::from_secs(1), handle.events.recv())
            .await
            .expect("event should arrive")
        {
            let terminal = event.is_terminal();
            events.push(event);
            if terminal {
                break;
            }
        }

        assert!(matches!(events[0], RuntimeEvent::Thinking { .. }));
        assert!(matches!(events[1], RuntimeEvent::ToolStarted { .. }));
        assert!(matches!(events[2], RuntimeEvent::ToolProgress { .. }));
        assert!(matches!(events[3], RuntimeEvent::ToolFinished { .. }));
        assert!(matches!(events[4], RuntimeEvent::MessageProduced { .. }));
        assert!(matches!(events[5], RuntimeEvent::Completed { .. }));
        assert_eq!(events.iter().filter(|event| event.is_terminal()).count(), 1);
    }

    struct BlockingFakeDriver;

    #[async_trait]
    impl RuntimeDriver for BlockingFakeDriver {
        async fn execute(
            &self,
            _request: RuntimeRequest,
            events: RuntimeEventSink,
            mut cancellation: RuntimeCancellation,
        ) -> Result<RuntimeCompletion, RuntimeError> {
            events
                .emit(RuntimeEvent::Thinking {
                    content: "waiting".into(),
                })
                .await?;
            cancellation.cancelled().await;
            Err(RuntimeError::Cancelled)
        }
    }

    #[tokio::test]
    async fn agent_runtime_events_cancel_with_exactly_one_terminal_event() {
        let runtime = AgentRuntime::new(Arc::new(BlockingFakeDriver), 2);
        let mut handle = runtime.start(request());
        assert!(matches!(
            handle.events.recv().await,
            Some(RuntimeEvent::Thinking { .. })
        ));
        handle.cancel();

        let terminal = timeout(Duration::from_secs(1), handle.events.recv())
            .await
            .expect("cancellation should finish")
            .expect("failed event should be emitted");
        assert!(matches!(
            terminal,
            RuntimeEvent::Failed {
                kind: RuntimeFailureKind::Cancelled,
                ..
            }
        ));
        assert!(handle.events.recv().await.is_none());
    }

    struct FailingFakeDriver;

    #[async_trait]
    impl RuntimeDriver for FailingFakeDriver {
        async fn execute(
            &self,
            _request: RuntimeRequest,
            _events: RuntimeEventSink,
            _cancellation: RuntimeCancellation,
        ) -> Result<RuntimeCompletion, RuntimeError> {
            Err(RuntimeError::Tool("fake failure".into()))
        }
    }

    #[tokio::test]
    async fn agent_runtime_events_fail_with_exactly_one_terminal_event() {
        let runtime = AgentRuntime::new(Arc::new(FailingFakeDriver), 1);
        let mut handle = runtime.start(request());
        let event = handle
            .events
            .recv()
            .await
            .expect("failed event should arrive");
        assert!(matches!(
            event,
            RuntimeEvent::Failed {
                kind: RuntimeFailureKind::Tool,
                ..
            }
        ));
        assert!(handle.events.recv().await.is_none());
    }
}
