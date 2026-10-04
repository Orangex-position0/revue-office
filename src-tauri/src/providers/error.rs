#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ProviderError {
    #[error("provider authentication failed")]
    Authentication,
    #[error("provider rate limited")]
    RateLimited,
    #[error("provider network unavailable")]
    Network,
    #[error("provider timed out")]
    Timeout,
    #[error("provider operation cancelled")]
    Cancelled,
    #[error("provider returned invalid data")]
    InvalidResponse,
    #[error("provider operation unsupported")]
    Unsupported,
    #[error("provider unavailable")]
    Unavailable,
    #[error("provider configuration invalid")]
    InvalidConfiguration,
}
