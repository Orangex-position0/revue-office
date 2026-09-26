use std::sync::Arc;

use thiserror::Error;

use crate::contracts::conversation::{
    Conversation, ConversationArtifact, ConversationDetail, ConversationMessage,
    ConversationUpdate, NewConversation,
};
use crate::ports::repositories::session::{SessionRepository, SessionRepositoryError};

#[derive(Debug, Error)]
pub enum SessionApplicationError {
    #[error("conversation not found")]
    NotFound,
    #[error("conversation access is forbidden")]
    Forbidden,
    #[error("conversation title must not be empty")]
    EmptyTitle,
    #[error(transparent)]
    Repository(#[from] SessionRepositoryError),
}

pub struct SessionApplicationService {
    repository: Arc<dyn SessionRepository>,
}

impl SessionApplicationService {
    pub fn new(repository: Arc<dyn SessionRepository>) -> Self {
        Self { repository }
    }

    pub async fn create(
        &self,
        request: NewConversation,
    ) -> Result<Conversation, SessionApplicationError> {
        if request.title.trim().is_empty() {
            return Err(SessionApplicationError::EmptyTitle);
        }
        Ok(self.repository.create(request).await?)
    }

    pub async fn list(
        &self,
        owner_id: &str,
        limit: u32,
        query: Option<&str>,
    ) -> Result<Vec<Conversation>, SessionApplicationError> {
        Ok(self
            .repository
            .list_by_owner(owner_id, limit, query)
            .await?)
    }

    pub async fn detail(
        &self,
        owner_id: &str,
        session_id: &str,
    ) -> Result<ConversationDetail, SessionApplicationError> {
        let conversation = self.owned_conversation(owner_id, session_id).await?;
        let messages = self.repository.history(session_id, 100).await?;
        let artifacts = self.repository.legacy_artifacts(session_id).await?;
        Ok(ConversationDetail {
            conversation,
            messages,
            artifacts,
        })
    }

    pub async fn messages(
        &self,
        owner_id: &str,
        session_id: &str,
        limit: u32,
    ) -> Result<Vec<ConversationMessage>, SessionApplicationError> {
        self.owned_conversation(owner_id, session_id).await?;
        Ok(self.repository.history(session_id, limit).await?)
    }

    pub async fn append_message(
        &self,
        owner_id: &str,
        session_id: &str,
        message: ConversationMessage,
    ) -> Result<(), SessionApplicationError> {
        self.owned_conversation(owner_id, session_id).await?;
        Ok(self.repository.append_message(session_id, message).await?)
    }

    pub async fn replace_legacy_artifacts(
        &self,
        owner_id: &str,
        session_id: &str,
        artifacts: Vec<ConversationArtifact>,
    ) -> Result<(), SessionApplicationError> {
        self.owned_conversation(owner_id, session_id).await?;
        Ok(self
            .repository
            .replace_legacy_artifacts(session_id, artifacts)
            .await?)
    }

    pub async fn update(
        &self,
        owner_id: &str,
        session_id: &str,
        update: ConversationUpdate,
    ) -> Result<bool, SessionApplicationError> {
        let conversation = self.owned_conversation(owner_id, session_id).await?;
        let mut updated = false;
        if let Some(title) = update.title {
            let title = title.trim();
            if title.is_empty() {
                return Err(SessionApplicationError::EmptyTitle);
            }
            updated |= self
                .repository
                .update_title(session_id, owner_id, title)
                .await?;
        }
        if update.project_id.is_some() || update.order.is_some() {
            let project_id = update.project_id.unwrap_or(conversation.project_id);
            let order = update.order.unwrap_or(conversation.order);
            updated |= self
                .repository
                .update_placement(session_id, owner_id, project_id.as_deref(), order)
                .await?;
        }
        Ok(updated)
    }

    pub async fn delete(
        &self,
        owner_id: &str,
        session_id: &str,
    ) -> Result<bool, SessionApplicationError> {
        Ok(self.repository.delete(session_id, owner_id).await?)
    }

    pub async fn clear(
        &self,
        owner_id: &str,
        session_id: &str,
    ) -> Result<bool, SessionApplicationError> {
        Ok(self.repository.clear_messages(session_id, owner_id).await?)
    }

    async fn owned_conversation(
        &self,
        owner_id: &str,
        session_id: &str,
    ) -> Result<Conversation, SessionApplicationError> {
        let conversation = self
            .repository
            .find_by_id(session_id)
            .await?
            .ok_or(SessionApplicationError::NotFound)?;
        if conversation.owner_id != owner_id {
            return Err(SessionApplicationError::Forbidden);
        }
        Ok(conversation)
    }
}
