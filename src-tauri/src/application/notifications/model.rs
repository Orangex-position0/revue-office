use serde::Serialize;

use crate::application::identity::ActorId;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct NotificationId(pub String);

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Notification {
    pub id: String,
    pub user_id: String,
    #[serde(rename = "type")]
    pub notification_type: String,
    pub title: String,
    pub content: Option<String>,
    pub is_read: bool,
    pub link: Option<String>,
    pub created_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotificationQuery {
    pub owner_id: ActorId,
    pub unread_only: bool,
    pub page: u32,
    pub page_size: u32,
}

impl NotificationQuery {
    pub fn limit(&self) -> i64 {
        i64::from(self.page_size)
    }

    pub fn offset(&self) -> i64 {
        i64::from(self.page.saturating_sub(1)).saturating_mul(i64::from(self.page_size))
    }
}
