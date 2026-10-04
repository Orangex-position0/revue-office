use std::collections::HashMap;
use std::sync::Arc;

use crate::providers::ToolDefinition;

use super::{AgentTool, RegistryError};

#[derive(Clone)]
pub struct ToolRegistry {
    tools: Arc<HashMap<String, Arc<dyn AgentTool>>>,
}

impl ToolRegistry {
    pub fn new(tools: Vec<Arc<dyn AgentTool>>) -> Result<Self, RegistryError> {
        let mut by_name = HashMap::with_capacity(tools.len());
        for tool in tools {
            let name = tool.definition().name.trim().to_owned();
            if name.is_empty() {
                return Err(RegistryError::EmptyName);
            }
            if by_name.insert(name.clone(), tool).is_some() {
                return Err(RegistryError::DuplicateName(name));
            }
        }
        Ok(Self {
            tools: Arc::new(by_name),
        })
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn AgentTool>> {
        self.tools.get(name).cloned()
    }

    pub fn definitions(&self, allowed: Option<&[String]>) -> Vec<ToolDefinition> {
        let mut definitions = self
            .tools
            .values()
            .map(|tool| tool.definition())
            .filter(|definition| {
                allowed.is_none_or(|names| names.iter().any(|name| name == &definition.name))
            })
            .collect::<Vec<_>>();
        definitions.sort_by(|left, right| left.name.cmp(&right.name));
        definitions
    }
}
