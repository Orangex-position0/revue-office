use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, Response, StatusCode};
use axum::routing::post;
use axum::{Json, Router};
use revue_office_lib::capabilities::presentation::{PresentationPlanRequest, PresentationPlanner};
use revue_office_lib::infrastructure::presentation_planner::ConfiguredPresentationPlanner;
use revue_office_lib::providers::credentials::CredentialSet;
use revue_office_lib::providers::{
    ChatMessage, ChatProviderResolver, ChatRequest, ChatRole, OpenAiChatConfig, ProviderError,
    ProviderEvent, ResolvedChatProvider, StopReason, ToolDefinition, openai_compatible,
};
use secrecy::SecretString;
use serde_json::{Value, json};
fn test_credentials(key: &str) -> CredentialSet {
    CredentialSet::new([SecretString::new(key.into())])
}

#[derive(Clone)]
struct Capture {
    requests: Arc<Mutex<Vec<(String, Value)>>>,
    mode: Mode,
}

#[derive(Clone, Copy)]
enum Mode {
    Json,
    Presentation,
    Stream,
    Unauthorized,
    EchoSecret,
    StreamSecret,
    Retry,
}

async fn chat_handler(
    State(capture): State<Capture>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response<Body> {
    capture.requests.lock().unwrap().push((
        headers
            .get("authorization")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_string(),
        body,
    ));
    if matches!(capture.mode, Mode::Retry)
        && headers.get("authorization").and_then(|h| h.to_str().ok())
            == Some("Bearer synthetic-first-rotation-key")
    {
        return Response::builder()
            .status(StatusCode::UNAUTHORIZED)
            .body(Body::from(format!("Authorization: Bearer {}", sentinel())))
            .unwrap();
    }
    match capture.mode {
        Mode::Json | Mode::Retry => Response::builder()
            .header("content-type", "application/json")
            .body(Body::from(
                json!({
                    "model": "wire-model",
                    "choices": [{
                        "message": {
                            "role": "assistant",
                            "content": "hello",
                            "tool_calls": [{
                                "id": "call-1",
                                "type": "function",
                                "function": {"name": "lookup", "arguments": "{\"q\":1}"}
                            }]
                        },
                        "finish_reason": "tool_calls"
                    }],
                    "usage": {"prompt_tokens": 4, "completion_tokens": 2}
                })
                .to_string(),
            ))
            .unwrap(),
        Mode::Presentation => Response::builder()
            .header("content-type", "application/json")
            .body(Body::from(
                json!({
                    "model": "presentation-wire-model",
                    "choices": [{
                        "message": {
                            "role": "assistant",
                            "content": json!({
                                "title": "Provider-backed deck",
                                "slides": [{
                                    "title": "Overview",
                                    "layout": "title",
                                    "goal": "Introduce the topic",
                                    "visual": "cover",
                                    "points": []
                                }]
                            }).to_string()
                        },
                        "finish_reason": "stop"
                    }]
                })
                .to_string(),
            ))
            .unwrap(),
        Mode::Stream => Response::builder()
            .header("content-type", "text/event-stream")
            .body(Body::from(concat!(
                "data: {\"model\":\"wire-model\",\"choices\":[{\"delta\":{\"content\":\"hel\"},\"finish_reason\":null}]}\n\n",
                "data: {\"choices\":[{\"delta\":{\"content\":\"lo\",\"tool_calls\":[{\"index\":0,\"id\":\"call-1\",\"function\":{\"name\":\"lookup\",\"arguments\":\"{\\\"q\\\":\"}}]},\"finish_reason\":null}]}\n\n",
                "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"1}\"}}]},\"finish_reason\":\"tool_calls\"}],\"usage\":{\"prompt_tokens\":4,\"completion_tokens\":2}}\n\n",
                "data: [DONE]\n\n"
            )))
            .unwrap(),
        Mode::EchoSecret => Response::builder().header("content-type", "application/json")
            .body(Body::from(json!({"model":"wire-model", "choices":[{"message":{"role":"assistant", "content":format!("prefix {} suffix", sentinel()), "tool_calls":[{"id":"safe-id", "type":"function", "function":{"name":"safe-tool", "arguments":json!({"Authorization":format!("Bearer {}", sentinel())}).to_string()}}]}, "finish_reason":"tool_calls"}]}).to_string())).unwrap(),
        Mode::StreamSecret => {
            let key = sentinel(); let midpoint = key.len()/2;
            let chunks = [format!("prefix {}", &key[..midpoint]), format!("{} suffix", &key[midpoint..])];
            let mut body = String::new();
            for content in chunks { body.push_str(&format!("data: {}\n\n", json!({"choices":[{"delta":{"content":content}, "finish_reason":null}]}))); }
            body.push_str("data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n");
            Response::builder().header("content-type", "text/event-stream").body(Body::from(body)).unwrap()
        }
        Mode::Unauthorized => Response::builder()
            .status(StatusCode::UNAUTHORIZED)
            .body(Body::from("secret-key must never escape"))
            .unwrap(),
    }
}

