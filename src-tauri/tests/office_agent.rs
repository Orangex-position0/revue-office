use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use revue_office_lib::agent::{
    AgentRunner, OfficeAgent, OfficeAgentEvent, OfficeAgentRequest, OfficeMessage,
};
use revue_office_lib::agent_core::{
    AgentTool, ToolContext, ToolError, ToolEventSink, ToolOutput, ToolRegistry,
};
use revue_office_lib::providers::{
    ChatMessage, ChatProvider, ChatProviderResolver, ChatRequest, ChatResponse, ChatRole,
    ProviderError, ProviderEvent, ResolvedChatProvider, StopReason, ToolCall, ToolDefinition,
};
use serde_json::{Value, json};
use tokio::sync::mpsc;

struct FakeResolver {
    provider: Arc<dyn ChatProvider>,
}

#[async_trait]
impl ChatProviderResolver for FakeResolver {
    async fn resolve(
        &self,
        _user_id: &str,
        _preferred_model: Option<&str>,
    ) -> Result<ResolvedChatProvider, ProviderError> {
        Ok(ResolvedChatProvider {
            provider: Arc::clone(&self.provider),
            model: "office-model".into(),
        })
    }
}

struct QueueProvider {
    requests: Arc<Mutex<Vec<ChatRequest>>>,
    responses: Mutex<Vec<ChatResponse>>,
}

#[async_trait]
impl ChatProvider for QueueProvider {
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, ProviderError> {
        self.requests.lock().unwrap().push(request);
        Ok(self.responses.lock().unwrap().remove(0))
    }

    async fn stream_chat(
        &self,
        request: ChatRequest,
        _events: mpsc::Sender<ProviderEvent>,
    ) -> Result<ChatResponse, ProviderError> {
        self.chat(request).await
    }
}

struct OfficeTool;

#[async_trait]
impl AgentTool for OfficeTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "office_echo".into(),
            description: "echo Office input".into(),
            parameters: json!({"type": "object"}),
        }
    }

    async fn call(
        &self,
        input: Value,
        _context: &ToolContext,
        events: ToolEventSink,
    ) -> Result<ToolOutput, ToolError> {
        events
            .progress("working", json!({"percent": 50}))
            .await
            .map_err(|error| ToolError::Execution(error.to_string()))?;
        Ok(ToolOutput::new("echoed").with_data(input))
    }
}

fn tool_response() -> ChatResponse {
    ChatResponse {
        message: ChatMessage {
            role: ChatRole::Assistant,
            content: Vec::new(),
            tool_calls: vec![ToolCall {
                id: "call-1".into(),
                name: "office_echo".into(),
                arguments: r#"{"value":42}"#.into(),
            }],
            tool_call_id: None,
        },
        model: "office-model".into(),
        stop_reason: StopReason::ToolCalls,
        usage: None,
    }
}

#[tokio::test]
async fn office_agent_resolves_profile_builds_prompt_and_runs_instance_registry() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let provider = Arc::new(QueueProvider {
        requests: Arc::clone(&requests),
        responses: Mutex::new(vec![
            tool_response(),
            ChatResponse {
                message: ChatMessage::text(ChatRole::Assistant, "Office task finished"),
                model: "office-model".into(),
                stop_reason: StopReason::EndTurn,
                usage: None,
            },
        ]),
    });
    let registry = ToolRegistry::new(vec![Arc::new(OfficeTool)]).unwrap();
    let agent = OfficeAgent::new(
        Arc::new(FakeResolver { provider }),
        registry,
        8,
        Duration::from_secs(1),
    );

    let mut run = agent.start(OfficeAgentRequest {
        run_id: "run-1".into(),
        session_id: "session-1".into(),
        user_id: "owner-1".into(),
        project_id: None,
        preferred_model: Some("preferred".into()),
        attachments: Vec::new(),
        tool_config: None,
        allowed_tools: None,
        history: vec![OfficeMessage {
            role: "assistant".into(),
            content: "Earlier answer".into(),
        }],
        user_message: "Create a report".into(),
        max_turns: 2,
    });

    let mut events = Vec::new();
    while let Some(event) = run.events.recv().await {
        let terminal = event.is_terminal();
        events.push(event);
        if terminal {
            break;
        }
    }
    assert!(matches!(events[0], OfficeAgentEvent::ToolStarted { .. }));
    assert!(matches!(events[1], OfficeAgentEvent::ToolProgress { .. }));
    assert!(matches!(events[2], OfficeAgentEvent::ToolFinished { .. }));
    assert!(matches!(
        events.last(),
        Some(OfficeAgentEvent::Completed { .. })
    ));
    assert_eq!(events.iter().filter(|event| event.is_terminal()).count(), 1);

    let captured = requests.lock().unwrap();
    assert_eq!(captured[0].model, "office-model");
    assert_eq!(captured[0].messages[0].role, ChatRole::System);
    assert!(
        captured[0].messages[0]
            .text_content()
            .contains("智能办公 Agent")
    );
    assert_eq!(captured[0].tools[0].name, "office_echo");
}
