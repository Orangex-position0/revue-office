use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Conversation {
    pub id: String,
    pub owner_id: String,
    pub project_id: Option<String>,
    pub tool_kind: Option<String>,
    pub title: String,
    pub summary: Option<String>,
    pub message_count: i64,
    pub order: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewConversation {
    pub owner_id: String,
    pub project_id: Option<String>,
    pub tool_kind: Option<String>,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationMessage {
    pub role: String,
    pub content: String,
    pub tool_calls: Option<Vec<serde_json::Value>>,
    pub tool_call_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationArtifact {
    pub id: String,
    pub kind: String,
    pub tool_kind: String,
    pub title: String,
    pub status: String,
    pub content: serde_json::Value,
    pub version: i32,
    pub created_at: String,
    pub updated_at: String,
}
