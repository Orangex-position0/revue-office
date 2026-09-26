use async_trait::async_trait;
use thiserror::Error;

use crate::contracts::conversation::{
    Conversation, ConversationArtifact, ConversationMessage, NewConversation,
};

#[derive(Debug, Error)]
pub enum SessionRepositoryError {
    #[error("session repository is unavailable")]
    Unavailable(#[source] anyhow::Error),
    #[error("session data is invalid: {0}")]
    InvalidData(String),
    #[error("session write conflicts with existing data")]
    Conflict,
}

#[async_trait]
pub trait SessionRepository: Send + Sync {
    async fn create(
        &self,
        request: NewConversation,
    ) -> Result<Conversation, SessionRepositoryError>;

    async fn find_by_id(&self, id: &str) -> Result<Option<Conversation>, SessionRepositoryError>;

    async fn list_by_owner(
        &self,
        owner_id: &str,
        limit: u32,
        query: Option<&str>,
    ) -> Result<Vec<Conversation>, SessionRepositoryError>;

    async fn append_message(
        &self,
        session_id: &str,
        message: ConversationMessage,
    ) -> Result<(), SessionRepositoryError>;

    async fn history(
        &self,
        session_id: &str,
        limit: u32,
    ) -> Result<Vec<ConversationMessage>, SessionRepositoryError>;

    async fn legacy_artifacts(
        &self,
        session_id: &str,
    ) -> Result<Vec<ConversationArtifact>, SessionRepositoryError>;

    async fn replace_legacy_artifacts(
        &self,
        session_id: &str,
        artifacts: Vec<ConversationArtifact>,
    ) -> Result<(), SessionRepositoryError>;

    async fn update_title(
        &self,
        session_id: &str,
        owner_id: &str,
        title: &str,
    ) -> Result<bool, SessionRepositoryError>;

    async fn update_placement(
        &self,
        session_id: &str,
        owner_id: &str,
        project_id: Option<&str>,
        order: i64,
    ) -> Result<bool, SessionRepositoryError>;

    async fn update_summary(
        &self,
        session_id: &str,
        summary: &str,
    ) -> Result<(), SessionRepositoryError>;

    async fn delete(
        &self,
        session_id: &str,
        owner_id: &str,
    ) -> Result<bool, SessionRepositoryError>;

    async fn clear_messages(
        &self,
        session_id: &str,
        owner_id: &str,
    ) -> Result<bool, SessionRepositoryError>;
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use async_trait::async_trait;

    use crate::contracts::conversation::{
        Conversation, ConversationArtifact, ConversationMessage, NewConversation,
    };

    use super::{SessionRepository, SessionRepositoryError};

    struct FakeSessionRepository {
        conversations: Mutex<HashMap<String, Conversation>>,
        messages: Mutex<HashMap<String, Vec<ConversationMessage>>>,
        artifacts: Mutex<HashMap<String, Vec<ConversationArtifact>>>,
    }

    impl Default for FakeSessionRepository {
        fn default() -> Self {
            Self {
                conversations: Mutex::new(HashMap::new()),
                messages: Mutex::new(HashMap::new()),
                artifacts: Mutex::new(HashMap::from([(
                    "session-1".into(),
                    vec![ConversationArtifact {
                        id: "artifact-1".into(),
                        kind: "presentation".into(),
                        tool_kind: "ppt".into(),
                        title: "Legacy deck".into(),
                        status: "completed".into(),
                        content: serde_json::json!({"projectId": "legacy-project"}),
                        version: 1,
                        created_at: "2026-09-25T00:00:00Z".into(),
                        updated_at: "2026-09-25T00:00:00Z".into(),
                    }],
                )])),
            }
        }
    }

