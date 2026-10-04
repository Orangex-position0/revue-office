use std::sync::Arc;

use revue_office_lib::application::artifacts::{
    ArtifactDraft, ArtifactPublicationRepository, ArtifactPublicationStatus, ArtifactService,
};
use revue_office_lib::infrastructure::filesystem::artifact_storage::LocalArtifactStorage;
use revue_office_lib::infrastructure::persistence::sqlite::SqliteSessionRepository;

#[tokio::test]
async fn artifact_publish_success_promotes_validated_staging_file_before_ready() {
    let root = std::env::temp_dir().join(format!("revue-artifact-files-{}", uuid::Uuid::new_v4()));
    let database_path = root.join("publication.db");
    std::fs::create_dir_all(&root).unwrap();
    let database_url = format!("sqlite://{}?mode=rwc", database_path.display());
    let repository = Arc::new(
        SqliteSessionRepository::connect(&database_url, 1)
            .await
            .expect("repository should connect"),
    );
    let service = ArtifactService::new(
        repository.clone(),
        Arc::new(LocalArtifactStorage::new(root.join("artifacts"))),
    );

    let ready = service
        .publish(ArtifactDraft {
            session_id: "session-1".into(),
            owner_id: "owner-1".into(),
            kind: "presentation".into(),
            title: "Ready presentation".into(),
            extension: "pptx".into(),
            content: serde_json::json!({"slides": 3}),
            bytes: b"valid presentation bytes".to_vec(),
        })
        .await
        .expect("artifact should publish");

    assert_eq!(ready.status, ArtifactPublicationStatus::Ready);
    assert!(ready.staging_path.is_none());
    let final_path = ready.final_path.expect("ready artifact has final path");
    assert_eq!(
        tokio::fs::read(&final_path).await.unwrap(),
        b"valid presentation bytes"
    );
    assert!(
        !root
            .join("artifacts/staging")
            .join(format!("{}.pptx", ready.id))
            .exists()
    );
    assert_eq!(ready.content["file_path"], final_path);
    assert_eq!(ready.content["file_size"], 24);
    assert!(repository.pending().await.unwrap().is_empty());

    drop(service);
    drop(repository);
    let _ = std::fs::remove_dir_all(root);
}
