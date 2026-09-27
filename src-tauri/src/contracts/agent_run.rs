use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeAttachment {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub mime_type: String,
    pub size: usize,
    pub text_content: Option<String>,
    pub data_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeRequest {
    pub run_id: String,
    pub session_id: String,
    pub user_id: String,
    pub project_id: Option<String>,
    pub preferred_model: Option<String>,
    pub attachments: Vec<RuntimeAttachment>,
    pub tool_config: Option<serde_json::Value>,
    pub allowed_tools: Option<Vec<String>>,
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
