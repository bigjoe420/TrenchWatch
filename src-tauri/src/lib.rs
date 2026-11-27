pub mod commands;
pub mod config;
pub mod listener;
pub mod pipeline;
pub mod provider;
pub mod store;

use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use tauri::Manager;
use tokio::sync::watch;

pub struct AppState {
    pub store: Arc<Mutex<store::EventStore>>,
    pub status: watch::Receiver<listener::ListenerStatus>,
    pub dropped: Arc<AtomicU64>,
    pub started: Instant,
    pub endpoint: String,
}

pub fn run() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let app_data = app
                .path()
                .app_data_dir()
                .map_err(|e| anyhow::anyhow!("resolve app data dir: {e}"))?;
            std::fs::create_dir_all(&app_data)
                .map_err(|e| anyhow::anyhow!("create app data dir: {e}"))?;

            let config = config::load(&app_data)?;
            let store_dir = app_data.join(&config.storage.store_dir);
            let store = store::EventStore::open(&store_dir)?;
            tracing::info!(path = ?store.path(), "event store open");
            let store = Arc::new(Mutex::new(store));

            let (status_tx, status_rx) = watch::channel(listener::ListenerStatus::Connecting);
            let dropped = Arc::new(AtomicU64::new(0));

            let (handle, sink_task) = pipeline::spawn(store.clone(), 1024, dropped.clone());
            tauri::async_runtime::spawn(sink_task);

            let listener_task = listener::spawn(config.rpc.ws_url.clone(), handle, status_tx);
            tauri::async_runtime::spawn(listener_task);

            app.manage(AppState {
                store,
                status: status_rx,
                dropped,
                started: Instant::now(),
                endpoint: config.rpc.ws_url.clone(),
            });
            tracing::info!("trenchwatch backend up");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_status,
            commands::get_recent_events
        ])
        .run(tauri::generate_context!())
        .map_err(|e| anyhow::anyhow!("tauri runtime error: {e}"))
}
