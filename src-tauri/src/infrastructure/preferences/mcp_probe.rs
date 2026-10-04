use std::sync::Arc;

use async_trait::async_trait;
use eventsource_stream::Eventsource;
use futures::StreamExt;
use secrecy::ExposeSecret;
use serde_json::{Value, json};

use crate::application::identity::ActorId;
use crate::application::preferences::{McpConnectionRequest, McpConnectionTester, PreferenceError};
use crate::infrastructure::credentials::migration::PreferenceCredentialService;
use crate::infrastructure::credentials::redaction::{redact_json, split_endpoint};
use crate::providers::credentials::CredentialSet;

use super::secure_store::map_credential_error;

pub struct HttpMcpConnectionTester {
    credentials: Arc<PreferenceCredentialService>,
}

impl HttpMcpConnectionTester {
    pub fn new(credentials: Arc<PreferenceCredentialService>) -> Self {
        Self { credentials }
    }
}

#[async_trait]
impl McpConnectionTester for HttpMcpConnectionTester {
    async fn test(
        &self,
        actor: &ActorId,
        mut request: McpConnectionRequest,
    ) -> Result<Option<Vec<Value>>, PreferenceError> {
        let id = std::mem::take(&mut request.id);
        let transport = std::mem::take(&mut request.transport);
        let raw_endpoint = std::mem::take(&mut request.endpoint);
        let (endpoint, embedded) = split_endpoint(&raw_endpoint).map_err(map_credential_error)?;
        if !embedded.is_empty() {
            return Err(PreferenceError::Invalid(
                "请先将 MCP 凭据保存到安全存储，再测试不含秘密的服务地址".into(),
            ));
        }
        // Migration must succeed before any credential is resolved or request sent.
        self.credentials
            .read(&actor.0)
            .await
            .map_err(map_credential_error)?;
        let keys = self
            .credentials
            .search_credentials(&actor.0, &id, &endpoint)
            .await
            .map_err(map_credential_error)?;
        let mut url = reqwest::Url::parse(&endpoint)
            .map_err(|_| PreferenceError::Invalid("MCP 地址无效".into()))?;
        if let Some(key) = keys.as_ref().and_then(|keys| keys.values().first()) {
            url.query_pairs_mut()
                .append_pair("api_key", key.expose_secret());
        }
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(20),
            run_mcp_test(url, &transport, keys.as_ref()),
        )
        .await;
        match result {
            Ok(Ok(mut tools)) => {
                if let Some(keys) = keys {
                    for tool in &mut tools {
                        redact_json(tool, keys.values());
                    }
                }
                Ok(Some(tools))
            }
            _ => Ok(None),
        }
    }
}

async fn post_rpc(
    client: &reqwest::Client,
    url: &reqwest::Url,
    method: &str,
    id: Option<u64>,
) -> Result<reqwest::Response, ()> {
    let params = if method == "initialize" {
        json!({"protocolVersion":"2024-11-05", "capabilities":{}, "clientInfo":{"name":"revueOffice", "version":"0.2.0"}})
    } else {
        json!({})
    };
    let mut body = json!({"jsonrpc":"2.0", "method":method, "params":params});
    if let Some(id) = id {
        body["id"] = json!(id);
    }
    client
        .post(url.clone())
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .json(&body)
        .send()
        .await
        .map_err(|_| ())?
        .error_for_status()
        .map_err(|_| ())
}

async fn run_mcp_test(
    endpoint: reqwest::Url,
    transport: &str,
    _keys: Option<&CredentialSet>,
) -> Result<Vec<Value>, ()> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| ())?;
    if transport == "http" {
        post_rpc(&client, &endpoint, "initialize", Some(1)).await?;
        post_rpc(&client, &endpoint, "notifications/initialized", None).await?;
        let value: Value = post_rpc(&client, &endpoint, "tools/list", Some(2))
            .await?
            .json()
            .await
            .map_err(|_| ())?;
        return Ok(value["result"]["tools"]
            .as_array()
            .cloned()
            .unwrap_or_default());
    }
    let response = client
        .get(endpoint.clone())
        .header("Accept", "text/event-stream")
        .send()
        .await
        .map_err(|_| ())?
        .error_for_status()
        .map_err(|_| ())?;
    let mut stream = response.bytes_stream().eventsource();
    let message_endpoint = loop {
        let event = stream.next().await.ok_or(())?.map_err(|_| ())?;
        if event.event == "endpoint" && !event.data.trim().is_empty() {
            let url = endpoint.join(&event.data).map_err(|_| ())?;
            if url.origin() != endpoint.origin() {
                return Err(());
            }
            break url;
        }
    };
    post_rpc(&client, &message_endpoint, "initialize", Some(1)).await?;
    post_rpc(
        &client,
        &message_endpoint,
        "notifications/initialized",
        None,
    )
    .await?;
    post_rpc(&client, &message_endpoint, "tools/list", Some(2)).await?;
    loop {
        let event = stream.next().await.ok_or(())?.map_err(|_| ())?;
        if event.event != "message" {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(&event.data) else {
            continue;
        };
        if value["id"].as_u64() == Some(2) {
            return Ok(value["result"]["tools"]
                .as_array()
                .cloned()
                .unwrap_or_default());
        }
    }
}
