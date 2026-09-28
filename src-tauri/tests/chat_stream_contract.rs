use revue_office_lib::application::event::ApplicationEvent;
use revue_office_lib::contracts::artifact::{ArtifactPublication, ArtifactPublicationStatus};
use revue_office_lib::transport::sse::application_event_frame;

fn ready_artifact() -> ArtifactPublication {
    ArtifactPublication {
        id: "artifact-1".into(),
        session_id: "session-1".into(),
        owner_id: "owner-1".into(),
        kind: "document".into(),
        title: "Draft".into(),
        status: ArtifactPublicationStatus::Ready,
        content: serde_json::json!({"markdown": "# Draft", "file_path": "outputs/draft.json"}),
        staging_path: None,
        final_path: Some("outputs/draft.json".into()),
        error: None,
        version: 1,
        created_at: "2026-09-26T00:00:00Z".into(),
        updated_at: "2026-09-26T00:00:00Z".into(),
    }
}

#[test]
fn chat_stream_contract_preserves_public_event_names_and_key_fields() {
    let cases = vec![
        (
            ApplicationEvent::StateChanged {
                state: "thinking".into(),
                detail: serde_json::json!({"content": "working"}),
            },
            "state_update",
            "phase",
        ),
        (
            ApplicationEvent::ToolResult {
                tool: "search".into(),
                success: true,
                result: serde_json::json!({"count": 1}),
            },
            "tool_result",
            "tool",
        ),
        (
            ApplicationEvent::ProjectUpdated {
                project: serde_json::json!({"id": "project-1"}),
            },
            "project_update",
            "id",
        ),
        (
            ApplicationEvent::SlideUpdated {
                slide: serde_json::json!({"id": "slide-1"}),
            },
            "slide_update",
            "id",
        ),
        (
            ApplicationEvent::ArtifactUpdated {
                artifact: ready_artifact(),
                artifacts: vec![ready_artifact()],
            },
            "artifact_update",
            "artifact",
        ),
        (
            ApplicationEvent::Message {
                content: "answer".into(),
            },
            "message",
            "text",
        ),
        (
            ApplicationEvent::Completed {
                summary: "finished".into(),
                artifacts: vec![ready_artifact()],
                new_artifacts: vec![ready_artifact()],
            },
            "done",
            "summary",
        ),
        (
            ApplicationEvent::Failed {
                code: "runtime_model".into(),
                message: "failed".into(),
            },
            "error",
            "message",
        ),
    ];

    for (event, expected_name, required_field) in cases {
        let frame = application_event_frame("session-1", event);
        assert_eq!(frame.event, expected_name);
        assert!(
            frame.data.get(required_field).is_some(),
            "{expected_name} must contain {required_field}"
        );
        if matches!(expected_name, "message" | "done") {
            assert_eq!(frame.data["session_id"], "session-1");
        }
    }
}
