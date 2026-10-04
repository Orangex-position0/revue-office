mod error;
mod model;
mod service;

pub use error::DashboardError;
pub use model::{
    DashboardFileStats, DashboardNotificationStats, DashboardProjectStats, DashboardSummary,
    RecentProject, RecentSession,
};
pub use service::DashboardApplicationService;
