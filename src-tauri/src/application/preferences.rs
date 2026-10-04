mod error;
mod model;
mod ports;
mod service;

pub use error::PreferenceError;
pub use model::{
    AppSettings, BasicSettings, LlmProfileConfig, McpConnectionRequest, McpConnectionResult,
    McpServerConfig, PreferenceDefaults, PreferenceUpdate,
};
pub use ports::{McpConnectionTester, SecurePreferencePort};
pub use service::PreferenceApplicationService;
