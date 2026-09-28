use axum::response::sse::Event;

use crate::application::event::ApplicationEvent;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseFrame {
    pub event: String,
    pub data: serde_json::Value,
}

impl SseFrame {
    pub fn into_event(self) -> Event {
        Event::default()
            .event(self.event)
            .data(self.data.to_string())
    }
}

pub fn application_event_frame(session_id: &str, event: ApplicationEvent) -> SseFrame {
    match event {
        ApplicationEvent::StateChanged { state, detail } => SseFrame {
            event: "state_update".into(),
            data: serde_json::json!({
                "phase": "running",
                "step": state,
                "detail": detail,
                "at": chrono::Utc::now().to_rfc3339(),
            }),
        },
        ApplicationEvent::ToolResult {
            tool,
            success,
            result,
        } => SseFrame {
            event: "tool_result".into(),
            data: serde_json::json!({
                "tool": tool,
                "success": success,
                "result": result,
                "error": null,
            }),
        },
        ApplicationEvent::ProjectUpdated { project } => SseFrame {
            event: "project_update".into(),
            data: project,
        },
        ApplicationEvent::SlideUpdated { slide } => SseFrame {
            event: "slide_update".into(),
            data: slide,
        },
        ApplicationEvent::ArtifactUpdated {
            artifact,
            artifacts,
        } => {
            let tool_kind = artifact.kind.clone();
            SseFrame {
                event: "artifact_update".into(),
                data: serde_json::json!({
                    "artifact": artifact,
                    "artifacts": artifacts,
                    "session_id": session_id,
                    "tool_kind": tool_kind,
                }),
            }
        }
        ApplicationEvent::Message { content } => SseFrame {
            event: "message".into(),
            data: serde_json::json!({
                "text": content,
                "session_id": session_id,
            }),
        },
        ApplicationEvent::LegacyToolProgress { event, data } => SseFrame { event, data },
        ApplicationEvent::Completed {
            summary,
            artifacts,
            new_artifacts,
        } => SseFrame {
            event: "done".into(),
            data: serde_json::json!({
                "session_id": session_id,
                "summary": summary,
                "artifacts": artifacts,
                "new_artifacts": new_artifacts,
            }),
        },
        ApplicationEvent::Failed { code, message } => SseFrame {
            event: "error".into(),
            data: serde_json::json!({"code": code, "message": message}),
        },
    }
}
