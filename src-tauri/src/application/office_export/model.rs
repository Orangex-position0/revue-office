use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DocumentExportRequest {
    pub title: String,
    pub sections: Vec<DocumentSection>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DocumentSection {
    pub heading: String,
    #[serde(
        default = "default_heading_level",
        rename = "headingLevel",
        alias = "heading_level"
    )]
    pub heading_level: u32,
    #[serde(default)]
    pub paragraphs: Vec<String>,
    #[serde(default)]
    pub bullets: Vec<String>,
    #[serde(default)]
    pub table: Option<DocumentTable>,
}

fn default_heading_level() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DocumentTable {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SpreadsheetExportRequest {
    pub title: String,
    pub tables: Vec<SpreadsheetTable>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SpreadsheetTable {
    pub title: String,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportedFile {
    pub filename: String,
    pub content_type: String,
    pub bytes: Vec<u8>,
}
