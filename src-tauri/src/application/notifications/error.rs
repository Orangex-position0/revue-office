#[derive(Debug, thiserror::Error)]
pub enum NotificationError {
    #[error("notification repository is unavailable")]
    Repository(#[source] anyhow::Error),
}
