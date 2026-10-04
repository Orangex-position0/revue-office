pub mod agent;
pub mod agent_core;
pub mod application;
pub mod bootstrap;
pub mod capabilities;
pub mod infrastructure;
pub mod providers;
pub mod transport;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

async fn start_axum_server() -> anyhow::Result<()> {
    bootstrap::run().await.map_err(anyhow::Error::new)
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
