use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use revue_office_lib::agent::{
    AgentRunner, OfficeAgentEvent, OfficeAgentRequest, OfficeAgentRunHandle,
    OfficeCancellationHandle, OfficeFailureKind, OfficeGeneratedOutput,
};
use revue_office_lib::application::artifacts::{
    ArtifactFinalization, ArtifactPublication, ArtifactPublicationRepository,
    ArtifactPublicationRepositoryError, ArtifactPublicationStatus, ArtifactService,
    NewArtifactPublication,
};
use revue_office_lib::application::chat_service::{ChatApplicationService, ChatCommand};
use revue_office_lib::application::conversations::model::{
    Conversation, ConversationArtifact, ConversationMessage, NewConversation,
};
use revue_office_lib::application::conversations::{SessionRepository, SessionRepositoryError};
use revue_office_lib::application::event::ApplicationEvent;
use revue_office_lib::infrastructure::filesystem::artifact_storage::LocalArtifactStorage;

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
    OutputThenComplete,
    Fail,
    Block,
}

struct TerminalAgent {
    outcome: DriverOutcome,
    stopped: Arc<AtomicBool>,
    timeout: Duration,
}

struct StopGuard(Arc<AtomicBool>);

impl Drop for StopGuard {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

impl AgentRunner for TerminalAgent {
    fn start(&self, request: OfficeAgentRequest) -> OfficeAgentRunHandle {
        let (sender, receiver) = tokio::sync::mpsc::channel(4);
        let (cancellation, mut cancelled) = OfficeCancellationHandle::channel();
        let outcome = self.outcome;
        let stopped = self.stopped.clone();
        let timeout = self.timeout;
        tokio::spawn(async move {
            let _guard = StopGuard(stopped);
            match outcome {
                DriverOutcome::Complete => {
                    let _ = sender
                        .send(OfficeAgentEvent::MessageProduced {
                            content: "complete".into(),
                        })
                        .await;
                    let _ = sender
                        .send(OfficeAgentEvent::Completed {
                            summary: "complete".into(),
                        })
                        .await;
                }
                DriverOutcome::OutputThenComplete => {
                    for event in [
                        OfficeAgentEvent::ToolProgress {
                            tool: "ppt_generate".into(),
                            stage: "presentation.planning".into(),
                            detail: serde_json::json!({}),
                        },
                        OfficeAgentEvent::ToolProgress {
                            tool: "ppt_generate".into(),
                            stage: "presentation.project_created".into(),
                            detail: serde_json::json!({"id": "project-1"}),
                        },
                        OfficeAgentEvent::ToolProgress {
                            tool: "ppt_generate".into(),
                            stage: "presentation.slide_generated".into(),
                            detail: serde_json::json!({"id": "project-1", "slide_count": 1}),
                        },
                        OfficeAgentEvent::OutputProduced {
                            output: OfficeGeneratedOutput {
                                kind: "ppt".into(),
                                title: "Layered presentation".into(),
                                extension: "pptx".into(),
                                content: serde_json::json!({"slides": [{}]}),
                                bytes: b"valid pptx candidate".to_vec(),
                            },
                        },
                        OfficeAgentEvent::Completed {
                            summary: "complete".into(),
                        },
                    ] {
                        if sender.send(event).await.is_err() {
                            return;
                        }
                    }
                }
                DriverOutcome::Fail => {
                    let _ = sender
                        .send(OfficeAgentEvent::Failed {
                            kind: OfficeFailureKind::Tool,
                            message: "tool failed".into(),
                        })
                        .await;
                }
                DriverOutcome::Block => {
                    if sender
                        .send(OfficeAgentEvent::Thinking {
                            content: "waiting".into(),
                        })
                        .await
                        .is_err()
                    {
                        return;
                    }
                    let (kind, message) = tokio::select! {
                        _ = cancelled.changed() => (
                            OfficeFailureKind::Cancelled,
                            "agent run was cancelled".to_owned(),
                        ),
                        _ = tokio::time::sleep(timeout) => (
                            OfficeFailureKind::Timeout,
                            "agent run timed out".to_owned(),
                        ),
                    };
                    let _ = sender
                        .send(OfficeAgentEvent::Failed { kind, message })
                        .await;
                }
            }
        });
        OfficeAgentRunHandle::new(request.run_id, receiver, cancellation)
    }
}

fn service(outcome: DriverOutcome, timeout: Duration) -> (ChatApplicationService, Arc<AtomicBool>) {
    let stopped = Arc::new(AtomicBool::new(false));
    let agent = Arc::new(TerminalAgent {
        outcome,
        stopped: stopped.clone(),
        timeout,
    });
    (
        ChatApplicationService::new(Arc::new(FakeRepository::default()), agent, 4),
        stopped,
    )
}

#[derive(Default)]
struct MemoryArtifactRepository {
    publications: Mutex<HashMap<String, ArtifactPublication>>,
}

#[async_trait]
impl ArtifactPublicationRepository for MemoryArtifactRepository {
    async fn reserve(
        &self,
        publication: NewArtifactPublication,
    ) -> Result<ArtifactPublication, ArtifactPublicationRepositoryError> {
        let reserved = ArtifactPublication {
            id: publication.id,
            session_id: publication.session_id,
            owner_id: publication.owner_id,
            kind: publication.kind,
            title: publication.title,
            status: ArtifactPublicationStatus::Publishing,
            content: publication.content,
            staging_path: Some(publication.staging_path),
            final_path: None,
            error: None,
            version: 1,
            created_at: "2026-09-26T00:00:00Z".into(),
            updated_at: "2026-09-26T00:00:00Z".into(),
        };
        self.publications
            .lock()
            .unwrap()
            .insert(reserved.id.clone(), reserved.clone());
        Ok(reserved)
    }

