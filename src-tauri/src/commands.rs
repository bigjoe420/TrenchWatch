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

#[derive(Debug, Serialize)]
pub struct RpcSetupStatus {
    /// Whether a Helius API key is on file. The key itself never leaves the
    /// backend — the wizard only needs to know if the step is already done.
    pub api_key_configured: bool,
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

/// Open an https link in the user's browser (used for the community
/// Discord invite). https-only by construction — the app never needs to
/// hand anything else to the OS shell.
#[tauri::command]
pub async fn open_url(url: String) -> Result<(), String> {
    validate_external_url(&url)?;
    open::that(&url).map_err(|e| format!("could not open the link: {e}"))
}

#[tauri::command]
pub async fn get_rpc_setup_status(state: State<'_, AppState>) -> Result<RpcSetupStatus, String> {
    let config = state.config.lock().map_err(|_| "config lock poisoned")?;
    Ok(RpcSetupStatus {
        api_key_configured: !config.rpc.api_key.trim().is_empty(),
    })
}

/// Install (or clear, with an empty string) the Helius API key from the
/// first-run wizard. Persists to config.toml and applies to the live meter —
/// the meter always exists (created with an empty key when none is set), so
/// a first key saved mid-session enables enrichment without a restart.
/// The launch stream itself is unaffected either way.
#[tauri::command]
pub async fn set_api_key(state: State<'_, AppState>, api_key: String) -> Result<(), String> {
    let key = api_key.trim().to_string();
    if key.len() > 256 {
        return Err("that api key looks too long".to_string());
    }
    let mut config = state.config.lock().map_err(|_| "config lock poisoned")?;
    config.rpc.api_key = key.clone();
    config
        .save(&state.app_data_dir)
        .map_err(|e| format!("could not write config.toml: {e}"))?;
    if let Some(meter) = &state.helius {
        meter.set_api_key(&key);
    }
    Ok(())
}

/// The only external links the UI offers are https; refuse anything else
/// so the command can never become a general shell-launch primitive.
fn validate_external_url(url: &str) -> Result<(), String> {
    if url.starts_with("https://") && url.len() <= 2048 {
        Ok(())
    } else {
        Err("only https links can be opened".to_string())
    }
}
