use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use revue_office_lib::agent::event::RuntimeEvent;
use revue_office_lib::agent::runtime::{
    AgentRuntime, RuntimeCancellation, RuntimeDriver, RuntimeError, RuntimeEventSink,
};
use revue_office_lib::application::chat_service::{ChatApplicationService, ChatCommand};
use revue_office_lib::application::event::ApplicationEvent;
use revue_office_lib::contracts::agent_run::{RuntimeCompletion, RuntimeRequest};
use revue_office_lib::contracts::conversation::{
    Conversation, ConversationArtifact, ConversationMessage, NewConversation,
};
use revue_office_lib::ports::repositories::session::{SessionRepository, SessionRepositoryError};

#[derive(Default)]
struct FakeRepository {
    messages: Mutex<HashMap<String, Vec<ConversationMessage>>>,
}

#[async_trait]
impl SessionRepository for FakeRepository {
    async fn create(
        &self,
        request: NewConversation,
    ) -> Result<Conversation, SessionRepositoryError> {
        Ok(Conversation {
            id: uuid::Uuid::new_v4().to_string(),
            owner_id: request.owner_id,
            project_id: request.project_id,
            tool_kind: request.tool_kind,
            title: request.title,
            summary: None,
            message_count: 0,
            order: 0,
            created_at: "2026-09-26T00:00:00Z".into(),
            updated_at: "2026-09-26T00:00:00Z".into(),
        })
    }

    async fn find_by_id(&self, _id: &str) -> Result<Option<Conversation>, SessionRepositoryError> {
        Ok(None)
    }

    async fn list_by_owner(
        &self,
        _owner_id: &str,
        _limit: u32,
        _query: Option<&str>,
    ) -> Result<Vec<Conversation>, SessionRepositoryError> {
        Ok(vec![])
    }

    async fn append_message(
        &self,
        session_id: &str,
        message: ConversationMessage,
    ) -> Result<(), SessionRepositoryError> {
        self.messages
            .lock()
            .unwrap()
            .entry(session_id.into())
            .or_default()
            .push(message);
        Ok(())
    }

    async fn history(
        &self,
        session_id: &str,
        _limit: u32,
    ) -> Result<Vec<ConversationMessage>, SessionRepositoryError> {
        Ok(self
            .messages
            .lock()
            .unwrap()
            .get(session_id)
            .cloned()
            .unwrap_or_default())
    }

    async fn legacy_artifacts(
        &self,
        _session_id: &str,
    ) -> Result<Vec<ConversationArtifact>, SessionRepositoryError> {
        Ok(vec![])
    }

    async fn replace_legacy_artifacts(
        &self,
        _session_id: &str,
        _artifacts: Vec<ConversationArtifact>,
    ) -> Result<(), SessionRepositoryError> {
        Ok(())
    }

    async fn update_title(
        &self,
        _session_id: &str,
        _owner_id: &str,
        _title: &str,
    ) -> Result<bool, SessionRepositoryError> {
        Ok(false)
    }

    async fn update_placement(
        &self,
        _session_id: &str,
        _owner_id: &str,
        _project_id: Option<&str>,
        _order: i64,
    ) -> Result<bool, SessionRepositoryError> {
        Ok(false)
    }

    async fn update_summary(
        &self,
        _session_id: &str,
        _summary: &str,
    ) -> Result<(), SessionRepositoryError> {
        Ok(())
    }

    async fn delete(
        &self,
        _session_id: &str,
        _owner_id: &str,
    ) -> Result<bool, SessionRepositoryError> {
        Ok(false)
    }

    async fn clear_messages(
        &self,
        _session_id: &str,
        _owner_id: &str,
    ) -> Result<bool, SessionRepositoryError> {
        Ok(false)
    }
}

#[derive(Clone, Copy)]
enum DriverOutcome {
    Complete,
    Fail,
    Block,
}

struct TerminalDriver {
    outcome: DriverOutcome,
    stopped: Arc<AtomicBool>,
}

struct StopGuard(Arc<AtomicBool>);

