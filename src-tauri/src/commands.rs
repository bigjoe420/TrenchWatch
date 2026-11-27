//! Tauri IPC surface (docs/ARCHITECTURE.md §2). Read-oriented: the frontend
//! queries state; nothing here can mutate chain data or local config.

use serde::Serialize;
use tauri::State;

use crate::config::AlertConfig;
use crate::listener::LaunchEvent;
use crate::pipeline;
use crate::risk::LaunchAnalysis;
use crate::AppState;

#[derive(Debug, Serialize)]
pub struct StatusResponse {
    /// Plain-language listener state, shown verbatim in the UI.
    pub listener: String,
    pub events_stored: usize,
    pub events_dropped: u64,
    pub uptime_secs: u64,
    pub endpoint: String,
    /// Paid-enrichment budget; disabled when no API key is configured.
    pub helius: HeliusStatus,
}

#[derive(Debug, Serialize)]
pub struct HeliusStatus {
    pub enabled: bool,
    pub used_today: u64,
    /// 0 = unlimited (not recommended).
    pub daily_cap: u64,
}

#[derive(Debug, Serialize)]
pub struct EventsResponse {
    pub events: Vec<LaunchEvent>,
    pub total_stored: usize,
}

#[tauri::command]
pub async fn get_status(state: State<'_, AppState>) -> Result<StatusResponse, String> {
    let status = state.status.borrow().clone();
    let store = state.store.lock().map_err(|_| "store lock poisoned")?;
    let helius = state
        .helius
        .as_ref()
        .map(|meter| HeliusStatus {
            enabled: meter.enabled(),
            used_today: meter.used_today(),
            daily_cap: meter.cap(),
        })
        .unwrap_or(HeliusStatus {
            enabled: false,
            used_today: 0,
            daily_cap: 0,
        });
    Ok(StatusResponse {
        listener: status.plain_text(),
        events_stored: store.len(),
        events_dropped: pipeline::dropped_count(&state.dropped),
        uptime_secs: state.started.elapsed().as_secs(),
        endpoint: state.endpoint.clone(),
        helius,
    })
}

#[tauri::command]
pub async fn get_recent_events(
    state: State<'_, AppState>,
    limit: Option<usize>,
) -> Result<EventsResponse, String> {
    let store = state.store.lock().map_err(|_| "store lock poisoned")?;
    let events = store.recent(limit.unwrap_or(100));
    Ok(EventsResponse {
        events,
        total_stored: store.len(),
    })
}

#[tauri::command]
pub async fn get_recent_analyses(
    state: State<'_, AppState>,
    limit: Option<usize>,
) -> Result<Vec<LaunchAnalysis>, String> {
    let store = state.store.lock().map_err(|_| "store lock poisoned")?;
    Ok(store.recent_analyses(limit.unwrap_or(100)))
}

#[tauri::command]
pub async fn get_analysis(
    state: State<'_, AppState>,
    signature: String,
) -> Result<Option<LaunchAnalysis>, String> {
    let store = state.store.lock().map_err(|_| "store lock poisoned")?;
    Ok(store.analysis_for(&signature).cloned())
}

#[tauri::command]
pub async fn get_alerts_config(state: State<'_, AppState>) -> Result<AlertConfig, String> {
    let config = state.config.lock().map_err(|_| "config lock poisoned")?;
    Ok(config.alerts.clone())
}

/// Update alert settings: validate, persist to the file-backed config, and
/// take effect immediately (the alert stage reads the shared config live).
#[tauri::command]
pub async fn update_alerts_config(
    state: State<'_, AppState>,
    alerts: AlertConfig,
) -> Result<(), String> {
    alerts.validate()?;
    let mut config = state.config.lock().map_err(|_| "config lock poisoned")?;
    config.alerts = alerts;
    config
        .save(&state.app_data_dir)
        .map_err(|e| format!("could not write config.toml: {e}"))?;
    Ok(())
}
