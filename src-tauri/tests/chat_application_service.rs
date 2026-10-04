use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use revue_office_lib::agent::{
    AgentRunner, OfficeAgentEvent, OfficeAgentRequest, OfficeAgentRunHandle,
    OfficeCancellationHandle, OfficeFailureKind,
};
use revue_office_lib::application::chat_service::{ChatApplicationService, ChatCommand};
use revue_office_lib::application::conversations::model::{
    Conversation, ConversationArtifact, ConversationMessage, NewConversation,
};
use revue_office_lib::application::conversations::{SessionRepository, SessionRepositoryError};
use revue_office_lib::application::event::ApplicationEvent;

#[derive(Default)]
struct FakeSessionRepository {
    conversations: Mutex<HashMap<String, Conversation>>,
    messages: Mutex<HashMap<String, Vec<ConversationMessage>>>,
}

impl FakeSessionRepository {
    fn messages(&self, session_id: &str) -> Vec<ConversationMessage> {
        self.messages
            .lock()
            .unwrap()
            .get(session_id)
            .cloned()
            .unwrap_or_default()
    }
}

#[async_trait]
impl SessionRepository for FakeSessionRepository {
    async fn create(
        &self,
        request: NewConversation,
    ) -> Result<Conversation, SessionRepositoryError> {
        let conversation = Conversation {
            id: "created-session".into(),
            owner_id: request.owner_id,
            project_id: request.project_id,
            tool_kind: request.tool_kind,
            title: request.title,
            summary: None,
            message_count: 0,
            order: 1,
            created_at: "2026-09-26T00:00:00Z".into(),
            updated_at: "2026-09-26T00:00:00Z".into(),
        };
        self.conversations
            .lock()
            .unwrap()
            .insert(conversation.id.clone(), conversation.clone());
        Ok(conversation)
    }

    async fn find_by_id(&self, id: &str) -> Result<Option<Conversation>, SessionRepositoryError> {
        Ok(self.conversations.lock().unwrap().get(id).cloned())
    }

    async fn list_by_owner(
        &self,
        _owner_id: &str,
        _limit: u32,
        _query: Option<&str>,
    ) -> Result<Vec<Conversation>, SessionRepositoryError> {
        Ok(Vec::new())
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
        Ok(self.messages(session_id))
    }