    async fn find(
        &self,
        id: &str,
    ) -> Result<Option<ArtifactPublication>, ArtifactPublicationRepositoryError> {
        Ok(self.publications.lock().unwrap().get(id).cloned())
    }

    async fn finalize(
        &self,
        id: &str,
        finalization: ArtifactFinalization,
    ) -> Result<Option<ArtifactPublication>, ArtifactPublicationRepositoryError> {
        let mut publications = self.publications.lock().unwrap();
        let Some(publication) = publications.get_mut(id) else {
            return Ok(None);
        };
        publication.status = ArtifactPublicationStatus::Ready;
        publication.content = finalization.content;
        publication.staging_path = None;
        publication.final_path = Some(finalization.final_path);
        publication.version += 1;
        Ok(Some(publication.clone()))
    }

    async fn fail(
        &self,
        id: &str,
        error: &str,
    ) -> Result<bool, ArtifactPublicationRepositoryError> {
        let mut publications = self.publications.lock().unwrap();
        let Some(publication) = publications.get_mut(id) else {
            return Ok(false);
        };
        publication.status = ArtifactPublicationStatus::Failed;
        publication.staging_path = None;
        publication.error = Some(error.into());
        Ok(true)
    }

    async fn pending(
        &self,
    ) -> Result<Vec<ArtifactPublication>, ArtifactPublicationRepositoryError> {
        Ok(self
            .publications
            .lock()
            .unwrap()
            .values()
            .filter(|publication| publication.status == ArtifactPublicationStatus::Publishing)
            .cloned()
            .collect())
    }
}

async fn start(
    service: &ChatApplicationService,
) -> revue_office_lib::application::chat_service::ChatRunHandle {
    service
        .start_chat(ChatCommand {
            actor: revue_office_lib::application::identity::Actor::user("owner-1"),
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
async fn chat_terminal_state_publishes_ready_output_after_layered_progress_before_completion() {
    let root = std::env::temp_dir().join(format!("revue-layered-events-{}", uuid::Uuid::new_v4()));
    let stopped = Arc::new(AtomicBool::new(false));
    let agent = Arc::new(TerminalAgent {
        outcome: DriverOutcome::OutputThenComplete,
        stopped,
        timeout: Duration::from_secs(1),
    });
    let artifact_service = Arc::new(ArtifactService::new(
        Arc::new(MemoryArtifactRepository::default()),
        Arc::new(LocalArtifactStorage::new(&root)),
    ));
    let service = ChatApplicationService::with_artifact_service(
        Arc::new(FakeRepository::default()),
        agent,
        artifact_service,
        16,
    );

    let events = collect(start(&service).await).await;
    assert!(matches!(events[0], ApplicationEvent::StateChanged { .. }));
    assert!(matches!(events[1], ApplicationEvent::ProjectUpdated { .. }));
    assert!(matches!(events[2], ApplicationEvent::SlideUpdated { .. }));
    assert!(matches!(
        &events[3],
        ApplicationEvent::ArtifactUpdated { artifact, .. } if artifact.is_ready()
    ));
    assert!(matches!(events[4], ApplicationEvent::Completed { .. }));
    assert_eq!(events.iter().filter(|event| event.is_terminal()).count(), 1);

    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn chat_terminal_state_candidate_without_publication_service_fails_once() {
    let (service, _) = service(DriverOutcome::OutputThenComplete, Duration::from_secs(1));
    let events = collect(start(&service).await).await;
    assert!(!events.iter().any(|event| matches!(
        event,
        ApplicationEvent::ArtifactUpdated { .. } | ApplicationEvent::Completed { .. }
    )));
    assert!(matches!(
        events.last(),
        Some(ApplicationEvent::Failed { code, .. }) if code == "artifact_service_unavailable"
    ));
    assert_eq!(events.iter().filter(|event| event.is_terminal()).count(), 1);
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
