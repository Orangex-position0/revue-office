use std::sync::Arc;

use async_trait::async_trait;
use revue_office_lib::application::artifacts::{
    ArtifactDraft, ArtifactPublicationRepository, ArtifactPublicationStatus, ArtifactService,
};
use revue_office_lib::application::conversations::SessionRepository;
use revue_office_lib::application::conversations::model::NewConversation;
use revue_office_lib::application::event::ApplicationEvent;
use revue_office_lib::capabilities::presentation::{
    PresentationCapability, PresentationGenerateRequest, PresentationPlan, PresentationPlanRequest,
    PresentationPlanner, PresentationPlannerError, PresentationProgress, PresentationProgressError,
    PresentationProgressSink, PresentationProject, PresentationSlidePlan, PresentationStore,
    PresentationStoreError,
};
use revue_office_lib::infrastructure::export::PptxPresentationExporter;
use revue_office_lib::infrastructure::filesystem::artifact_storage::LocalArtifactStorage;
use revue_office_lib::infrastructure::persistence::sqlite::SqliteSessionRepository;
use revue_office_lib::transport::sse::application_event_frame;
use tokio::sync::Mutex as AsyncMutex;

struct FakePlanner;

#[async_trait]
impl PresentationPlanner for FakePlanner {
    async fn plan(
        &self,
        request: PresentationPlanRequest,
    ) -> Result<PresentationPlan, PresentationPlannerError> {
        Ok(PresentationPlan {
            title: request.topic,
            slides: vec![
                PresentationSlidePlan {
                    title: "Launch".into(),
                    layout: Some("title".into()),
                    goal: Some("Introduce launch".into()),
                    points: vec![],
                    visual: Some("cover".into()),
                },
                PresentationSlidePlan {
                    title: "Milestones".into(),
                    layout: Some("content".into()),
                    goal: Some("Show delivery".into()),
                    points: vec!["Alpha".into(), "General availability".into()],
                    visual: Some("timeline".into()),
                },
            ],
        })
    }
}

#[derive(Default)]
struct FakeStore(AsyncMutex<Option<PresentationProject>>);

#[async_trait]
impl PresentationStore for FakeStore {
    async fn save(&self, project: &PresentationProject) -> Result<(), PresentationStoreError> {
        *self.0.lock().await = Some(project.clone());
        Ok(())
    }

    async fn load(
        &self,
        project_id: &str,
    ) -> Result<Option<PresentationProject>, PresentationStoreError> {
        Ok(self
            .0
            .lock()
            .await
            .clone()
            .filter(|project| project.id == project_id))
    }
}

#[derive(Default)]
struct CaptureProgress(AsyncMutex<Vec<PresentationProgress>>);

#[async_trait]
impl PresentationProgressSink for CaptureProgress {
    async fn emit(&self, progress: PresentationProgress) -> Result<(), PresentationProgressError> {
        self.0.lock().await.push(progress);
        Ok(())
    }
}

