use std::collections::BTreeMap;

use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DashboardSummary {
    pub projects: DashboardProjectStats,
    pub files: DashboardFileStats,
    pub notifications: DashboardNotificationStats,
    pub recent_sessions: Vec<RecentSession>,
    pub recent_projects: Vec<RecentProject>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DashboardProjectStats {
    pub total: usize,
    pub by_kind: BTreeMap<String, i64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DashboardFileStats {
    pub total: i64,
    pub by_type: BTreeMap<String, i64>,
    pub total_size: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DashboardNotificationStats {
    pub unread: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RecentSession {
    pub id: String,
    pub project_id: Option<String>,
    pub tool_kind: Option<String>,
    pub title: String,
    pub summary: Option<String>,
    pub message_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RecentProject {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub tool_kind: String,
    pub created_at: String,
    pub updated_at: String,
}
