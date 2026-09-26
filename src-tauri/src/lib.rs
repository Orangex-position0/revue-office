mod agent;
mod app;
pub mod application;
mod auth;
mod commands;
mod config;
pub mod contracts;
mod db;
mod error;
mod file_extract;
mod files;
mod image_ocr;
pub mod infrastructure;
mod llm;
mod models;
pub mod ports;
mod render;
mod routes;
mod state;
mod transport;

use std::net::SocketAddr;
use tracing::info;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

async fn start_axum_server() -> anyhow::Result<()> {
    let cfg = config::config();
    cfg.ensure_dirs()?;

    let pool = state::init_db_pool().await;
    let session_service =
        app::bootstrap::build_session_service(&cfg.database_url, cfg.db_max_connections).await?;
    state::set_db_pool(pool);
    app::state::set_session_service(session_service)?;

    agent::tools::register_all_tools().await;

    let app = routes::build_router();
    let addr: SocketAddr = format!("{}:{}", cfg.host, cfg.port).parse()?;

    info!("🚀 {} API running at http://{}", cfg.app_name, addr);
    info!("📝 LLM: {} @ {}", cfg.llm_model, cfg.llm_base_url);
    info!("📂 Projects: {}", cfg.projects_dir);
    if cfg.is_mysql() {
        info!("🗄️ Database: MySQL");
    } else {
        info!("🗄️ Database: SQLite");
    }

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,revue_office=debug".into()),
        )
        .try_init();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_tracing();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|_app| {
            tauri::async_runtime::spawn(async {
                if let Err(err) = start_axum_server().await {
                    tracing::error!("failed to start API server: {err:#}");
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![greet])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