#[tokio::test]
async fn presentation_flow_emits_compatible_progress_and_publishes_before_done() {
    let root =
        std::env::temp_dir().join(format!("revue-presentation-flow-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let progress = CaptureProgress::default();
    let store = Arc::new(FakeStore::default());
    let capability = PresentationCapability::new(
        Arc::new(FakePlanner),
        store.clone(),
        Arc::new(PptxPresentationExporter),
    );
    let output = capability
        .generate(
            PresentationGenerateRequest {
                owner_id: "owner-1".into(),
                title: "Launch".into(),
                topic: "Launch".into(),
                theme: "business".into(),
                preferred_model: None,
                plan: None,
            },
            &progress,
        )
        .await
        .unwrap();
    let project = output.project;

    let database_url = format!("sqlite://{}?mode=rwc", root.join("flow.db").display());
    let repository = Arc::new(
        SqliteSessionRepository::connect(&database_url, 1)
            .await
            .unwrap(),
    );
    let fixture_pool = sqlx::SqlitePool::connect(&database_url).await.unwrap();
    sqlx::query("INSERT INTO users (id, username, password_hash, created_at, updated_at) VALUES (?, ?, ?, ?, ?)")
        .bind("owner-1")
        .bind(format!("owner-{}", uuid::Uuid::new_v4()))
        .bind("not-a-real-hash")
        .bind("2026-09-26T00:00:00Z")
        .bind("2026-09-26T00:00:00Z")
        .execute(&fixture_pool)
        .await
        .unwrap();
    fixture_pool.close().await;
    let session = repository
        .create(NewConversation {
            owner_id: "owner-1".into(),
            project_id: None,
            tool_kind: Some("presentation".into()),
            title: project.title.clone(),
        })
        .await
        .unwrap();
    let artifact_service = ArtifactService::new(
        repository.clone(),
        Arc::new(LocalArtifactStorage::new(root.join("artifacts"))),
    );
    let content = serde_json::to_value(&project).unwrap();
    let pptx = output.bytes;
    assert_eq!(output.format, "pptx");
    assert!(pptx.starts_with(b"PK"));
    let publication = artifact_service
        .publish(ArtifactDraft {
            session_id: session.id.clone(),
            owner_id: "owner-1".into(),
            kind: "ppt".into(),
            title: project.title.clone(),
            extension: "pptx".into(),
            bytes: pptx,
            content,
        })
        .await
        .unwrap();
    assert_eq!(publication.status, ArtifactPublicationStatus::Ready);

    let mut event_names = Vec::new();
    for progress in progress.0.into_inner() {
        let event = match progress {
            PresentationProgress::Planning => ApplicationEvent::StateChanged {
                state: "planning_presentation".into(),
                detail: serde_json::json!({}),
            },
            PresentationProgress::ProjectCreated { project } => ApplicationEvent::ProjectUpdated {
                project: serde_json::to_value(project).unwrap(),
            },
            PresentationProgress::SlideGenerated { project, .. } => {
                ApplicationEvent::SlideUpdated {
                    slide: serde_json::to_value(project).unwrap(),
                }
            }
            PresentationProgress::GenerationCompleted { .. } => continue,
        };
        event_names.push(application_event_frame(&session.id, event).event);
    }
    event_names.push(
        application_event_frame(
            &session.id,
            ApplicationEvent::ArtifactUpdated {
                artifact: publication.clone(),
                artifacts: vec![publication.clone()],
            },
        )
        .event,
    );
    event_names.push(
        application_event_frame(
            &session.id,
            ApplicationEvent::Message {
                content: "Presentation ready".into(),
            },
        )
        .event,
    );
    event_names.push(
        application_event_frame(
            &session.id,
            ApplicationEvent::Completed {
                summary: "complete".into(),
                artifacts: vec![publication.clone()],
                new_artifacts: vec![publication.clone()],
            },
        )
        .event,
    );
    assert_eq!(
        event_names,
        vec![
            "state_update",
            "project_update",
            "slide_update",
            "slide_update",
            "artifact_update",
            "message",
            "done",
        ]
    );
    assert_eq!(publication.content["slides"].as_array().unwrap().len(), 2);
    let final_path = publication.final_path.clone().unwrap();
    assert!(tokio::fs::metadata(final_path).await.unwrap().is_file());
    assert_eq!(
        store.load(&project.id).await.unwrap().unwrap().slides.len(),
        2
    );

    drop(artifact_service);
    drop(repository);
    let reopened = SqliteSessionRepository::connect(&database_url, 1)
        .await
        .unwrap();
    assert_eq!(
        reopened
            .find(&publication.id)
            .await
            .unwrap()
            .unwrap()
            .status,
        ArtifactPublicationStatus::Ready
    );
    let recovered = reopened.legacy_artifacts(&session.id).await.unwrap();
    assert_eq!(recovered.len(), 1);
    assert_eq!(recovered[0].id, publication.id);
    assert_eq!(
        recovered[0].content["file_path"],
        publication.final_path.clone().unwrap()
    );
    drop(reopened);
    let _ = std::fs::remove_dir_all(root);
}
