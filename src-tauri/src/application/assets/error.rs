#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum AssetError {
    #[error("invalid asset: {0}")]
    Invalid(String),
    #[error("asset not found")]
    NotFound,
    #[error("asset access forbidden")]
    Forbidden,
    #[error("asset repository unavailable: {0}")]
    Repository(#[source] anyhow::Error),
    #[error("asset storage unavailable: {0}")]
    Storage(#[source] anyhow::Error),
    #[error("asset extraction failed: {0}")]
    Extraction(#[source] anyhow::Error),
    #[error("asset operation unsupported: {0}")]
    Unsupported(String),
}
