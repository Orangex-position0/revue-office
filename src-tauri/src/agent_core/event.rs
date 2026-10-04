use serde_json::Value;

use super::{AgentFailureKind, GeneratedOutput};

#[derive(Debug, Clone, PartialEq)]
pub enum AgentEvent {
    Thinking {
        content: String,
    },
    ToolStarted {
        tool: String,
        input: Value,
    },
    ToolProgress {
        tool: String,
        stage: String,
        detail: Value,
    },
    ToolFinished {
        tool: String,
        result: Value,
    },
    OutputProduced {
        output: GeneratedOutput,
    },
    MessageProduced {
        content: String,
    },
    TurnFinished {
        turn: usize,
    },
    Completed {
        summary: String,
        outputs: Vec<GeneratedOutput>,
    },
    Failed {
        kind: AgentFailureKind,
        message: String,
    },
}

impl AgentEvent {
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed { .. } | Self::Failed { .. })
    }
}