    async fn legacy_artifacts(
        &self,
        _session_id: &str,
    ) -> Result<Vec<ConversationArtifact>, SessionRepositoryError> {
        Ok(Vec::new())
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

struct FakeNoToolAgent {
    requests: Arc<Mutex<Vec<OfficeAgentRequest>>>,
}

impl AgentRunner for FakeNoToolAgent {
    fn start(&self, request: OfficeAgentRequest) -> OfficeAgentRunHandle {
        let run_id = request.run_id.clone();
        self.requests.lock().unwrap().push(request);
        fake_run(
            run_id,
            vec![
                OfficeAgentEvent::Thinking {
                    content: "Preparing response".into(),
                },
                OfficeAgentEvent::MessageProduced {
                    content: "Assistant reply".into(),
                },
                OfficeAgentEvent::Completed {
                    summary: "Finished".into(),
                },
            ],
        )
    }
}

fn fake_run(run_id: String, events: Vec<OfficeAgentEvent>) -> OfficeAgentRunHandle {
    let (sender, receiver) = tokio::sync::mpsc::channel(events.len().max(1));
    let (cancellation, _cancelled) = OfficeCancellationHandle::channel();
    tokio::spawn(async move {
        for event in events {
            if sender.send(event).await.is_err() {
                break;
            }
        }
    });
    OfficeAgentRunHandle::new(run_id, receiver, cancellation)
}

#[tokio::test]
async fn chat_application_service_creates_session_persists_messages_and_publishes_one_terminal() {
    let repository = Arc::new(FakeSessionRepository::default());
    let requests = Arc::new(Mutex::new(Vec::new()));
    let agent = Arc::new(FakeNoToolAgent {
        requests: requests.clone(),
    });
    let service = ChatApplicationService::new(repository.clone(), agent, 4);

    let mut run = service
        .start_chat(ChatCommand {
            actor: revue_office_lib::application::identity::Actor::user("owner-1"),
            session_id: None,
            project_id: None,
            message: "Hello application boundary".into(),
            runtime_message: None,
            preferred_model: None,
            attachments: Vec::new(),
            tool_config: None,
            allowed_tools: Some(Vec::new()),
            max_turns: 4,
        })
        .await
        .expect("chat should start");

    assert_eq!(run.session_id, "created-session");
    let mut events = Vec::new();
    while let Some(event) = run.events.recv().await {
        let terminal = event.is_terminal();
        events.push(event);
        if terminal {
            break;
        }
    }

    assert!(matches!(events[0], ApplicationEvent::StateChanged { .. }));
    assert!(matches!(
        events[1],
        ApplicationEvent::Message { ref content } if content == "Assistant reply"
    ));
    assert!(matches!(events[2], ApplicationEvent::Completed { .. }));
    assert_eq!(events.iter().filter(|event| event.is_terminal()).count(), 1);

    let stored = repository.messages("created-session");
    assert_eq!(stored.len(), 2);
    assert_eq!(stored[0].role, "user");
    assert_eq!(stored[0].content, "Hello application boundary");
    assert_eq!(stored[1].role, "assistant");
    assert_eq!(stored[1].content, "Assistant reply");
    assert!(requests.lock().unwrap()[0].history.is_empty());
}

struct FailingAgent;

impl AgentRunner for FailingAgent {
    fn start(&self, request: OfficeAgentRequest) -> OfficeAgentRunHandle {
        fake_run(
            request.run_id,
            vec![OfficeAgentEvent::Failed {
                kind: OfficeFailureKind::Provider,
                message: "fake provider failure".into(),
            }],
        )
    }
}

#[tokio::test]
async fn chat_application_service_publishes_exactly_one_failed_terminal() {
    let repository = Arc::new(FakeSessionRepository::default());
    let service = ChatApplicationService::new(repository, Arc::new(FailingAgent), 2);
    let mut run = service
        .start_chat(ChatCommand {
            actor: revue_office_lib::application::identity::Actor::user("owner-1"),
            session_id: None,
            project_id: None,
            message: "Fail safely".into(),
            runtime_message: None,
            preferred_model: None,
            attachments: Vec::new(),
            tool_config: None,
            allowed_tools: Some(Vec::new()),
            max_turns: 4,
        })
        .await
        .expect("chat should start before the runtime failure");

    let failed = run.events.recv().await.expect("failed event should arrive");
    assert!(matches!(
        failed,
        ApplicationEvent::Failed { ref code, .. } if code == "runtime_model"
    ));
    assert!(run.events.recv().await.is_none());
}

#[tokio::test]
async fn chat_application_service_restores_owned_history_before_starting_runtime() {
    let repository = Arc::new(FakeSessionRepository::default());
    repository.conversations.lock().unwrap().insert(
        "existing-session".into(),
        Conversation {
            id: "existing-session".into(),
            owner_id: "owner-1".into(),
            project_id: None,
            tool_kind: Some("general".into()),
            title: "Existing".into(),
            summary: None,
            message_count: 1,
            order: 1,
            created_at: "2026-09-26T00:00:00Z".into(),
            updated_at: "2026-09-26T00:00:00Z".into(),
        },
    );
    repository.messages.lock().unwrap().insert(
        "existing-session".into(),
        vec![ConversationMessage {
            role: "assistant".into(),
            content: "Earlier answer".into(),
            tool_calls: None,
            tool_call_id: None,
            created_at: "2026-09-26T00:00:00Z".into(),
        }],
    );
    let requests = Arc::new(Mutex::new(Vec::new()));
    let agent = Arc::new(FakeNoToolAgent {
        requests: requests.clone(),
    });
    let service = ChatApplicationService::new(repository.clone(), agent, 4);

    let mut run = service
        .start_chat(ChatCommand {
            actor: revue_office_lib::application::identity::Actor::user("owner-1"),
            session_id: Some("existing-session".into()),
            project_id: None,
            message: "Continue".into(),
            runtime_message: None,
            preferred_model: None,
            attachments: Vec::new(),
            tool_config: None,
            allowed_tools: Some(Vec::new()),
            max_turns: 4,
        })
        .await
        .expect("existing chat should start");
    while let Some(event) = run.events.recv().await {
        if event.is_terminal() {
            break;
        }
    }

    let captured = &requests.lock().unwrap()[0];
    assert_eq!(captured.session_id, "existing-session");
    assert_eq!(captured.history.len(), 1);
    assert_eq!(captured.history[0].content, "Earlier answer");
    assert_eq!(repository.messages("existing-session").len(), 3);
}
