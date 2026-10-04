mod api;
mod builtin;
mod client;
pub mod credentials;
mod error;
mod registry;
mod types;

pub use builtin::{OpenAiChatConfig, openai_compatible};
pub use client::ChatProvider;
pub use error::ProviderError;
pub use registry::{ChatProviderResolver, ProviderId, ProviderRegistry, ResolvedChatProvider};
pub use types::{
    ChatMessage, ChatRequest, ChatResponse, ChatRole, ContentPart, ProviderEvent, StopReason,
    ToolCall, ToolDefinition, Usage,
};
