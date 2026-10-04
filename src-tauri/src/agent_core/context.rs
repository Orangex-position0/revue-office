use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;
use tokio::sync::Mutex;

use crate::providers::ChatMessage;

#[derive(Debug, Clone)]
pub struct AgentRequest {
    pub run_id: String,
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub max_turns: usize,
    pub allowed_tools: Option<Vec<String>>,
    pub temperature: Option<f64>,
    pub tool_context: ToolContext,
}

#[derive(Debug, Clone)]
pub struct ToolContext {
    pub run_id: String,
    pub metadata: Value,
    scratchpad: Arc<Mutex<HashMap<String, Value>>>,
}

impl ToolContext {
    pub fn new(run_id: impl Into<String>) -> Self {
        Self {
            run_id: run_id.into(),
            metadata: Value::Null,
            scratchpad: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn with_metadata(mut self, metadata: Value) -> Self {
        self.metadata = metadata;
        self
    }

    pub async fn insert(&self, key: impl Into<String>, value: Value) {
        self.scratchpad.lock().await.insert(key.into(), value);
    }

    pub async fn get(&self, key: &str) -> Option<Value> {
        self.scratchpad.lock().await.get(key).cloned()
    }

    pub(crate) fn shared_scratchpad(&self) -> Arc<Mutex<HashMap<String, Value>>> {
        Arc::clone(&self.scratchpad)
    }
}
