use std::sync::Arc;

use revue_office_lib::contracts::artifact::{
    ArtifactFinalization, ArtifactPublicationStatus, NewArtifactPublication,
};
use revue_office_lib::infrastructure::persistence::mysql::MySqlSessionRepository;
use revue_office_lib::infrastructure::persistence::sqlite::SqliteSessionRepository;
use revue_office_lib::ports::repositories::artifact_publication::ArtifactPublicationRepository;

async fn exercise_repository(repository: Arc<dyn ArtifactPublicationRepository>) {
    let ready_candidate = repository
        .reserve(NewArtifactPublication {
            session_id: "session-1".into(),
            owner_id: "owner-1".into(),
            kind: "presentation".into(),
            title: "Quarterly review".into(),
            content: serde_json::json!({"slides": 3}),
            staging_path: "staging/artifact-1.pptx".into(),
        })
        .await
        .expect("publication should be reserved");
    assert_eq!(
        ready_candidate.status,
        ArtifactPublicationStatus::Publishing
    );
    assert_eq!(repository.pending().await.unwrap().len(), 1);

    let ready = repository
        .finalize(
            &ready_candidate.id,
            ArtifactFinalization {
                final_path: "outputs/artifact-1.pptx".into(),
                content: serde_json::json!({"slides": 3, "validated": true}),
            },
        )
        .await
        .expect("publication should finalize")
        .expect("publishing record should exist");
    assert_eq!(ready.status, ArtifactPublicationStatus::Ready);
    assert_eq!(ready.final_path.as_deref(), Some("outputs/artifact-1.pptx"));
    assert!(ready.staging_path.is_none());

    let failed_candidate = repository
        .reserve(NewArtifactPublication {
            session_id: "session-1".into(),
            owner_id: "owner-1".into(),
            kind: "document".into(),
            title: "Broken draft".into(),
            content: serde_json::json!({}),
            staging_path: "staging/artifact-2.docx".into(),
        })
        .await
        .expect("second publication should be reserved");
    assert!(repository
        .fail(&failed_candidate.id, "render failed")
        .await
        .expect("publication should fail"));
    let failed = repository
        .find(&failed_candidate.id)
        .await
        .unwrap()
        .expect("failed record should remain diagnosable");
    assert_eq!(failed.status, ArtifactPublicationStatus::Failed);
    assert_eq!(failed.error.as_deref(), Some("render failed"));
    assert!(repository.pending().await.unwrap().is_empty());
}

#[tokio::test]
async fn artifact_publication_repository_tracks_sqlite_state_transitions() {
    let database_path = std::env::temp_dir().join(format!(
        "revue-artifact-publication-{}.db",
        uuid::Uuid::new_v4()
    ));
    let database_url = format!("sqlite://{}?mode=rwc", database_path.display());
    let repository = Arc::new(
        SqliteSessionRepository::connect(&database_url, 1)
            .await
            .expect("SQLite repository should connect"),
    );
    exercise_repository(repository).await;
    let _ = std::fs::remove_file(database_path);
}

#[tokio::test]
#[ignore = "requires an explicitly isolated MySQL test database"]
async fn artifact_publication_repository_tracks_mysql_state_transitions() {
    assert_eq!(
        std::env::var("REVUE_ALLOW_MYSQL_TEST").as_deref(),
        Ok("1"),
        "MySQL integration tests require REVUE_ALLOW_MYSQL_TEST=1"
    );
    let database_url = std::env::var("MYSQL_TEST_DATABASE_URL")
        .expect("MYSQL_TEST_DATABASE_URL must be configured");
    let database_name = database_url
        .split('?')
        .next()
        .and_then(|value| value.rsplit('/').next())
        .unwrap_or_default();
    assert!(
        database_url.starts_with("mysql://")
            && (database_name.starts_with("test_") || database_name.ends_with("_test")),
        "MySQL integration tests require an isolated test database"
    );
    let repository = Arc::new(
        MySqlSessionRepository::connect(&database_url, 2)
            .await
            .expect("MySQL repository should connect"),
    );
    exercise_repository(repository).await;
}