impl Drop for StopGuard {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

#[async_trait]
impl RuntimeDriver for TerminalDriver {
    async fn execute(
        &self,
        _request: RuntimeRequest,
        events: RuntimeEventSink,
        mut cancellation: RuntimeCancellation,
    ) -> Result<RuntimeCompletion, RuntimeError> {
        let _guard = StopGuard(self.stopped.clone());
        match self.outcome {
            DriverOutcome::Complete => {
                events
                    .emit(RuntimeEvent::MessageProduced {
                        content: "complete".into(),
                    })
                    .await?;
                Ok(RuntimeCompletion {
                    summary: "complete".into(),
                })
            }
            DriverOutcome::Fail => Err(RuntimeError::Tool("tool failed".into())),
            DriverOutcome::Block => {
                events
                    .emit(RuntimeEvent::Thinking {
                        content: "waiting".into(),
                    })
                    .await?;
                cancellation.cancelled().await;
                Err(RuntimeError::Cancelled)
            }
        }
    }
}

fn service(outcome: DriverOutcome, timeout: Duration) -> (ChatApplicationService, Arc<AtomicBool>) {
    let stopped = Arc::new(AtomicBool::new(false));
    let runtime = AgentRuntime::with_timeout(
        Arc::new(TerminalDriver {
            outcome,
            stopped: stopped.clone(),
        }),
        4,
        timeout,
    );
    (
        ChatApplicationService::new(Arc::new(FakeRepository::default()), Arc::new(runtime), 4),
        stopped,
    )
}

async fn start(
    service: &ChatApplicationService,
) -> revue_office_lib::application::chat_service::ChatRunHandle {
    service
        .start_chat(ChatCommand {
            owner_id: "owner-1".into(),
            session_id: None,
            project_id: None,
            message: "hello".into(),
            runtime_message: None,
            preferred_model: None,
            attachments: vec![],
            tool_config: None,
            allowed_tools: Some(vec![]),
            max_turns: 2,
        })
        .await
        .unwrap()
}

async fn collect(
    mut run: revue_office_lib::application::chat_service::ChatRunHandle,
) -> Vec<ApplicationEvent> {
    let mut events = Vec::new();
    while let Some(event) = run.events.recv().await {
        let terminal = event.is_terminal();
        events.push(event);
        if terminal {
            break;
        }
    }
    assert!(run.events.recv().await.is_none());
    events
}

#[tokio::test]
async fn chat_terminal_state_completion_and_tool_failure_each_emit_one_terminal() {
    let (complete_service, _) = service(DriverOutcome::Complete, Duration::from_secs(1));
    let completed = collect(start(&complete_service).await).await;
    assert!(matches!(
        completed.last(),
        Some(ApplicationEvent::Completed { .. })
    ));
    assert_eq!(
        completed.iter().filter(|event| event.is_terminal()).count(),
        1
    );

    let (failure_service, _) = service(DriverOutcome::Fail, Duration::from_secs(1));
    let failed = collect(start(&failure_service).await).await;
    assert!(matches!(
        failed.last(),
        Some(ApplicationEvent::Failed { code, .. }) if code == "runtime_tool"
    ));
    assert_eq!(failed.iter().filter(|event| event.is_terminal()).count(), 1);
}

#[tokio::test]
async fn chat_terminal_state_explicit_cancellation_emits_failed_without_completed() {
    let (service, stopped) = service(DriverOutcome::Block, Duration::from_secs(1));
    let mut run = start(&service).await;
    assert!(matches!(
        run.events.recv().await,
        Some(ApplicationEvent::StateChanged { .. })
    ));
    run.cancel();
    let terminal = run.events.recv().await.unwrap();
    assert!(matches!(
        terminal,
        ApplicationEvent::Failed { ref code, .. } if code == "runtime_cancelled"
    ));
    assert!(run.events.recv().await.is_none());
    assert!(stopped.load(Ordering::SeqCst));
}

#[tokio::test]
async fn chat_terminal_state_timeout_emits_failed_and_stops_work() {
    let (service, stopped) = service(DriverOutcome::Block, Duration::from_millis(20));
    let mut run = start(&service).await;
    assert!(matches!(
        run.events.recv().await,
        Some(ApplicationEvent::StateChanged { .. })
    ));
    let terminal = tokio::time::timeout(Duration::from_secs(1), run.events.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        terminal,
        ApplicationEvent::Failed { ref code, .. } if code == "runtime_timeout"
    ));
    assert!(run.events.recv().await.is_none());
    assert!(stopped.load(Ordering::SeqCst));
}

#[tokio::test]
async fn chat_terminal_state_consumer_disconnect_cancels_blocked_runtime() {
    let (service, stopped) = service(DriverOutcome::Block, Duration::from_secs(30));
    let mut run = start(&service).await;
    assert!(matches!(
        run.events.recv().await,
        Some(ApplicationEvent::StateChanged { .. })
    ));
    drop(run.events);
    tokio::time::timeout(Duration::from_secs(1), async {
        while !stopped.load(Ordering::SeqCst) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("disconnect should stop the runtime");
}
