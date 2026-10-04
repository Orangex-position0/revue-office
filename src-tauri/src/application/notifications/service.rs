use std::sync::Arc;

use crate::application::identity::Actor;

use super::{
    Notification, NotificationError, NotificationId, NotificationQuery, NotificationRepository,
};

pub struct NotificationApplicationService {
    repository: Arc<dyn NotificationRepository>,
}

impl NotificationApplicationService {
    pub fn new(repository: Arc<dyn NotificationRepository>) -> Self {
        Self { repository }
    }

    pub async fn list(
        &self,
        actor: &Actor,
        unread_only: bool,
        page: Option<u32>,
        page_size: Option<u32>,
    ) -> Result<Vec<Notification>, NotificationError> {
        self.repository
            .list(NotificationQuery {
                owner_id: actor.id.clone(),
                unread_only,
                page: page.unwrap_or(1).max(1),
                page_size: page_size.unwrap_or(50),
            })
            .await
    }

    pub async fn unread_count(&self, actor: &Actor) -> Result<i64, NotificationError> {
        self.repository.unread_count(&actor.id).await
    }

    pub async fn mark_read(&self, actor: &Actor, id: String) -> Result<bool, NotificationError> {
        self.repository
            .mark_read(&actor.id, &NotificationId(id))
            .await
    }

    pub async fn mark_all_read(&self, actor: &Actor) -> Result<(), NotificationError> {
        self.repository.mark_all_read(&actor.id).await
    }

    pub async fn delete(&self, actor: &Actor, id: String) -> Result<bool, NotificationError> {
        self.repository.delete(&actor.id, &NotificationId(id)).await
    }
}
