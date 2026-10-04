pub mod agnes_media;
pub mod chart_generate;
mod chat_provider;
pub mod doc_generate;
pub mod drawio_generate;
pub mod image_prompt;
pub mod local_video;
pub mod md_generate;
pub mod presentation;
pub mod sheet_generate;
pub mod video_generate;
pub mod web_search;

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Value, json};
use tokio::sync::mpsc;

use super::profile::OfficeToolMetadata;
use super::tool::{OfficeTool, ToolAttachment, ToolContext as LegacyToolContext};
use crate::agent_core::{
    AgentTool, GeneratedFile, GeneratedOutput, RegistryError, ToolContext, ToolError,
    ToolEventSink, ToolOutput, ToolRegistry,
};
use crate::capabilities::presentation::PresentationCapability;
use crate::providers::ToolDefinition;

pub fn office_tool_registry(
    presentation: Arc<PresentationCapability>,
    provider_resolver: Arc<dyn crate::providers::ChatProviderResolver>,
    credentials: Arc<dyn crate::providers::credentials::CredentialStore>,
    web_search: web_search::WebSearchConfig,
) -> Result<ToolRegistry, RegistryError> {
    let legacy_tools: Vec<Arc<dyn OfficeTool>> = vec![
        Arc::new(doc_generate::DocGenerateTool),
        Arc::new(md_generate::MarkdownGenerateTool),
        Arc::new(sheet_generate::SheetGenerateTool),
        Arc::new(chart_generate::ChartGenerateTool),
        Arc::new(drawio_generate::DrawioGenerateTool),
        Arc::new(image_prompt::ImagePromptTool),
        Arc::new(video_generate::VideoGenerateTool),
        Arc::new(web_search::WebSearchTool::new(credentials, web_search)),
    ];
    let mut tools: Vec<Arc<dyn AgentTool>> = vec![
        Arc::new(presentation::PresentationPlanTool::new(Arc::clone(
            &presentation,
        ))),
        Arc::new(presentation::PresentationGenerateTool::new(presentation)),
    ];
    tools.extend(legacy_tools.into_iter().map(|tool| {
        Arc::new(OfficeToolAdapter {
            tool,
            provider_resolver: provider_resolver.clone(),
        }) as Arc<dyn AgentTool>
    }));
    let names = tools
        .iter()
        .map(|tool| tool.definition().name)
        .collect::<Vec<_>>();
    tracing::info!(
        "[AgentTools] constructed {} instance-scoped tools: {}",
        names.len(),
        names.join(", ")
    );
    ToolRegistry::new(tools)
}

struct OfficeToolAdapter {
    tool: Arc<dyn OfficeTool>,
    provider_resolver: Arc<dyn crate::providers::ChatProviderResolver>,
}

#[derive(Debug)]
struct LegacyProgress {
    event: String,
    data: Value,
}

#[async_trait]
impl AgentTool for OfficeToolAdapter {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.tool.name().to_owned(),
            description: self.tool.description().to_owned(),
            parameters: self.tool.parameters(),
        }
    }

    async fn call(
        &self,
        input: Value,
        context: &ToolContext,
        events: ToolEventSink,
    ) -> Result<ToolOutput, ToolError> {
        let metadata: OfficeToolMetadata = serde_json::from_value(context.metadata.clone())
            .map_err(|error| {
                ToolError::Execution(format!("invalid Office tool context: {error}"))
            })?;
        let (progress_sender, mut progress_receiver) = mpsc::unbounded_channel::<LegacyProgress>();
        let callback_sender = progress_sender.clone();
        let progress_task = tokio::spawn(async move {
            while let Some(progress) = progress_receiver.recv().await {
                if events
                    .progress(
                        "legacy",
                        json!({"event": progress.event, "data": progress.data}),
                    )
                    .await
                    .is_err()
                {
                    break;
                }
            }
        });

        let attachments = metadata
            .attachments
            .into_iter()
            .map(|attachment| ToolAttachment {
                id: attachment.id,
                name: attachment.name,
                kind: attachment.kind,
                mime_type: attachment.mime_type,
                size: attachment.size,
                text_content: attachment.text_content,
                data_url: attachment.data_url,
            })
            .collect();
        let emit = move |event: &str, data: Value| {
            let _ = callback_sender.send(LegacyProgress {
                event: event.to_owned(),
                data,
            });
        };
        let mut legacy_context = LegacyToolContext::new(
            metadata.session_id,
            metadata.user_id,
            metadata.project_id,
            metadata.preferred_model,
            attachments,
            emit,
        )
        .with_scratchpad(context.shared_scratchpad())
        .with_provider_resolver(self.provider_resolver.clone());
        if let Some(config) = metadata.tool_config {
            legacy_context = legacy_context.with_tool_config(config);
        }

        let result = self.tool.call(input, &legacy_context).await;
        drop(legacy_context);
        drop(progress_sender);
        let _ = progress_task.await;
        if !result.success {
            return Err(ToolError::Execution(
                result.error.unwrap_or(result.observation),
            ));
        }

        let outputs = result
            .artifacts
            .unwrap_or_default()
            .into_iter()
            .map(|artifact| GeneratedOutput {
                kind: artifact.kind,
                title: artifact.title,
                content: artifact.content,
                file: Some(GeneratedFile {
                    extension: artifact.extension,
                    bytes: artifact.bytes,
                }),
            })
            .collect();
        Ok(ToolOutput::new(result.observation)
            .with_data(result.data.unwrap_or(Value::Null))
            .with_outputs(outputs))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parameterless_tools_publish_object_json_schemas() {
        let schemas = [
            image_prompt::ImagePromptTool.parameters(),
            video_generate::VideoGenerateTool.parameters(),
            agnes_media::AgnesMediaTool.parameters(),
        ];

        for schema in schemas {
            assert_eq!(schema.get("type").and_then(Value::as_str), Some("object"));
            assert!(schema.get("properties").is_some_and(Value::is_object));
            assert_eq!(
                schema.get("additionalProperties").and_then(Value::as_bool),
                Some(false)
            );
        }
    }
}
