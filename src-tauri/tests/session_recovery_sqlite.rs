use std::sync::Arc;

use revue_office_lib::application::conversations::SessionApplicationService;
use revue_office_lib::application::conversations::model::{
    ConversationArtifact, ConversationMessage, NewConversation,
};
use revue_office_lib::infrastructure::persistence::sqlite::SqliteSessionRepository;

#[tokio::test]
async fn restores_messages_and_legacy_artifacts_after_reconnecting() {
    let database_path = std::env::temp_dir().join(format!(
        "revue-session-recovery-{}.db",
        uuid::Uuid::new_v4()
    ));
    let database_url = format!("sqlite://{}?mode=rwc", database_path.display());

    let repository = Arc::new(
        SqliteSessionRepository::connect(&database_url, 1)
            .await
            .expect("SQLite adapter should connect"),
    );
    let seed_pool = sqlx::SqlitePool::connect(&database_url)
        .await
        .expect("SQLite fixture pool should connect");
    sqlx::query(
        "INSERT INTO users (id, username, password_hash, created_at, updated_at) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("owner-1")
    .bind("owner")
    .bind("not-a-real-hash")
    .bind("2026-09-26T00:00:00Z")
    .bind("2026-09-26T00:00:00Z")
    .execute(&seed_pool)
    .await
    .expect("owner fixture should be inserted");
    sqlx::query(
        "INSERT INTO projects (id, title, tool_kind, owner_id, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind("project-1")
    .bind("Quarterly review")
    .bind("presentation")
    .bind("owner-1")
    .bind("2026-09-26T00:00:00Z")
    .bind("2026-09-26T00:00:00Z")
    .execute(&seed_pool)
    .await
    .expect("project fixture should be inserted");
    seed_pool.close().await;

    let service = SessionApplicationService::new(repository);
    let conversation = service
        .create(
            &revue_office_lib::application::identity::Actor::user("owner-1"),
            NewConversation {
                owner_id: "owner-1".into(),
                project_id: Some("project-1".into()),
                tool_kind: Some("presentation".into()),
                title: "Persistent conversation".into(),
            },
        )
        .await
        .expect("conversation should be created");
    service
        .append_message(
            &revue_office_lib::application::identity::Actor::user("owner-1"),
            &conversation.id,
            ConversationMessage {
                role: "user".into(),
                content: "Build a durable deck".into(),
                tool_calls: None,
                tool_call_id: None,
                created_at: "2026-09-26T00:00:01Z".into(),
            },
        )
        .await
        .expect("message should be stored");
    service
        .replace_legacy_artifacts(
            &revue_office_lib::application::identity::Actor::user("owner-1"),
            &conversation.id,
            vec![ConversationArtifact {
                id: "artifact-1".into(),
                kind: "presentation".into(),
                tool_kind: "ppt".into(),
                title: "Durable deck".into(),
                status: "completed".into(),
                content: serde_json::json!({"projectId": "project-1"}),
                version: 1,
                created_at: "2026-09-26T00:00:02Z".into(),
                updated_at: "2026-09-26T00:00:02Z".into(),
            }],
        )
        .await
        .expect("legacy artifact payload should be stored");
    let mut other = revue_office_lib::application::identity::Actor::user("other-owner");
    other.roles = vec!["admin".into()]; // Existing ownership policy is unchanged.
    assert!(matches!(
        service.detail(&other, &conversation.id).await,
        Err(revue_office_lib::application::conversations::SessionApplicationError::Forbidden)
    ));
    assert!(!service.delete(&other, &conversation.id).await.unwrap());
    assert!(!service.clear(&other, &conversation.id).await.unwrap());
    drop(service);

    let reopened = Arc::new(
        SqliteSessionRepository::connect(&database_url, 1)
            .await
            .expect("SQLite adapter should reconnect"),
    );
    let reopened_service = SessionApplicationService::new(reopened);
    let detail = reopened_service
        .detail(
            &revue_office_lib::application::identity::Actor::user("owner-1"),
            &conversation.id,
        )
        .await
        .expect("conversation should be restored");

    assert_eq!(detail.conversation.title, "Persistent conversation");
    assert_eq!(detail.messages.len(), 1);
    assert_eq!(detail.messages[0].content, "Build a durable deck");
    assert_eq!(detail.artifacts.len(), 1);
    assert_eq!(detail.artifacts[0].title, "Durable deck");

    drop(reopened_service);
    let _ = std::fs::remove_file(database_path);
}
