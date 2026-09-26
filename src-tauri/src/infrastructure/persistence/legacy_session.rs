use async_trait::async_trait;

use crate::contracts::conversation::{
    Conversation, ConversationArtifact, ConversationMessage, NewConversation,
};
use crate::db::{session_repo, DbPool};
use crate::models::{Artifact, ChatMessage};
use crate::ports::repositories::session::{SessionRepository, SessionRepositoryError};

pub struct LegacyAnySessionRepository {
    pool: DbPool,
}

impl LegacyAnySessionRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    fn unavailable(error: impl std::fmt::Display) -> SessionRepositoryError {
        SessionRepositoryError::Unavailable(anyhow::anyhow!(error.to_string()))
    }
}

impl From<session_repo::SessionRow> for Conversation {
    fn from(row: session_repo::SessionRow) -> Self {
        Self {
            id: row.id,
            owner_id: row.owner_id,
            project_id: row.project_id,
            tool_kind: row.tool_kind,
            title: row.title,
            summary: row.summary,
            message_count: row.message_count,
            order: row.order_col,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

fn artifact_to_contract(artifact: Artifact) -> ConversationArtifact {
    ConversationArtifact {
        id: artifact.id,
        kind: artifact.kind,
        tool_kind: artifact.tool_kind,
        title: artifact.title,
        status: artifact.status,
        content: artifact.content,
        version: artifact.version,
        created_at: artifact.created_at,
        updated_at: artifact.updated_at,
    }
}

fn artifact_to_legacy(artifact: ConversationArtifact) -> Artifact {
    Artifact {
        id: artifact.id,
        kind: artifact.kind,
        tool_kind: artifact.tool_kind,
        title: artifact.title,
        status: artifact.status,
        content: artifact.content,
        version: artifact.version,
        created_at: artifact.created_at,
        updated_at: artifact.updated_at,
    }
}

#[async_trait]
impl SessionRepository for LegacyAnySessionRepository {
    async fn create(
        &self,
        request: NewConversation,
    ) -> Result<Conversation, SessionRepositoryError> {
        session_repo::create(
            &self.pool,
            &request.owner_id,
            request.project_id.as_deref(),
            request.tool_kind.as_deref(),
            &request.title,
        )
        .await
        .map(Into::into)
        .map_err(Self::unavailable)
    }

    async fn find_by_id(&self, id: &str) -> Result<Option<Conversation>, SessionRepositoryError> {
        session_repo::find_by_id(&self.pool, id)
            .await
            .map(|row| row.map(Into::into))
            .map_err(Self::unavailable)
    }

    async fn list_by_owner(
        &self,
        owner_id: &str,
        limit: u32,
        query: Option<&str>,
    ) -> Result<Vec<Conversation>, SessionRepositoryError> {
        session_repo::list_by_owner(&self.pool, owner_id, i64::from(limit), query)
            .await
            .map(|rows| rows.into_iter().map(Into::into).collect())
            .map_err(Self::unavailable)
    }

    async fn append_message(
        &self,
        session_id: &str,
        message: ConversationMessage,
    ) -> Result<(), SessionRepositoryError> {
        session_repo::add_message(
            &self.pool,
            session_id,
            &ChatMessage {
                role: message.role,
                content: message.content,
                tool_calls: message.tool_calls,
                tool_call_id: message.tool_call_id,
            },
        )
        .await
        .map_err(Self::unavailable)
    }

    async fn history(
        &self,
        session_id: &str,
        limit: u32,
    ) -> Result<Vec<ConversationMessage>, SessionRepositoryError> {
        session_repo::get_persisted_messages(&self.pool, session_id, i64::from(limit))
            .await
            .map(|messages| {
                messages
                    .into_iter()
                    .map(|message| ConversationMessage {
                        role: message.role,
                        content: message.content,
                        tool_calls: message.tool_calls,
                        tool_call_id: message.tool_call_id,
                        created_at: message.created_at,
                    })
                    .collect()
            })
            .map_err(Self::unavailable)
    }

    async fn legacy_artifacts(
        &self,
        session_id: &str,
    ) -> Result<Vec<ConversationArtifact>, SessionRepositoryError> {
        session_repo::get_artifacts(&self.pool, session_id)
            .await
            .map(|artifacts| artifacts.into_iter().map(artifact_to_contract).collect())
            .map_err(Self::unavailable)
    }

    async fn replace_legacy_artifacts(
        &self,
        session_id: &str,
        artifacts: Vec<ConversationArtifact>,
    ) -> Result<(), SessionRepositoryError> {
        let artifacts = artifacts
            .into_iter()
            .map(artifact_to_legacy)
            .collect::<Vec<_>>();
        session_repo::save_artifacts(&self.pool, session_id, &artifacts)
            .await
            .map_err(Self::unavailable)
    }

    async fn update_title(
        &self,
        session_id: &str,
        owner_id: &str,
        title: &str,
    ) -> Result<bool, SessionRepositoryError> {
        session_repo::update_title(&self.pool, session_id, owner_id, title)
            .await
            .map_err(Self::unavailable)
    }

    async fn update_placement(
        &self,
        session_id: &str,
        owner_id: &str,
        project_id: Option<&str>,
        order: i64,
    ) -> Result<bool, SessionRepositoryError> {
        session_repo::update_project_and_order(&self.pool, session_id, owner_id, project_id, order)
            .await
            .map_err(Self::unavailable)
    }

    async fn update_summary(
        &self,
        session_id: &str,
        summary: &str,
    ) -> Result<(), SessionRepositoryError> {
        session_repo::update_summary(&self.pool, session_id, summary)
            .await
            .map_err(Self::unavailable)
    }

    async fn delete(
        &self,
        session_id: &str,
        owner_id: &str,
    ) -> Result<bool, SessionRepositoryError> {
        session_repo::delete(&self.pool, session_id, owner_id)
            .await
            .map_err(Self::unavailable)
    }

    async fn clear_messages(
        &self,
        session_id: &str,
        owner_id: &str,
    ) -> Result<bool, SessionRepositoryError> {
        session_repo::clear_messages(&self.pool, session_id, owner_id)
            .await
            .map_err(Self::unavailable)
    }
}
