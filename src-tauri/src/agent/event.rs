use serde::{Deserialize, Serialize};

use crate::contracts::agent_run::RuntimeArtifact;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RuntimeEvent {
    Thinking {
        content: String,
    },
    ToolStarted {
        tool: String,
        input: serde_json::Value,
    },
    ToolProgress {
        tool: String,
        stage: String,
        detail: serde_json::Value,
    },
    ToolFinished {
        tool: String,
        success: bool,
        result: serde_json::Value,
    },
    ArtifactProduced {
        artifact: RuntimeArtifact,
    },
    MessageProduced {
        content: String,
    },
    TurnFinished {
        turn: usize,
    },
    Completed {
        summary: String,
    },
    Failed {
        kind: RuntimeFailureKind,
        message: String,
    },
}

impl RuntimeEvent {
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed { .. } | Self::Failed { .. })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeFailureKind {
    Cancelled,
    Model,
    Tool,
    Internal,
}
