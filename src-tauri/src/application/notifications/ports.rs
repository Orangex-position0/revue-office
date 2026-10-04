use async_trait::async_trait;

use crate::application::identity::ActorId;

use super::{Notification, NotificationError, NotificationId, NotificationQuery};

#[async_trait]
pub trait NotificationRepository: Send + Sync {
    async fn list(&self, query: NotificationQuery) -> Result<Vec<Notification>, NotificationError>;
    async fn unread_count(&self, owner: &ActorId) -> Result<i64, NotificationError>;
    async fn mark_read(
        &self,
        owner: &ActorId,
        id: &NotificationId,
    ) -> Result<bool, NotificationError>;
    async fn mark_all_read(&self, owner: &ActorId) -> Result<(), NotificationError>;
    async fn delete(&self, owner: &ActorId, id: &NotificationId)
    -> Result<bool, NotificationError>;
}
