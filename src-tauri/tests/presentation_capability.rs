use std::sync::Arc;

use async_trait::async_trait;
use revue_office_lib::capabilities::presentation::PresentationCapability;
use revue_office_lib::contracts::presentation::{
    PresentationGenerateRequest, PresentationPlan, PresentationPlanRequest, PresentationProgress,
    PresentationProject, PresentationSlidePlan,
};
use revue_office_lib::ports::llm::{PresentationLlm, PresentationLlmError};
use revue_office_lib::ports::presentation_progress::{
    PresentationProgressError, PresentationProgressSink,
};
use revue_office_lib::ports::presentation_store::{PresentationStore, PresentationStoreError};
use tokio::sync::Mutex;

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
                    title: "Architecture".into(),
                    layout: Some("title".into()),
                    goal: Some("Introduce the system".into()),
                    points: vec![],
                    visual: Some("cover".into()),
                },
                PresentationSlidePlan {
                    title: "Boundaries".into(),
                    layout: Some("content".into()),
                    goal: Some("Explain module boundaries".into()),
                    points: vec!["Transport".into(), "Application".into()],
                    visual: Some("layers".into()),
                },
            ],
        })
    }
}

#[derive(Default)]
struct FakeStore {
    saves: Mutex<Vec<PresentationProject>>,
}

#[async_trait]
impl PresentationStore for FakeStore {
    async fn save(&self, project: &PresentationProject) -> Result<(), PresentationStoreError> {
        self.saves.lock().await.push(project.clone());
        Ok(())
    }

    async fn load(
        &self,
        project_id: &str,
    ) -> Result<Option<PresentationProject>, PresentationStoreError> {
        Ok(self
            .saves
            .lock()
            .await
            .iter()
            .rev()
            .find(|project| project.id == project_id)
            .cloned())
    }
}

#[derive(Default)]
struct FakeProgress {
    events: Mutex<Vec<PresentationProgress>>,
}

#[async_trait]
impl PresentationProgressSink for FakeProgress {
    async fn emit(&self, progress: PresentationProgress) -> Result<(), PresentationProgressError> {
        self.events.lock().await.push(progress);
        Ok(())
    }
}

#[tokio::test]
async fn presentation_capability_plans_and_generates_each_slide_through_ports() {
    let store = Arc::new(FakeStore::default());
    let progress = FakeProgress::default();
    let capability = PresentationCapability::new(Arc::new(FakeLlm), store.clone());

    let project = capability
        .generate(
            PresentationGenerateRequest {
                owner_id: "owner-1".into(),
                title: "Architecture".into(),
                topic: "Architecture".into(),
                theme: "tech".into(),
                preferred_model: Some("fake-model".into()),
                plan: None,
            },
            &progress,
        )
        .await
        .expect("presentation should be generated");

    assert_eq!(project.slides.len(), 2);
    assert_eq!(
        store
            .saves
            .lock()
            .await
            .iter()
            .map(|p| p.slides.len())
            .collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
    let events = progress.events.lock().await;
    assert!(matches!(events[0], PresentationProgress::Planning));
    assert!(matches!(
        events[1],
        PresentationProgress::ProjectCreated { .. }
    ));
    assert!(matches!(
        events[2],
        PresentationProgress::SlideGenerated {
            current_index: 0,
            ..
        }
    ));
    assert!(matches!(
        events[3],
        PresentationProgress::SlideGenerated {
            current_index: 1,
            ..
        }
    ));
    assert!(matches!(events[4], PresentationProgress::Completed { .. }));
    assert_eq!(
        store.load(&project.id).await.unwrap().unwrap().slides.len(),
        2
    );
}
