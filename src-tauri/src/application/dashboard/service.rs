use std::collections::BTreeMap;
use std::sync::Arc;

use crate::application::assets::AssetApplicationService;
use crate::application::conversations::SessionApplicationService;
use crate::application::identity::Actor;
use crate::application::notifications::NotificationApplicationService;
use crate::application::projects::ProjectApplicationService;

use super::{
    DashboardError, DashboardFileStats, DashboardNotificationStats, DashboardProjectStats,
    DashboardSummary, RecentProject, RecentSession,
};

pub struct DashboardApplicationService {
    projects: Arc<ProjectApplicationService>,
    sessions: Arc<SessionApplicationService>,
    assets: Arc<AssetApplicationService>,
    notifications: Arc<NotificationApplicationService>,
}

impl DashboardApplicationService {
    pub fn new(
        projects: Arc<ProjectApplicationService>,
        sessions: Arc<SessionApplicationService>,
        assets: Arc<AssetApplicationService>,
        notifications: Arc<NotificationApplicationService>,
    ) -> Self {
        Self {
            projects,
            sessions,
            assets,
            notifications,
        }
    }

    pub async fn summary(&self, actor: &Actor) -> Result<DashboardSummary, DashboardError> {
        let (projects, presentations, sessions, assets, unread) = tokio::try_join!(
            async {
                self.projects
                    .list(actor, None)
                    .await
                    .map_err(DashboardError::Projects)
            },
            async {
                self.projects
                    .list_presentations(actor)
                    .await
                    .map_err(DashboardError::Projects)
            },
            async {
                self.sessions
                    .list(actor, 5, None)
                    .await
                    .map_err(DashboardError::Sessions)
            },
            async {
                self.assets
                    .stats(actor)
                    .await
                    .map_err(DashboardError::Assets)
            },
            async {
                self.notifications
                    .unread_count(actor)
                    .await
                    .map_err(DashboardError::Notifications)
            },
        )?;

        let mut by_kind = BTreeMap::new();
        for project in &projects {
            *by_kind.entry(project.tool_kind.clone()).or_insert(0) += 1;
        }
        if !presentations.is_empty() {
            *by_kind.entry("ppt".into()).or_insert(0) += presentations.len() as i64;
        }

        let recent_projects = projects
            .iter()
            .take(5)
            .map(|project| RecentProject {
                id: project.id.clone(),
                title: project.title.clone(),
                description: project.description.clone(),
                tool_kind: project.tool_kind.clone(),
                created_at: project.created_at.clone(),
                updated_at: project.updated_at.clone(),
            })
            .collect();
        let recent_sessions = sessions
            .into_iter()
            .take(5)
            .map(|session| RecentSession {
                id: session.id,
                project_id: session.project_id,
                tool_kind: session.tool_kind,
                title: session.title,
                summary: session.summary,
                message_count: session.message_count,
                created_at: session.created_at,
                updated_at: session.updated_at,
            })
            .collect();

        Ok(DashboardSummary {
            projects: DashboardProjectStats {
                total: projects.len() + presentations.len(),
                by_kind,
            },
            files: DashboardFileStats {
                total: assets.total_files,
                by_type: assets.by_type.into_iter().collect(),
                total_size: assets.total_size,
            },
            notifications: DashboardNotificationStats { unread },
            recent_sessions,
            recent_projects,
        })
    }
}
