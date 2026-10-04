use crate::application::assets::AssetError;
use crate::application::conversations::SessionApplicationError;
use crate::application::notifications::NotificationError;
use crate::application::projects::ProjectError;

#[derive(Debug, thiserror::Error)]
pub enum DashboardError {
    #[error("dashboard project data is unavailable")]
    Projects(#[source] ProjectError),
    #[error("dashboard session data is unavailable")]
    Sessions(#[source] SessionApplicationError),
    #[error("dashboard asset data is unavailable")]
    Assets(#[source] AssetError),
    #[error("dashboard notification data is unavailable")]
    Notifications(#[source] NotificationError),
}
