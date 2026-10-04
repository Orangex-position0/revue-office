use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, watch};

use crate::providers::ChatProvider;

use super::r#loop::AgentLoop;
use super::{AgentError, AgentEvent, AgentRequest, ToolRegistry};

#[derive(Clone)]
pub struct CancellationHandle {
    sender: watch::Sender<bool>,
}

impl CancellationHandle {
    pub fn cancel(&self) {
        let _ = self.sender.send(true);
    }
}

pub struct AgentRunHandle {
    pub run_id: String,
    pub events: mpsc::Receiver<AgentEvent>,
    cancellation: CancellationHandle,
}

impl AgentRunHandle {
    pub fn cancel(&self) {
        self.cancellation.cancel();
    }

    pub fn cancellation_handle(&self) -> CancellationHandle {
        self.cancellation.clone()
    }
}

pub struct AgentCore {
    driver: Arc<AgentLoop>,
    event_capacity: usize,
    timeout: Duration,
}

impl AgentCore {
    pub fn new(
        provider: Arc<dyn ChatProvider>,
        tools: ToolRegistry,
        event_capacity: usize,
        timeout: Duration,
    ) -> Self {
        Self {
            driver: Arc::new(AgentLoop::new(provider, tools)),
            event_capacity: event_capacity.max(1),
            timeout: timeout.max(Duration::from_millis(1)),
        }
    }

