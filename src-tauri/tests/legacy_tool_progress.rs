use revue_office_lib::agent::tool::LegacyToolProgressAdapter;

#[tokio::test]
async fn legacy_tool_progress_preserves_named_events_without_silent_drops() {
    let (adapter, mut receiver) = LegacyToolProgressAdapter::bounded(1);
    let emit = adapter.callback();

    emit("state_update", serde_json::json!({"step": "searching"}));
    emit(
        "project_update",
        serde_json::json!({"project_id": "project-1"}),
    );

    let first = receiver.recv().await.expect("first event should arrive");
    let second = receiver
        .recv()
        .await
        .expect("backpressured event should arrive");
    assert_eq!(first.event, "state_update");
    assert_eq!(first.data["step"], "searching");
    assert_eq!(second.event, "project_update");
    assert_eq!(second.data["project_id"], "project-1");
}
