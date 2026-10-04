use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub struct GeneratedFile {
    pub extension: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GeneratedOutput {
    pub kind: String,
    pub title: String,
    pub content: Value,
    pub file: Option<GeneratedFile>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolOutput {
    pub observation: String,
    pub data: Value,
    pub outputs: Vec<GeneratedOutput>,
}

impl ToolOutput {
    pub fn new(observation: impl Into<String>) -> Self {
        Self {
            observation: observation.into(),
            data: Value::Null,
            outputs: Vec::new(),
        }
    }

    pub fn with_data(mut self, data: Value) -> Self {
        self.data = data;
        self
    }

    pub fn with_outputs(mut self, outputs: Vec<GeneratedOutput>) -> Self {
        self.outputs = outputs;
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentCompletion {
    pub summary: String,
    pub outputs: Vec<GeneratedOutput>,
}
