use std::sync::Arc;

use axum::extract::FromRef;

use crate::application::assets::AssetApplicationService;
use crate::application::chat_service::ChatApplicationService;
use crate::application::conversations::SessionApplicationService;
use crate::application::dashboard::DashboardApplicationService;
use crate::application::identity::IdentityApplicationService;
use crate::application::notifications::NotificationApplicationService;
use crate::application::office_export::OfficeExportApplicationService;
use crate::application::preferences::PreferenceApplicationService;
use crate::application::projects::ProjectApplicationService;
use crate::transport::http::auth::IdentityState;

#[derive(Clone, FromRef)]
pub struct HttpState {
    identity: IdentityState,
    identity_application: Arc<IdentityApplicationService>,
    chat: Arc<ChatApplicationService>,
    sessions: Arc<SessionApplicationService>,
    assets: Arc<AssetApplicationService>,
    projects: Arc<ProjectApplicationService>,
    office_export: Arc<OfficeExportApplicationService>,
    preferences: Arc<PreferenceApplicationService>,
    notifications: Arc<NotificationApplicationService>,
    dashboard: Arc<DashboardApplicationService>,
}

impl HttpState {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        identity: IdentityState,
        identity_application: Arc<IdentityApplicationService>,
        chat: Arc<ChatApplicationService>,
        sessions: Arc<SessionApplicationService>,
        assets: Arc<AssetApplicationService>,
        projects: Arc<ProjectApplicationService>,
        office_export: Arc<OfficeExportApplicationService>,
        preferences: Arc<PreferenceApplicationService>,
        notifications: Arc<NotificationApplicationService>,
        dashboard: Arc<DashboardApplicationService>,
    ) -> Self {
        Self {
            identity,
            identity_application,
            chat,
            sessions,
            assets,
            projects,
            office_export,
            preferences,
            notifications,
            dashboard,
        }
    }
}
