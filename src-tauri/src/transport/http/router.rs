use std::path::PathBuf;

use crate::transport::http::handlers;
use crate::transport::http::{HttpState, fallback};
use axum::Router;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

pub struct HttpConfig {
    pub output_root: PathBuf,
    pub health: handlers::health::HealthInfo,
    pub cors: CorsLayer,
}

pub fn build(state: HttpState, config: HttpConfig) -> Router {
    Router::new()
        .merge(handlers::auth::router())
        .merge(handlers::chat::router())
        .merge(handlers::session::router())
        .merge(handlers::projects::router())
        .merge(handlers::notifications::router())
        .merge(handlers::preferences::router())
        .merge(handlers::assets::router())
        .merge(handlers::dashboard::router())
        .merge(handlers::office_export::router())
        .merge(handlers::health::router(config.health))
        .nest_service("/outputs", ServeDir::new(config.output_root))
        .fallback(fallback::fallback_handler)
        .layer(config.cors)
        .with_state(state)
}
