use serde::{Deserialize, Serialize};

use crate::application::identity::ActorId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub tool_kind: String,
    pub owner_id: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewProject {
    pub owner_id: ActorId,
    pub title: String,
    pub description: Option<String>,
    pub tool_kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectListQuery {
    pub owner_id: ActorId,
    pub query: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectUpdate {
    pub owner_id: ActorId,
    pub id: ProjectId,
    pub title: Option<String>,
    /// `None` leaves the value unchanged; `Some(None)` clears it.
    pub description: Option<Option<String>>,
    pub tool_kind: Option<String>,
}
