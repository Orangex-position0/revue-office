use std::sync::Arc;

use thiserror::Error;

use crate::contracts::presentation::{
    PresentationElement, PresentationGenerateRequest, PresentationPlan, PresentationPlanRequest,
    PresentationProgress, PresentationProject, PresentationSlide, PresentationSlidePlan,
};
use crate::ports::llm::{PresentationLlm, PresentationLlmError};
use crate::ports::presentation_progress::{PresentationProgressError, PresentationProgressSink};
use crate::ports::presentation_store::{PresentationStore, PresentationStoreError};

#[derive(Debug, Error)]
pub enum PresentationCapabilityError {
    #[error(transparent)]
    Llm(#[from] PresentationLlmError),
    #[error(transparent)]
    Store(#[from] PresentationStoreError),
    #[error(transparent)]
    Progress(#[from] PresentationProgressError),
    #[error("presentation topic cannot be empty")]
    EmptyTopic,
    #[error("presentation plan contains no slides")]
    EmptyPlan,
}

pub struct PresentationCapability {
    llm: Arc<dyn PresentationLlm>,
    store: Arc<dyn PresentationStore>,
}

impl PresentationCapability {
    pub fn new(llm: Arc<dyn PresentationLlm>, store: Arc<dyn PresentationStore>) -> Self {
        Self { llm, store }
    }

    pub async fn plan(
        &self,
        request: PresentationPlanRequest,
        progress: &dyn PresentationProgressSink,
    ) -> Result<PresentationPlan, PresentationCapabilityError> {
        if request.topic.trim().is_empty() {
            return Err(PresentationCapabilityError::EmptyTopic);
        }
        progress.emit(PresentationProgress::Planning).await?;
        let plan = self.llm.plan(request).await?;
        if plan.slides.is_empty() {
            return Err(PresentationCapabilityError::EmptyPlan);
        }
        Ok(plan)
    }

    pub async fn generate(
        &self,
        request: PresentationGenerateRequest,
        progress: &dyn PresentationProgressSink,
    ) -> Result<PresentationProject, PresentationCapabilityError> {
        let plan = match request.plan {
            Some(plan) if !plan.slides.is_empty() => plan,
            Some(_) => return Err(PresentationCapabilityError::EmptyPlan),
            None => {
                self.plan(
                    PresentationPlanRequest {
                        topic: request.topic.clone(),
                        audience: None,
                        preferred_model: request.preferred_model.clone(),
                    },
                    progress,
                )
                .await?
            }
        };
        let now = chrono::Utc::now().to_rfc3339();
        let total_slides = plan.slides.len();
        let mut project = PresentationProject {
            id: uuid::Uuid::new_v4().to_string(),
            title: if plan.title.trim().is_empty() {
                request.title
            } else {
                plan.title
            },
            theme: request.theme,
            slides: Vec::with_capacity(total_slides),
            history: vec![serde_json::json!({
                "type": "create",
                "title": "AI generated presentation",
                "total_slides": total_slides,
                "created_at": now,
            })],
            layout: "16x9".into(),
            created_at: now.clone(),
            updated_at: now,
            owner_id: request.owner_id,
        };
        self.store.save(&project).await?;
        progress
            .emit(PresentationProgress::ProjectCreated {
                project: project.clone(),
            })
            .await?;

        for (index, slide_plan) in plan.slides.iter().enumerate() {
            let slide = render_slide(slide_plan, index, &project.theme);
            project.history.push(serde_json::json!({
                "type": "draw",
                "slide_index": index,
                "slide_title": slide.title,
                "created_at": chrono::Utc::now().to_rfc3339(),
            }));
            project.slides.push(slide);
            project.updated_at = chrono::Utc::now().to_rfc3339();
            self.store.save(&project).await?;
            progress
                .emit(PresentationProgress::SlideGenerated {
                    project: project.clone(),
                    current_index: index,
                    total_slides,
                })
                .await?;
        }

        progress
            .emit(PresentationProgress::Completed {
                project: project.clone(),
            })
            .await?;
        Ok(project)
    }
}

fn render_slide(plan: &PresentationSlidePlan, index: usize, theme: &str) -> PresentationSlide {
    let (background, primary, text_color) = palette(theme, index);
    let is_title = index == 0 || plan.layout.as_deref() == Some("title");
    let layout = if is_title {
        "title"
    } else {
        plan.layout.as_deref().unwrap_or("content")
    };
    let mut elements = vec![shape(0.0, 0.0, 13.33, 7.5, background, "rect")];
    if is_title {
        elements.push(text(
            1.0,
            2.2,
            11.3,
            1.2,
            &plan.title,
            42.0,
            text_color,
            true,
        ));
        if let Some(goal) = &plan.goal {
            elements.push(text(1.0, 3.6, 10.5, 0.7, goal, 21.0, primary, false));
        }
    } else {
        elements.push(shape(0.8, 0.7, 0.12, 6.1, primary, "roundRect"));
        elements.push(text(
            1.2,
            0.9,
            10.8,
            0.8,
            &plan.title,
            30.0,
            text_color,
            true,
        ));
        if !plan.points.is_empty() {
            let points = plan
                .points
                .iter()
                .take(4)
                .map(|point| format!("• {point}"))
                .collect::<Vec<_>>()
                .join("\n");
            elements.push(text(1.3, 2.0, 10.6, 4.5, &points, 20.0, text_color, false));
        }
    }
    PresentationSlide {
        id: uuid::Uuid::new_v4().to_string(),
        layout: layout.into(),
        background: background.into(),
        elements,
        notes: plan.goal.clone(),
        title: Some(plan.title.clone()),
    }
}

fn palette(theme: &str, index: usize) -> (&'static str, &'static str, &'static str) {
    match theme {
        "tech" => (
            "0B1120",
            if index % 2 == 0 { "22D3EE" } else { "A78BFA" },
            "F8FAFC",
        ),
        "warm" => (
            "FFF7ED",
            if index % 2 == 0 { "EA580C" } else { "D97706" },
            "431407",
        ),
        "minimal" => ("FFFFFF", "71717A", "18181B"),
        _ => (
            "F8FAFC",
            if index % 2 == 0 { "2563EB" } else { "0F766E" },
            "0F172A",
        ),
    }
}

fn shape(x: f64, y: f64, w: f64, h: f64, fill: &str, shape_type: &str) -> PresentationElement {
    PresentationElement {
        element_type: "shape".into(),
        x,
        y,
        w,
        h,
        text: None,
        font_size: None,
        color: None,
        bold: None,
        align: None,
        valign: None,
        fill: Some(fill.into()),
        shape: Some(shape_type.into()),
    }
}

fn text(
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    content: &str,
    font_size: f64,
    color: &str,
    bold: bool,
) -> PresentationElement {
    PresentationElement {
        element_type: "text".into(),
        x,
        y,
        w,
        h,
        text: Some(content.into()),
        font_size: Some(font_size),
        color: Some(color.into()),
        bold: Some(bold),
        align: Some("left".into()),
        valign: Some("middle".into()),
        fill: None,
        shape: None,
    }
}
