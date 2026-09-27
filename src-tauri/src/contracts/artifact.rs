use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactPublicationStatus {
    Publishing,
    Ready,
    Failed,
}

impl ArtifactPublicationStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Publishing => "publishing",
            Self::Ready => "ready",
            Self::Failed => "failed",
        }
    }
}

impl TryFrom<&str> for ArtifactPublicationStatus {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "publishing" => Ok(Self::Publishing),
            "ready" => Ok(Self::Ready),
            "failed" => Ok(Self::Failed),
            other => Err(format!("unknown artifact publication status: {other}")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewArtifactPublication {
    pub id: String,
    pub session_id: String,
    pub owner_id: String,
    pub kind: String,
    pub title: String,
    pub content: serde_json::Value,
    pub staging_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactDraft {
    pub session_id: String,
    pub owner_id: String,
    pub kind: String,
    pub title: String,
    pub extension: String,
    pub content: serde_json::Value,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactFinalization {
    pub final_path: String,
    pub content: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactPublication {
    pub id: String,
    pub session_id: String,
    pub owner_id: String,
    pub kind: String,
    pub title: String,
    pub status: ArtifactPublicationStatus,
    pub content: serde_json::Value,
    pub staging_path: Option<String>,
    pub final_path: Option<String>,
    pub error: Option<String>,
    pub version: i32,
    pub created_at: String,
    pub updated_at: String,
}
