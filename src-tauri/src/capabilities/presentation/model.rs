use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PresentationPlanRequest {
    pub owner_id: String,
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
    #[serde(default)]
    pub slides: Vec<PresentationSlide>,
    #[serde(default)]
    pub history: Vec<serde_json::Value>,
    #[serde(default = "default_layout")]
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

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
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
    pub italic: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub align: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valign: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fill: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shape: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rows: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cols: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub table_data: Option<Vec<Vec<String>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chart_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chart_data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresentationExport {
    pub format: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PresentationOutput {
    pub project: PresentationProject,
    pub format: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PresentationProjectUpdate {
    pub title: Option<String>,
    pub theme: Option<String>,
}

fn default_layout() -> String {
    "16x9".into()
}