async fn server(mode: Mode) -> (String, Arc<Mutex<Vec<(String, Value)>>>) {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let app = Router::new()
        .route("/chat/completions", post(chat_handler))
        .with_state(Capture {
            requests: requests.clone(),
            mode,
        });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (format!("http://{address}"), requests)
}

struct FixedResolver {
    resolved: ResolvedChatProvider,
}

#[async_trait]
impl ChatProviderResolver for FixedResolver {
    async fn resolve(
        &self,
        _user_id: &str,
        _preferred_model: Option<&str>,
    ) -> Result<ResolvedChatProvider, ProviderError> {
        Ok(self.resolved.clone())
    }
}

fn sentinel() -> &'static str {
    "synthetic-Q7rZ2mV9pK5xN8cT4wL6bH3jF0dS1aE"
}

fn request() -> ChatRequest {
    ChatRequest {
        model: "requested-model".into(),
        messages: vec![ChatMessage::text(ChatRole::User, "hello")],
        tools: vec![ToolDefinition {
            name: "lookup".into(),
            description: "Look up a value".into(),
            parameters: json!({"type": "object"}),
        }],
        temperature: Some(0.2),
    }
}

#[tokio::test]
async fn non_streaming_maps_openai_wire_data_to_neutral_contract() {
    let (endpoint, requests) = server(Mode::Json).await;
    let provider = openai_compatible(OpenAiChatConfig {
        endpoint,
        credentials: test_credentials("test-key"),
        timeout: Duration::from_secs(2),
    })
    .unwrap();

    let response = provider.chat(request()).await.unwrap();

    assert_eq!(response.message.text_content(), "hello");
    assert_eq!(response.message.tool_calls[0].name, "lookup");
    assert_eq!(response.stop_reason, StopReason::ToolCalls);
    assert_eq!(response.usage.unwrap().input_tokens, 4);
    let captured = requests.lock().unwrap();
    assert!(
        captured[0].0 == "Bearer test-key",
        "outbound authorization mismatch"
    );
    assert_eq!(captured[0].1["model"], "requested-model");
    assert_eq!(captured[0].1["tools"][0]["type"], "function");
}

#[tokio::test]
async fn chat_and_presentation_use_the_same_contract_with_distinct_configurations() {
    let (chat_endpoint, chat_requests) = server(Mode::Json).await;
    let chat_provider = openai_compatible(OpenAiChatConfig {
        endpoint: chat_endpoint,
        credentials: test_credentials("chat-key"),
        timeout: Duration::from_secs(2),
    })
    .unwrap();
    let mut chat_request = request();
    chat_request.model = "chat-model".into();
    chat_provider.chat(chat_request).await.unwrap();

    let (presentation_endpoint, presentation_requests) = server(Mode::Presentation).await;
    let presentation_provider = openai_compatible(OpenAiChatConfig {
        endpoint: presentation_endpoint,
        credentials: test_credentials("presentation-key"),
        timeout: Duration::from_secs(2),
    })
    .unwrap();
    let presentation = ConfiguredPresentationPlanner::new(Arc::new(FixedResolver {
        resolved: ResolvedChatProvider {
            provider: presentation_provider,
            model: "presentation-model".into(),
        },
    }));

    let plan = presentation
        .plan(PresentationPlanRequest {
            owner_id: "owner-1".into(),
            topic: "Provider boundaries".into(),
            audience: Some("Engineers".into()),
            preferred_model: Some("presentation-model".into()),
        })
        .await
        .unwrap();

    assert_eq!(plan.title, "Provider-backed deck");
    assert_eq!(plan.slides.len(), 1);
    let chat_capture = chat_requests.lock().unwrap();
    assert!(
        chat_capture[0].0 == "Bearer chat-key",
        "outbound authorization mismatch"
    );
    assert_eq!(chat_capture[0].1["model"], "chat-model");
    let presentation_capture = presentation_requests.lock().unwrap();
    assert!(
        presentation_capture[0].0 == "Bearer presentation-key",
        "outbound authorization mismatch"
    );
    assert_eq!(presentation_capture[0].1["model"], "presentation-model");
    assert_eq!(presentation_capture[0].1["stream"], false);
    assert!(presentation_capture[0].1.get("tools").is_none());
}

