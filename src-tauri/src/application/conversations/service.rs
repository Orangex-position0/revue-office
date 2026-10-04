use std::sync::Arc;

use crate::application::identity::Actor;

use super::error::SessionApplicationError;
use super::model::{
    Conversation, ConversationArtifact, ConversationDetail, ConversationMessage,
    ConversationUpdate, NewConversation,
};
use super::ports::SessionRepository;

pub struct SessionApplicationService {
    repository: Arc<dyn SessionRepository>,
}
impl SessionApplicationService {
    pub fn new(repository: Arc<dyn SessionRepository>) -> Self {
        Self { repository }
    }
    pub async fn create(
        &self,
        actor: &Actor,
        mut request: NewConversation,
    ) -> Result<Conversation, SessionApplicationError> {
        if request.title.trim().is_empty() {
            return Err(SessionApplicationError::EmptyTitle);
        }
        request.owner_id = actor.id.0.clone();
        Ok(self.repository.create(request).await?)
    }
    pub async fn list(
        &self,
        actor: &Actor,
        limit: u32,
        query: Option<&str>,
    ) -> Result<Vec<Conversation>, SessionApplicationError> {
        Ok(self
            .repository
            .list_by_owner(&actor.id.0, limit, query)
            .await?)
    }
    pub async fn detail(
        &self,
        actor: &Actor,
        session_id: &str,
    ) -> Result<ConversationDetail, SessionApplicationError> {
        let conversation = self.owned_conversation(actor, session_id).await?;
        Ok(ConversationDetail {
            conversation,
            messages: self.repository.history(session_id, 100).await?,
            artifacts: self.repository.legacy_artifacts(session_id).await?,
        })
    }
    pub async fn messages(
        &self,
        actor: &Actor,
        session_id: &str,
        limit: u32,
    ) -> Result<Vec<ConversationMessage>, SessionApplicationError> {
        self.owned_conversation(actor, session_id).await?;
        Ok(self.repository.history(session_id, limit).await?)
    }
    pub async fn append_message(
        &self,
        actor: &Actor,
        session_id: &str,
        message: ConversationMessage,
    ) -> Result<(), SessionApplicationError> {
        self.owned_conversation(actor, session_id).await?;
        Ok(self.repository.append_message(session_id, message).await?)
    }
    pub async fn replace_legacy_artifacts(
        &self,
        actor: &Actor,
        session_id: &str,
        artifacts: Vec<ConversationArtifact>,
    ) -> Result<(), SessionApplicationError> {
        self.owned_conversation(actor, session_id).await?;
        Ok(self
            .repository
            .replace_legacy_artifacts(session_id, artifacts)
            .await?)
    }
    pub async fn update(
        &self,
        actor: &Actor,
        session_id: &str,
        update: ConversationUpdate,
    ) -> Result<bool, SessionApplicationError> {
        let conversation = self.owned_conversation(actor, session_id).await?;
        let mut updated = false;
        if let Some(title) = update.title {
            let title = title.trim();
            if title.is_empty() {
                return Err(SessionApplicationError::EmptyTitle);
            }
            updated |= self
                .repository
                .update_title(session_id, &actor.id.0, title)
                .await?;
        }
        if update.project_id.is_some() || update.order.is_some() {
            let project_id = update.project_id.unwrap_or(conversation.project_id);
            let order = update.order.unwrap_or(conversation.order);
            updated |= self
                .repository
                .update_placement(session_id, &actor.id.0, project_id.as_deref(), order)
                .await?;
        }
        Ok(updated)
    }
    pub async fn delete(
        &self,
        actor: &Actor,
        session_id: &str,
    ) -> Result<bool, SessionApplicationError> {
        Ok(self.repository.delete(session_id, &actor.id.0).await?)
    }
    pub async fn clear(
        &self,
        actor: &Actor,
        session_id: &str,
    ) -> Result<bool, SessionApplicationError> {
        Ok(self
            .repository
            .clear_messages(session_id, &actor.id.0)
            .await?)
    }
    async fn owned_conversation(
        &self,
        actor: &Actor,
        session_id: &str,
    ) -> Result<Conversation, SessionApplicationError> {
        let conversation = self
            .repository
            .find_by_id(session_id)
            .await?
            .ok_or(SessionApplicationError::NotFound)?;
        if !actor.owns(&conversation.owner_id) {
            return Err(SessionApplicationError::Forbidden);
        }
        Ok(conversation)
    }
}
