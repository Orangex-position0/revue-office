use std::sync::Arc;

use revue_office_lib::application::session_service::SessionApplicationService;
use revue_office_lib::contracts::conversation::{
    ConversationArtifact, ConversationMessage, ConversationUpdate, NewConversation,
};
use revue_office_lib::infrastructure::persistence::mysql::MySqlSessionRepository;

fn isolated_database_url() -> String {
    assert_eq!(
        std::env::var("REVUE_ALLOW_MYSQL_TEST").as_deref(),
        Ok("1"),
        "MySQL integration tests require REVUE_ALLOW_MYSQL_TEST=1"
    );
    let url = std::env::var("MYSQL_TEST_DATABASE_URL")
        .expect("MYSQL_TEST_DATABASE_URL must be configured");
    let database_name = url
        .split('?')
        .next()
        .and_then(|value| value.rsplit('/').next())
        .unwrap_or_default();
    assert!(
        url.starts_with("mysql://")
            && (database_name.starts_with("test_") || database_name.ends_with("_test")),
        "MySQL integration tests require an isolated test_ or _test database"
    );
    url
}

#[tokio::test]
#[ignore = "requires an explicitly isolated MySQL test database"]
async fn mysql_session_repository_matches_the_sqlite_application_contract() {
    let database_url = isolated_database_url();
    let repository = Arc::new(
        MySqlSessionRepository::connect(&database_url, 2)
            .await
            .expect("MySQL adapter should connect"),
    );

    let fixture_pool = sqlx::MySqlPool::connect(&database_url)
        .await
        .expect("fixture pool should connect");
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let owner_id = uuid::Uuid::new_v4().to_string();
    let username = format!("owner-{suffix}");
    sqlx::query(
        "INSERT INTO users (id, username, password_hash, created_at, updated_at) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&owner_id)
    .bind(&username)
    .bind("not-a-real-hash")
    .bind("2026-09-26T00:00:00Z")
    .bind("2026-09-26T00:00:00Z")
    .execute(&fixture_pool)
    .await
    .expect("owner fixture should be inserted");
    fixture_pool.close().await;

    let service = SessionApplicationService::new(repository);
    let conversation = service
        .create(NewConversation {
            owner_id: owner_id.clone(),
            project_id: None,
            tool_kind: Some("general".into()),
            title: "MySQL conversation".into(),
        })
        .await
        .expect("conversation should be created");
    service
        .append_message(
            &owner_id,
            &conversation.id,
            ConversationMessage {
                role: "user".into(),
                content: "Persist with native text types".into(),
                tool_calls: None,
                tool_call_id: None,
                created_at: "2026-09-26T00:00:01Z".into(),
            },
        )
        .await
        .expect("message should be appended");
    service
        .replace_legacy_artifacts(
            &owner_id,
            &conversation.id,
            vec![ConversationArtifact {
                id: "artifact-1".into(),
                kind: "presentation".into(),
                tool_kind: "ppt".into(),
                title: "Existing artifact".into(),
                status: "ready".into(),
                content: serde_json::json!({"slides": 3}),
                version: 1,
                created_at: "2026-09-26T00:00:02Z".into(),
                updated_at: "2026-09-26T00:00:02Z".into(),
            }],
        )
        .await
        .expect("legacy artifacts should be persisted");
    service
        .update(
            &owner_id,
            &conversation.id,
            ConversationUpdate {
                title: Some("Renamed MySQL conversation".into()),
                project_id: None,
                order: Some(42),
            },
        )
        .await
        .expect("conversation should be updated");
    drop(service);

    let reopened = Arc::new(
        MySqlSessionRepository::connect(&database_url, 2)
            .await
            .expect("MySQL adapter should reconnect without clearing data"),
    );
    let reopened_service = SessionApplicationService::new(reopened);
    let detail = reopened_service
        .detail(&owner_id, &conversation.id)
        .await
        .expect("conversation should survive restart");

    assert_eq!(detail.conversation.title, "Renamed MySQL conversation");
    assert_eq!(detail.conversation.order, 42);
    assert_eq!(detail.messages.len(), 1);
    assert_eq!(detail.messages[0].content, "Persist with native text types");
    assert_eq!(detail.artifacts.len(), 1);
    assert_eq!(detail.artifacts[0].title, "Existing artifact");

    let listed = reopened_service
        .list(&owner_id, 20, Some("Renamed"))
        .await
        .expect("conversation should be listed");
    assert_eq!(listed.len(), 1);
    assert!(reopened_service
        .clear(&owner_id, &conversation.id)
        .await
        .expect("messages should be cleared"));
    assert!(reopened_service
        .messages(&owner_id, &conversation.id, 100)
        .await
        .expect("history should remain readable")
        .is_empty());
    assert!(reopened_service
        .delete(&owner_id, &conversation.id)
        .await
        .expect("conversation should be deleted"));
    assert!(reopened_service
        .list(&owner_id, 20, None)
        .await
        .expect("empty list should remain readable")
        .is_empty());
}
