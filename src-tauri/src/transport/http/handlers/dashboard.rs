use std::sync::Arc;

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};

use crate::application::dashboard::{
    DashboardApplicationService, DashboardError, DashboardSummary,
};
use crate::transport::http::HttpState;
use crate::transport::http::auth::AuthenticatedActor;
use crate::transport::http::error::AppError;

pub fn router() -> Router<HttpState> {
    Router::new().route("/api/dashboard/stats", get(dashboard_stats))
}

fn map_error(error: DashboardError) -> AppError {
    AppError::Internal(anyhow::Error::new(error))
}

async fn dashboard_stats(
    State(service): State<Arc<DashboardApplicationService>>,
    AuthenticatedActor(actor): AuthenticatedActor,
) -> Result<Json<DashboardSummary>, AppError> {
    service.summary(&actor).await.map(Json).map_err(map_error)
}
