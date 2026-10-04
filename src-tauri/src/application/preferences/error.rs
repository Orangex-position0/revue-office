#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PreferenceError {
    #[error("invalid preference: {0}")]
    Invalid(String),
    #[error("credential mutation is forbidden")]
    Forbidden,
    #[error("preference conflict")]
    Conflict,
    #[error("preference service unavailable")]
    Unavailable,
}
