pub mod alerts;
pub mod analysis;
pub mod commands;
pub mod config;
pub mod enrich;
pub mod listener;
pub mod pipeline;
pub mod provider;
pub mod risk;
pub mod store;

use std::path::PathBuf;
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
    /// Shared with the listener/analysis spawns: edits via `set_api_key`
    /// are visible to the reconnect loops, which rebuild the effective
    /// websocket (and thus the key) on every connect attempt.
    pub config: Arc<Mutex<config::Config>>,
    pub app_data_dir: PathBuf,
    /// Paid-enrichment budget; `None` = public endpoint for everything.
    pub helius: Option<Arc<provider::HeliusMeter>>,
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
            let shared_config = Arc::new(Mutex::new(config.clone()));
            let store_dir = app_data.join(&config.storage.store_dir);
            let store = store::EventStore::open(&store_dir)?;
            tracing::info!(path = ?store.path(), "event store open");
            let store = Arc::new(Mutex::new(store));

            let (status_tx, status_rx) = watch::channel(listener::ListenerStatus::Connecting);
            let dropped = Arc::new(AtomicU64::new(0));

            let alerts_config = alerts::SharedAlertConfig::new(Mutex::new(config.alerts.clone()));
            let alerts = Arc::new(alerts::Alerts::new(alerts_config, app.handle().clone()));

            // Paid enrichment budget: only built when a key is configured,
            // so the app runs entirely on the free public RPC otherwise.
            let helius = if config.rpc.api_key.trim().is_empty() {
                None
            } else {
                tracing::info!(
                    cap = config.rpc.daily_credit_cap,
                    "helius enrichment enabled (key loaded from config.toml)"
                );
                Some(Arc::new(provider::HeliusMeter::new(
                    &config.rpc.api_key,
                    config.rpc.daily_credit_cap,
                )))
            };

            let (analysis_tx, analysis_task) = analysis::spawn(
                store.clone(),
                shared_config.clone(),
                alerts,
                helius.clone(),
            );
            tauri::async_runtime::spawn(analysis_task);

            let (handle, sink_task) =
                pipeline::spawn(store.clone(), 1024, dropped.clone(), analysis_tx);
            tauri::async_runtime::spawn(sink_task);

            let listener_task =
                listener::spawn(shared_config.clone(), handle, status_tx);
            tauri::async_runtime::spawn(listener_task);

            app.manage(AppState {
                store,
                status: status_rx,
                dropped,
                started: Instant::now(),
                endpoint: config.rpc.ws_url.clone(),
                config: shared_config,
                app_data_dir: app_data.clone(),
                helius,
            });
            tracing::info!("trenchwatch backend up");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_status,
            commands::get_recent_events,
            commands::get_recent_analyses,
            commands::get_analysis,
            commands::get_alerts_config,
            commands::update_alerts_config,
            commands::get_rpc_setup_status,
            commands::set_api_key,
            commands::open_url
        ])
        .run(tauri::generate_context!())
        .map_err(|e| anyhow::anyhow!("tauri runtime error: {e}"))
}
