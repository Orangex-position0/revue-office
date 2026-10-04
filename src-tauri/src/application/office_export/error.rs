#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum OfficeExportError {
    #[error("invalid export request: {0}")]
    Invalid(String),
    #[error("exported file not found")]
    NotFound,
    #[error("office renderer failed: {0}")]
    Render(#[source] anyhow::Error),
    #[error("export file storage failed: {0}")]
    Storage(#[source] anyhow::Error),
}
