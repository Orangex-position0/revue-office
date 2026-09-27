use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use revue_office_lib::agent::tool::PresentationToolProgressAdapter;
use revue_office_lib::application::artifact_service::ArtifactService;
use revue_office_lib::application::event::ApplicationEvent;
use revue_office_lib::capabilities::presentation::PresentationCapability;
use revue_office_lib::contracts::agent_run::RuntimeArtifact;
use revue_office_lib::contracts::artifact::{ArtifactDraft, ArtifactPublicationStatus};
use revue_office_lib::contracts::presentation::{
    PresentationGenerateRequest, PresentationPlan, PresentationPlanRequest, PresentationProject,
    PresentationSlidePlan,
};
use revue_office_lib::infrastructure::filesystem::artifact_storage::LocalArtifactStorage;
use revue_office_lib::infrastructure::persistence::sqlite::SqliteSessionRepository;
use revue_office_lib::ports::llm::{PresentationLlm, PresentationLlmError};
use revue_office_lib::ports::presentation_store::{PresentationStore, PresentationStoreError};
use revue_office_lib::ports::repositories::artifact_publication::ArtifactPublicationRepository;
use revue_office_lib::transport::sse::application_event_frame;
use tokio::sync::Mutex as AsyncMutex;

struct FakeLlm;

#[async_trait]
impl PresentationLlm for FakeLlm {
    async fn plan(
        &self,
        request: PresentationPlanRequest,
    ) -> Result<PresentationPlan, PresentationLlmError> {
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

#[tokio::test]
async fn presentation_flow_emits_compatible_progress_and_publishes_before_done() {
    let root =
        std::env::temp_dir().join(format!("revue-presentation-flow-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let events = Arc::new(Mutex::new(Vec::<(String, serde_json::Value)>::new()));
    let event_log = events.clone();
    let progress = PresentationToolProgressAdapter::new(Arc::new(move |name, data| {
        event_log.lock().unwrap().push((name.into(), data));
    }));
    let store = Arc::new(FakeStore::default());
    let capability = PresentationCapability::new(Arc::new(FakeLlm), store.clone());
    let project = capability
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

    let database_url = format!("sqlite://{}?mode=rwc", root.join("flow.db").display());
    let repository = Arc::new(
        SqliteSessionRepository::connect(&database_url, 1)
            .await
            .unwrap(),
    );
    let artifact_service = ArtifactService::new(
        repository.clone(),
        Arc::new(LocalArtifactStorage::new(root.join("artifacts"))),
    );
    let content = serde_json::to_value(&project).unwrap();
    let publication = artifact_service
        .publish(ArtifactDraft {
            session_id: "session-1".into(),
            owner_id: "owner-1".into(),
            kind: "ppt".into(),
            title: project.title.clone(),
            extension: "json".into(),
            bytes: serde_json::to_vec_pretty(&content).unwrap(),
            content,
        })
        .await
        .unwrap();
    assert_eq!(publication.status, ArtifactPublicationStatus::Ready);

    let mut event_names = events
        .lock()
        .unwrap()
        .iter()
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>();
    event_names.push(
        application_event_frame(
            "session-1",
            ApplicationEvent::ArtifactUpdated {
                artifact: RuntimeArtifact {
                    kind: publication.kind.clone(),
                    title: publication.title.clone(),
                    content: publication.content.clone(),
                },
            },
        )
        .event,
    );
    event_names.push(
        application_event_frame(
            "session-1",
            ApplicationEvent::Message {
                content: "Presentation ready".into(),
            },
        )
        .event,
    );
    event_names.push(
        application_event_frame(
            "session-1",
            ApplicationEvent::Completed {
                summary: "complete".into(),
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
    drop(reopened);
    let _ = std::fs::remove_dir_all(root);
}