#[tokio::test]
async fn streaming_emits_deltas_usage_and_exactly_one_finished_event() {
    let (endpoint, _) = server(Mode::Stream).await;
    let provider = openai_compatible(OpenAiChatConfig {
        endpoint,
        credentials: test_credentials("test-key"),
        timeout: Duration::from_secs(2),
    })
    .unwrap();
    let (sender, mut receiver) = tokio::sync::mpsc::channel(128);

    let response = provider.stream_chat(request(), sender).await.unwrap();
    let mut events = Vec::new();
    while let Some(event) = receiver.recv().await {
        events.push(event);
    }

    assert_eq!(response.message.text_content(), "hello");
    assert_eq!(response.message.tool_calls[0].arguments, "{\"q\":1}");
    assert!(
        events
            .iter()
            .any(|event| matches!(event, ProviderEvent::Usage(_)))
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, ProviderEvent::Finished(_)))
            .count(),
        1
    );
}

#[tokio::test]
async fn errors_and_debug_output_do_not_expose_credentials_or_response_bodies() {
    let (endpoint, _) = server(Mode::Unauthorized).await;
    let config = OpenAiChatConfig {
        endpoint,
        credentials: test_credentials("test-key"),
        timeout: Duration::from_secs(2),
    };
    assert!(!format!("{config:?}").contains("test-key"));
    let provider = openai_compatible(config).unwrap();

    let error = provider.chat(request()).await.unwrap_err();

    assert!(matches!(error, ProviderError::Authentication));
    let rendered = format!("{error:?} {error}");
    assert!(!rendered.contains("test-key"));
    assert!(!rendered.contains("secret-key"));
}

#[tokio::test]
async fn provider_redacts_echoed_credentials_and_split_sse_chunks_before_events() {
    let (endpoint, _) = server(Mode::EchoSecret).await;
    let provider = openai_compatible(OpenAiChatConfig {
        endpoint,
        credentials: test_credentials(sentinel()),
        timeout: Duration::from_secs(2),
    })
    .unwrap();
    let response = provider.chat(request()).await.unwrap();
    assert!(!response.message.text_content().contains(sentinel()));
    assert!(!format!("{response:?}").contains(sentinel()));
    let (endpoint, _) = server(Mode::StreamSecret).await;
    let provider = openai_compatible(OpenAiChatConfig {
        endpoint,
        credentials: test_credentials(sentinel()),
        timeout: Duration::from_secs(2),
    })
    .unwrap();
    let (sender, mut receiver) = tokio::sync::mpsc::channel(128);
    let response = provider.stream_chat(request(), sender).await.unwrap();
    let mut concatenated = String::new();
    let mut finished = 0;
    while let Some(event) = receiver.recv().await {
        assert!(!format!("{event:?}").contains(sentinel()));
        match event {
            ProviderEvent::TextDelta(text) => concatenated.push_str(&text),
            ProviderEvent::Finished(_) => finished += 1,
            _ => {}
        }
    }
    assert!(!concatenated.contains(sentinel()));
    assert!(concatenated.contains("[REDACTED]"));
    assert!(concatenated == response.message.text_content());
    assert!(finished == 1);
}

#[tokio::test]
async fn provider_rotates_secure_key_set_and_retries_auth_failures_without_body_exposure() {
    let (endpoint, requests) = server(Mode::Retry).await;
    let keys = CredentialSet::new([
        SecretString::new("synthetic-first-rotation-key".into()),
        SecretString::new("synthetic-second-rotation-key".into()),
    ]);
    let provider = openai_compatible(OpenAiChatConfig {
        endpoint,
        credentials: keys,
        timeout: Duration::from_secs(2),
    })
    .unwrap();
    let response = provider.chat(request()).await.unwrap();
    assert!(!format!("{response:?}").contains(sentinel()));
    provider.chat(request()).await.unwrap();
    let capture = requests.lock().unwrap();
    assert!(capture.len() == 3);
    assert!(
        capture[0].0 == "Bearer synthetic-first-rotation-key",
        "unexpected outbound rotation order"
    );
    assert!(
        capture[1].0 == "Bearer synthetic-second-rotation-key",
        "unexpected retry authorization"
    );
    assert!(
        capture[2].0 == "Bearer synthetic-second-rotation-key",
        "unexpected next-request rotation"
    );
}

#[test]
fn provider_rejects_credential_bearing_endpoint_without_echoing_it() {
    let config = OpenAiChatConfig {
        endpoint: format!(
            "https://user:{}@example.invalid/v1?api_key={}",
            sentinel(),
            sentinel()
        ),
        credentials: test_credentials(sentinel()),
        timeout: Duration::from_secs(2),
    };
    assert!(!format!("{config:?}").contains(sentinel()));
    assert!(matches!(
        openai_compatible(config),
        Err(ProviderError::InvalidConfiguration)
    ));
}
