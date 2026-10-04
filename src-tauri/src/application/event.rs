use serde::{Deserialize, Serialize};

use super::artifacts::ArtifactPublication;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ApplicationEvent {
    StateChanged {
        state: String,
        detail: serde_json::Value,
    },
    ToolResult {
        tool: String,
        success: bool,
        result: serde_json::Value,
    },
    ProjectUpdated {
        project: serde_json::Value,
    },
    SlideUpdated {
        slide: serde_json::Value,
    },
    ArtifactUpdated {
        artifact: ArtifactPublication,
        artifacts: Vec<ArtifactPublication>,
    },
    Message {
        content: String,
    },
    LegacyToolProgress {
        event: String,
        data: serde_json::Value,
    },
    Completed {
        summary: String,
        artifacts: Vec<ArtifactPublication>,
        new_artifacts: Vec<ArtifactPublication>,
    },
    Failed {
        code: String,
        message: String,
    },
}

impl ApplicationEvent {
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed { .. } | Self::Failed { .. })
    }
}
