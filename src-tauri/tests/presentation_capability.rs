use std::sync::Arc;

use async_trait::async_trait;
use revue_office_lib::capabilities::presentation::{
    PresentationCapability, PresentationExport, PresentationExportError, PresentationExporter,
    PresentationGenerateRequest, PresentationPlan, PresentationPlanRequest, PresentationPlanner,
    PresentationPlannerError, PresentationProgress, PresentationProgressError,
    PresentationProgressSink, PresentationProject, PresentationSlidePlan, PresentationStore,
    PresentationStoreError,
};
use tokio::sync::Mutex;

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

struct FakeExporter;

#[async_trait]
impl PresentationExporter for FakeExporter {
    async fn export(
        &self,
        _project: &PresentationProject,
    ) -> Result<PresentationExport, PresentationExportError> {
        Ok(PresentationExport {
            format: "pptx".into(),
            bytes: b"fake-pptx".to_vec(),
        })
    }
}

struct FailingExporter;

#[async_trait]
impl PresentationExporter for FailingExporter {
    async fn export(
        &self,
        _project: &PresentationProject,
    ) -> Result<PresentationExport, PresentationExportError> {
        Err(PresentationExportError::Failed(anyhow::anyhow!(
            "injected export failure"
        )))
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
async fn presentation_capability_plans_generates_persists_and_exports_through_owned_ports() {
    let store = Arc::new(FakeStore::default());
    let progress = FakeProgress::default();
    let capability =
        PresentationCapability::new(Arc::new(FakePlanner), store.clone(), Arc::new(FakeExporter));

    let output = capability
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

    assert_eq!(output.project.slides.len(), 2);
    assert_eq!(output.format, "pptx");
    assert_eq!(output.bytes, b"fake-pptx");
    assert_eq!(
        store
            .saves
            .lock()
            .await
            .iter()
            .map(|project| project.slides.len())
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
    assert!(matches!(
        events[4],
        PresentationProgress::GenerationCompleted { .. }
    ));
    assert_eq!(
        store
            .load(&output.project.id)
            .await
            .unwrap()
            .unwrap()
            .slides
            .len(),
        2
    );
}

#[tokio::test]
async fn presentation_capability_export_failure_produces_no_completed_candidate() {
    let progress = FakeProgress::default();
    let capability = PresentationCapability::new(
        Arc::new(FakePlanner),
        Arc::new(FakeStore::default()),
        Arc::new(FailingExporter),
    );

    let result = capability
        .generate(
            PresentationGenerateRequest {
                owner_id: "owner-1".into(),
                title: "Architecture".into(),
                topic: "Architecture".into(),
                theme: "tech".into(),
                preferred_model: None,
                plan: None,
            },
            &progress,
        )
        .await;

    assert!(result.is_err());
    assert!(
        !progress
            .events
            .lock()
            .await
            .iter()
            .any(|event| matches!(event, PresentationProgress::GenerationCompleted { .. }))
    );
}
