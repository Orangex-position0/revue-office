#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentFailureKind {
    Cancelled,
    Timeout,
    Provider,
    Tool,
    MaximumTurns,
    Internal,
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum AgentError {
    #[error("agent run was cancelled")]
    Cancelled,
    #[error("agent run timed out")]
    Timeout,
    #[error("provider failed: {0}")]
    Provider(String),
    #[error("tool `{tool}` failed: {message}")]
    Tool { tool: String, message: String },
    #[error("agent reached the maximum of {0} turns")]
    MaximumTurns(usize),
    #[error("agent event receiver closed")]
    EventReceiverClosed,
    #[error("only the runtime may emit terminal events")]
    TerminalEventOwnedByRuntime,
    #[error("agent run failed: {0}")]
    Internal(String),
}

impl AgentError {
    pub(crate) fn failure_kind(&self) -> AgentFailureKind {
        match self {
            Self::Cancelled => AgentFailureKind::Cancelled,
            Self::Timeout => AgentFailureKind::Timeout,
            Self::Provider(_) => AgentFailureKind::Provider,
            Self::Tool { .. } => AgentFailureKind::Tool,
            Self::MaximumTurns(_) => AgentFailureKind::MaximumTurns,
            Self::EventReceiverClosed | Self::TerminalEventOwnedByRuntime | Self::Internal(_) => {
                AgentFailureKind::Internal
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ToolError {
    #[error("invalid tool input: {0}")]
    InvalidInput(String),
    #[error("tool execution failed: {0}")]
    Execution(String),
    #[error("tool operation was cancelled")]
    Cancelled,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum RegistryError {
    #[error("duplicate tool name `{0}`")]
    DuplicateName(String),
    #[error("tool name cannot be empty")]
    EmptyName,
}