    #[async_trait]
    impl SessionRepository for FakeSessionRepository {
        async fn create(
            &self,
            request: NewConversation,
        ) -> Result<Conversation, SessionRepositoryError> {
            let conversation = Conversation {
                id: "session-1".into(),
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

        async fn find_by_id(
            &self,
            id: &str,
        ) -> Result<Option<Conversation>, SessionRepositoryError> {
            Ok(self.conversations.lock().unwrap().get(id).cloned())
        }

        async fn list_by_owner(
            &self,
            owner_id: &str,
            limit: u32,
            query: Option<&str>,
        ) -> Result<Vec<Conversation>, SessionRepositoryError> {
            let query = query.map(str::to_lowercase);
            Ok(self
                .conversations
                .lock()
                .unwrap()
                .values()
                .filter(|conversation| conversation.owner_id == owner_id)
                .filter(|conversation| {
                    query.as_ref().is_none_or(|query| {
                        conversation.title.to_lowercase().contains(query)
                            || conversation
                                .summary
                                .as_ref()
                                .is_some_and(|summary| summary.to_lowercase().contains(query))
                    })
                })
                .take(limit as usize)
                .cloned()
                .collect())
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
            session_id: &str,
        ) -> Result<Vec<ConversationArtifact>, SessionRepositoryError> {
            Ok(self
                .artifacts
                .lock()
                .unwrap()
                .get(session_id)
                .cloned()
                .unwrap_or_default())
        }

        async fn replace_legacy_artifacts(
            &self,
            session_id: &str,
            artifacts: Vec<ConversationArtifact>,
        ) -> Result<(), SessionRepositoryError> {
            self.artifacts
                .lock()
                .unwrap()
                .insert(session_id.into(), artifacts);
            Ok(())
        }

        async fn update_title(
            &self,
            session_id: &str,
            owner_id: &str,
            title: &str,
        ) -> Result<bool, SessionRepositoryError> {
            let mut conversations = self.conversations.lock().unwrap();
            let Some(conversation) = conversations.get_mut(session_id) else {
                return Ok(false);
            };
            if conversation.owner_id != owner_id {
                return Ok(false);
            }
            conversation.title = title.into();
            Ok(true)
        }

        async fn update_placement(
            &self,
            session_id: &str,
            owner_id: &str,
            project_id: Option<&str>,
            order: i64,
        ) -> Result<bool, SessionRepositoryError> {
            let mut conversations = self.conversations.lock().unwrap();
            let Some(conversation) = conversations.get_mut(session_id) else {
                return Ok(false);
            };
            if conversation.owner_id != owner_id {
                return Ok(false);
            }
            conversation.project_id = project_id.map(str::to_owned);
            conversation.order = order;
            Ok(true)
        }

        async fn update_summary(
            &self,
            session_id: &str,
            summary: &str,
        ) -> Result<(), SessionRepositoryError> {
            if let Some(conversation) = self.conversations.lock().unwrap().get_mut(session_id) {
                conversation.summary = Some(summary.into());
            }
            Ok(())
        }

        async fn delete(
            &self,
            session_id: &str,
            owner_id: &str,
        ) -> Result<bool, SessionRepositoryError> {
            let mut conversations = self.conversations.lock().unwrap();
            if conversations
                .get(session_id)
                .is_some_and(|conversation| conversation.owner_id == owner_id)
            {
                conversations.remove(session_id);
                return Ok(true);
            }
            Ok(false)
        }

        async fn clear_messages(
            &self,
            session_id: &str,
            owner_id: &str,
        ) -> Result<bool, SessionRepositoryError> {
            if !self
                .conversations
                .lock()
                .unwrap()
                .get(session_id)
                .is_some_and(|conversation| conversation.owner_id == owner_id)
            {
                return Ok(false);
            }
            self.messages.lock().unwrap().remove(session_id);
            Ok(true)
        }
    }

    async fn exercise_session_repository_contract(repository: &dyn SessionRepository) {
        let created = repository
            .create(NewConversation {
                owner_id: "owner-1".into(),
                project_id: Some("project-1".into()),
                tool_kind: Some("presentation".into()),
                title: "Quarterly review".into(),
            })
            .await
            .unwrap();
        assert_eq!(created.owner_id, "owner-1");

        let restored = repository.find_by_id(&created.id).await.unwrap().unwrap();
        assert_eq!(restored.title, "Quarterly review");
        assert_eq!(
            repository
                .list_by_owner("owner-1", 10, None)
                .await
                .unwrap()
                .len(),
            1
        );

        repository
            .append_message(
                &created.id,
                ConversationMessage {
                    role: "user".into(),
                    content: "Create the deck".into(),
                    tool_calls: None,
                    tool_call_id: None,
                    created_at: "2026-09-26T00:00:01Z".into(),
                },
            )
            .await
            .unwrap();
        let history = repository.history(&created.id, 100).await.unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].content, "Create the deck");
        let artifacts = repository.legacy_artifacts(&created.id).await.unwrap();
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].title, "Legacy deck");
    }

    #[tokio::test]
    async fn session_repository_contract_supports_a_fake_adapter() {
        exercise_session_repository_contract(&FakeSessionRepository::default()).await;
    }
}
