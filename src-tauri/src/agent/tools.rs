pub mod agnes_media;
pub mod chart_generate;
pub mod doc_generate;
pub mod drawio_generate;
pub mod image_prompt;
pub mod local_video;
pub mod md_generate;
pub mod ppt_generate;
pub mod ppt_plan;
pub mod sheet_generate;
pub mod video_generate;
pub mod web_search;

use super::registry::REGISTRY;
use std::sync::Arc;

pub async fn register_all_tools() {
    REGISTRY.register(Arc::new(ppt_plan::PptPlanTool)).await;
    REGISTRY
        .register(Arc::new(ppt_generate::PptGenerateTool))
        .await;
    REGISTRY
        .register(Arc::new(doc_generate::DocGenerateTool))
        .await;
    REGISTRY
        .register(Arc::new(md_generate::MarkdownGenerateTool))
        .await;
    REGISTRY
        .register(Arc::new(sheet_generate::SheetGenerateTool))
        .await;
    REGISTRY
        .register(Arc::new(chart_generate::ChartGenerateTool))
        .await;
    REGISTRY
        .register(Arc::new(drawio_generate::DrawioGenerateTool))
        .await;
    REGISTRY
        .register(Arc::new(image_prompt::ImagePromptTool))
        .await;
    REGISTRY
        .register(Arc::new(video_generate::VideoGenerateTool))
        .await;
    REGISTRY.register(Arc::new(web_search::WebSearchTool)).await;

    let tools = REGISTRY.list().await;
    tracing::info!(
        "[AgentTools] 已注册 {} 个工具: {}",
        tools.len(),
        tools
            .iter()
            .map(|t| t.name())
            .collect::<Vec<_>>()
            .join(", ")
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::tool::OfficeTool;

    #[test]
    fn parameterless_tools_publish_object_json_schemas() {
        let schemas = [
            image_prompt::ImagePromptTool.parameters(),
            video_generate::VideoGenerateTool.parameters(),
            agnes_media::AgnesMediaTool.parameters(),
        ];

        for schema in schemas {
            assert_eq!(
                schema.get("type").and_then(|value| value.as_str()),
                Some("object")
            );
            assert!(schema
                .get("properties")
                .is_some_and(|value| value.is_object()));
            assert_eq!(
                schema
                    .get("additionalProperties")
                    .and_then(|value| value.as_bool()),
                Some(false)
            );
        }
    }
}