    pub fn start(&self, request: AgentRequest) -> AgentRunHandle {
        let run_id = request.run_id.clone();
        let (sender, events) = mpsc::channel(self.event_capacity);
        let (cancel_sender, mut cancel_receiver) = watch::channel(false);
        let driver = Arc::clone(&self.driver);
        let timeout = self.timeout;
        let driver_sender = sender.clone();

        tokio::spawn(async move {
            let outcome = tokio::select! {
                biased;
                _ = wait_for_cancellation(&mut cancel_receiver) => Err(AgentError::Cancelled),
                _ = tokio::time::sleep(timeout) => Err(AgentError::Timeout),
                result = driver.execute(request, driver_sender) => result,
            };
            let terminal = match outcome {
                Ok(completion) => AgentEvent::Completed {
                    summary: completion.summary,
                    outputs: completion.outputs,
                },
                Err(error) => AgentEvent::Failed {
                    kind: error.failure_kind(),
                    message: error.to_string(),
                },
            };
            let _ = sender.send(terminal).await;
        });

        AgentRunHandle {
            run_id,
            events,
            cancellation: CancellationHandle {
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

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::future::pending;
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use serde_json::{Value, json};
    use tokio::sync::{Notify, mpsc};
    use tokio::time::{Duration, timeout};

    use crate::agent_core::{
        AgentEvent, AgentFailureKind, AgentRequest, AgentTool, GeneratedOutput, ToolContext,
        ToolError, ToolEventSink, ToolOutput, ToolRegistry,
    };
    use crate::providers::{
        ChatMessage, ChatProvider, ChatRequest, ChatResponse, ChatRole, ProviderError,
        ProviderEvent, StopReason, ToolCall, ToolDefinition,
    };

    use super::AgentCore;

    struct QueueProvider {
        responses: Mutex<VecDeque<Result<ChatResponse, ProviderError>>>,
    }

    impl QueueProvider {
        fn new(responses: Vec<Result<ChatResponse, ProviderError>>) -> Self {
            Self {
                responses: Mutex::new(responses.into()),
            }
        }
    }

    #[async_trait]
    impl ChatProvider for QueueProvider {
        async fn chat(&self, _request: ChatRequest) -> Result<ChatResponse, ProviderError> {
            self.responses
                .lock()
                .expect("fake provider response lock should not be poisoned")
                .pop_front()
                .expect("fake provider should have a queued response")
        }

        async fn stream_chat(
            &self,
            request: ChatRequest,
            _events: mpsc::Sender<ProviderEvent>,
        ) -> Result<ChatResponse, ProviderError> {
            self.chat(request).await
        }
    }

    struct BlockingProvider;

    #[async_trait]
    impl ChatProvider for BlockingProvider {
        async fn chat(&self, _request: ChatRequest) -> Result<ChatResponse, ProviderError> {
            pending().await
        }

        async fn stream_chat(
            &self,
            request: ChatRequest,
            _events: mpsc::Sender<ProviderEvent>,
        ) -> Result<ChatResponse, ProviderError> {
            self.chat(request).await
        }
    }

    struct FakeTool {
        name: &'static str,
        fail: bool,
        called: Option<Arc<Notify>>,
    }

    #[async_trait]
    impl AgentTool for FakeTool {
        fn definition(&self) -> ToolDefinition {
            ToolDefinition {
                name: self.name.to_owned(),
                description: "fake tool".to_owned(),
                parameters: json!({"type": "object"}),
            }
        }

        async fn call(
            &self,
            _input: Value,
            context: &ToolContext,
            events: ToolEventSink,
        ) -> Result<ToolOutput, ToolError> {
            if let Some(called) = &self.called {
                called.notify_one();
            }
            events
                .progress("working", json!({"percent": 50}))
                .await
                .map_err(|error| ToolError::Execution(error.to_string()))?;
            context.insert("fake", json!(true)).await;
            if self.fail {
                return Err(ToolError::Execution("intentional failure".to_owned()));
            }
            Ok(ToolOutput::new("tool completed")
                .with_data(json!({"answer": 42}))
                .with_outputs(vec![GeneratedOutput {
                    kind: "test".to_owned(),
                    title: "Fake output".to_owned(),
                    content: json!({"answer": 42}),
                    file: None,
                }]))
        }
    }

    fn response_with_tool(name: &str) -> ChatResponse {
        ChatResponse {
            message: ChatMessage {
                role: ChatRole::Assistant,
                content: Vec::new(),
                tool_calls: vec![ToolCall {
                    id: "call-1".to_owned(),
                    name: name.to_owned(),
                    arguments: "{}".to_owned(),
                }],
                tool_call_id: None,
            },
            model: "fake-model".to_owned(),
            stop_reason: StopReason::ToolCalls,
            usage: None,
        }
    }

    fn response_with_text(text: &str) -> ChatResponse {
        ChatResponse {
            message: ChatMessage::text(ChatRole::Assistant, text),
            model: "fake-model".to_owned(),
            stop_reason: StopReason::EndTurn,
            usage: None,
        }
    }

    fn request(max_turns: usize) -> AgentRequest {
        AgentRequest {
            run_id: "run-1".to_owned(),
            model: "fake-model".to_owned(),
            messages: vec![ChatMessage::text(ChatRole::User, "hello")],
            max_turns,
            allowed_tools: None,
            temperature: None,
            tool_context: ToolContext::new("run-1"),
        }
    }

    fn tool(name: &'static str, fail: bool) -> Arc<dyn AgentTool> {
        Arc::new(FakeTool {
            name,
            fail,
            called: None,
        })
    }

    async fn collect_events(mut receiver: mpsc::Receiver<AgentEvent>) -> Vec<AgentEvent> {
        let mut events = Vec::new();
        while let Some(event) = timeout(Duration::from_secs(1), receiver.recv())
            .await
            .expect("agent event should arrive before the test timeout")
        {
            events.push(event);
        }
        events
    }

    #[tokio::test]
    async fn agent_core_runs_provider_tool_loop_and_emits_one_completed_terminal() {
        let provider = Arc::new(QueueProvider::new(vec![
            Ok(response_with_tool("fake")),
            Ok(response_with_text("finished")),
        ]));
        let registry =
            ToolRegistry::new(vec![tool("fake", false)]).expect("fake registry should be valid");
        let core = AgentCore::new(provider, registry, 2, Duration::from_secs(1));

        let handle = core.start(request(2));
        assert_eq!(handle.run_id, "run-1");
        let events = collect_events(handle.events).await;

        assert!(matches!(events[0], AgentEvent::ToolStarted { .. }));
        assert!(matches!(events[1], AgentEvent::ToolProgress { .. }));
        assert!(matches!(events[2], AgentEvent::ToolFinished { .. }));
        assert!(matches!(events[3], AgentEvent::OutputProduced { .. }));
        assert!(matches!(events[4], AgentEvent::TurnFinished { turn: 1 }));
        assert!(matches!(events[5], AgentEvent::MessageProduced { .. }));
        assert!(matches!(events[6], AgentEvent::Completed { .. }));
        assert_eq!(events.iter().filter(|event| event.is_terminal()).count(), 1);
    }

    #[tokio::test]
    async fn agent_core_applies_backpressure_to_tool_progress() {
        let called = Arc::new(Notify::new());
        let registry = ToolRegistry::new(vec![Arc::new(FakeTool {
            name: "fake",
            fail: false,
            called: Some(Arc::clone(&called)),
        })])
        .expect("fake registry should be valid");
        let provider = Arc::new(QueueProvider::new(vec![
            Ok(response_with_tool("fake")),
            Ok(response_with_text("finished")),
        ]));
        let core = AgentCore::new(provider, registry, 1, Duration::from_secs(1));
        let mut handle = core.start(request(2));

        timeout(Duration::from_secs(1), called.notified())
            .await
            .expect("tool should start");
        assert_eq!(
            handle.events.len(),
            1,
            "bounded event channel should be full"
        );
        assert!(matches!(
            handle.events.recv().await,
            Some(AgentEvent::ToolStarted { .. })
        ));
        assert!(matches!(
            handle.events.recv().await,
            Some(AgentEvent::ToolProgress { .. })
        ));
        let remaining = collect_events(handle.events).await;
        assert!(matches!(
            remaining.last(),
            Some(AgentEvent::Completed { .. })
        ));
    }

    #[tokio::test]
    async fn agent_core_provider_failure_has_exactly_one_failed_terminal() {
        let provider = Arc::new(QueueProvider::new(vec![Err(ProviderError::Unavailable)]));
        let registry = ToolRegistry::new(Vec::new()).expect("empty registry should be valid");
        let core = AgentCore::new(provider, registry, 1, Duration::from_secs(1));

        let events = collect_events(core.start(request(1)).events).await;

        assert!(matches!(
            events.as_slice(),
            [AgentEvent::Failed {
                kind: AgentFailureKind::Provider,
                ..
            }]
        ));
    }

    #[tokio::test]
    async fn agent_core_tool_failure_has_exactly_one_failed_terminal() {
        let provider = Arc::new(QueueProvider::new(vec![Ok(response_with_tool("fake"))]));
        let registry =
            ToolRegistry::new(vec![tool("fake", true)]).expect("fake registry should be valid");
        let core = AgentCore::new(provider, registry, 8, Duration::from_secs(1));

        let events = collect_events(core.start(request(1)).events).await;

        assert!(matches!(
            events.last(),
            Some(AgentEvent::Failed {
                kind: AgentFailureKind::Tool,
                ..
            })
        ));
        assert_eq!(events.iter().filter(|event| event.is_terminal()).count(), 1);
    }

    #[tokio::test]
    async fn agent_core_maximum_turns_has_exactly_one_failed_terminal() {
        let provider = Arc::new(QueueProvider::new(vec![Ok(response_with_tool("fake"))]));
        let registry =
            ToolRegistry::new(vec![tool("fake", false)]).expect("fake registry should be valid");
        let core = AgentCore::new(provider, registry, 8, Duration::from_secs(1));

        let events = collect_events(core.start(request(1)).events).await;

        assert!(matches!(
            events.last(),
            Some(AgentEvent::Failed {
                kind: AgentFailureKind::MaximumTurns,
                ..
            })
        ));
        assert_eq!(events.iter().filter(|event| event.is_terminal()).count(), 1);
    }

    #[tokio::test]
    async fn agent_core_cancellation_has_exactly_one_failed_terminal() {
        let registry = ToolRegistry::new(Vec::new()).expect("empty registry should be valid");
        let core = AgentCore::new(
            Arc::new(BlockingProvider),
            registry,
            1,
            Duration::from_secs(5),
        );
        let handle = core.start(request(1));
        handle.cancel();

        let events = collect_events(handle.events).await;

        assert!(matches!(
            events.as_slice(),
            [AgentEvent::Failed {
                kind: AgentFailureKind::Cancelled,
                ..
            }]
        ));
    }

    #[tokio::test]
    async fn agent_core_timeout_has_exactly_one_failed_terminal() {
        let registry = ToolRegistry::new(Vec::new()).expect("empty registry should be valid");
        let core = AgentCore::new(
            Arc::new(BlockingProvider),
            registry,
            1,
            Duration::from_millis(10),
        );

        let events = collect_events(core.start(request(1)).events).await;

        assert!(matches!(
            events.as_slice(),
            [AgentEvent::Failed {
                kind: AgentFailureKind::Timeout,
                ..
            }]
        ));
    }

    #[test]
    fn agent_core_registry_is_immutable_and_rejects_duplicate_names() {
        let error =
            match ToolRegistry::new(vec![tool("duplicate", false), tool("duplicate", false)]) {
                Ok(_) => panic!("duplicate names must be rejected"),
                Err(error) => error,
            };
        assert_eq!(
            error,
            crate::agent_core::RegistryError::DuplicateName("duplicate".to_owned())
        );
    }
}
