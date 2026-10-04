use std::sync::Arc;
use std::time::Duration;

use crate::agent::OfficeAgent;
use crate::agent::tools::web_search::WebSearchConfig;
use crate::capabilities::presentation::PresentationCapability;
use crate::providers::ChatProviderResolver;
use crate::providers::credentials::CredentialStore;

pub(crate) fn build_office_agent(
    presentation: Arc<PresentationCapability>,
    provider_resolver: Arc<dyn ChatProviderResolver>,
    credentials: Arc<dyn CredentialStore>,
    web_search: WebSearchConfig,
    runtime_timeout: Duration,
) -> anyhow::Result<Arc<OfficeAgent>> {
    let tools = crate::agent::tools::office_tool_registry(
        presentation,
        provider_resolver.clone(),
        credentials,
        web_search,
    )?;
    Ok(Arc::new(OfficeAgent::new(
        provider_resolver,
        tools,
        256,
        runtime_timeout,
    )))
}
