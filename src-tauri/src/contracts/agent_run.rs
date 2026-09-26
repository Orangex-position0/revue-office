use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeRequest {
    pub run_id: String,
    pub session_id: String,
    pub user_id: String,
    pub history: Vec<RuntimeMessage>,
    pub user_message: String,
    pub max_turns: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeArtifact {
    pub kind: String,
    pub title: String,
    pub content: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeCompletion {
    pub summary: String,
}
