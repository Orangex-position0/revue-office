use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PresentationPlanRequest {
    pub topic: String,
    pub audience: Option<String>,
    pub preferred_model: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PresentationGenerateRequest {
    pub owner_id: String,
    pub title: String,
    pub topic: String,
    pub theme: String,
    pub preferred_model: Option<String>,
    pub plan: Option<PresentationPlan>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PresentationPlan {
    pub title: String,
    pub slides: Vec<PresentationSlidePlan>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PresentationSlidePlan {
    pub title: String,
    #[serde(default)]
    pub layout: Option<String>,
    #[serde(default)]
    pub goal: Option<String>,
    #[serde(default)]
    pub points: Vec<String>,
    #[serde(default)]
    pub visual: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PresentationProject {
    pub id: String,
    pub title: String,
    pub theme: String,
    pub slides: Vec<PresentationSlide>,
    pub history: Vec<serde_json::Value>,
    pub layout: String,
    pub created_at: String,
    pub updated_at: String,
    pub owner_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PresentationSlide {
    pub id: String,
    pub layout: String,
    pub background: String,
    pub elements: Vec<PresentationElement>,
    pub notes: Option<String>,
    pub title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PresentationElement {
    #[serde(rename = "type")]
    pub element_type: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_size: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bold: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub align: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valign: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fill: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shape: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PresentationProgress {
    Planning,
    ProjectCreated {
        project: PresentationProject,
    },
    SlideGenerated {
        project: PresentationProject,
        current_index: usize,
        total_slides: usize,
    },
    Completed {
        project: PresentationProject,
    },
}
