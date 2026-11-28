//! Analysis stage: receives launches from the pipeline, enriches each one
//! (one provider connection, reused; reconnects on transport failure), and
//! persists the resulting analysis next to the event.

use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::sync::mpsc;
use tokio::time::sleep;

use crate::alerts::Alerts;
use crate::config::Config;
use crate::enrich::analyze_launch;
use crate::listener::LaunchEvent;
use crate::provider::WsProvider;
use crate::store::{EventStore, StoreError};

/// Returns the channel end for producers and the stage future for the
/// caller to spawn on its async runtime (Tauri setup has no Tokio context).
/// `config` is shared with the settings commands; the effective websocket
/// (which follows the current API key) is rebuilt on every connect.
pub fn spawn(
    store: Arc<Mutex<EventStore>>,
    config: Arc<Mutex<Config>>,
    alerts: Arc<Alerts>,
    helius: Option<Arc<crate::provider::HeliusMeter>>,
) -> (mpsc::Sender<LaunchEvent>, impl Future<Output = ()>) {
    let (tx, mut rx) = mpsc::channel::<LaunchEvent>(256);

    let task = async move {
        let mut conn = connect_with_retry(&config, &helius).await;
        while let Some(event) = rx.recv().await {
            if conn.is_none() {
                conn = connect_with_retry(&config, &helius).await;
            }
            let Some(provider) = conn.as_mut() else {
                tracing::warn!(signature = %event.signature, "no provider; skipping analysis");
                continue;
            };

            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_millis() as u64)
                .unwrap_or(0);
            // Wallet reputation comes from our own stored receipts: prior
            // analyses attributed to the same deployer wallet.
            let history_store = Arc::clone(&store);
            let signature = event.signature.clone();
            let outcome = analyze_launch(provider, &event, now, &move |wallet| {
                history_store
                    .lock()
                    .map(|store| {
                        store
                            .analyses_for_wallet(wallet, &signature)
                            .iter()
                            .map(crate::risk::PriorLaunch::from)
                            .collect()
                    })
                    .unwrap_or_default()
            })
            .await;

            if !outcome.provider_ok {
                // Socket looks broken; drop it so the next event reconnects.
                conn = None;
            }

            let result = store
                .lock()
                .map_err(|_| StoreError::Poisoned)
                .and_then(|mut guard| guard.append_analysis(&outcome.analysis));
            if let Err(error) = result {
                tracing::warn!(%error, signature = %event.signature, "analysis persist failed");
            }

            // Lifecycle: a PumpSwap pool creation whose mint matches an
            // earlier Pump.fun launch IS that token's graduation — the
            // migration transaction creates the PumpSwap pool. Link the two
            // receipts and alert; if we never saw the Pump.fun launch (app
            // was off, mint unidentified) there is honestly nothing to link.
            if event.program == crate::listener::PUMPSWAP_PROGRAM {
                if let Some(mint) = outcome.analysis.mint.as_deref() {
                    let graduated = store.lock().ok().and_then(|mut guard| {
                        let launch_signature =
                            guard.launch_signature_by_mint(mint, crate::listener::PUMPFUN_PROGRAM)?;
                        guard.mark_graduated(
                            &launch_signature,
                            crate::risk::Graduation {
                                signature: event.signature.clone(),
                                slot: event.slot,
                                venue_label: event.program_label.clone(),
                                detected_unix_ms: now,
                            },
                        )
                    });
                    if let Some(updated) = graduated {
                        tracing::info!(%mint, "pump.fun token graduated to pumpswap");
                        alerts.notify_graduation(&updated).await;
                    }
                }
            }

            alerts.notify(&outcome.analysis).await;
        }
        tracing::info!("analysis stage drained");
    };

    (tx, task)
}

async fn connect_with_retry(
    config: &Arc<Mutex<Config>>,
    helius: &Option<Arc<crate::provider::HeliusMeter>>,
) -> Option<WsProvider> {
    let mut attempt = 0u32;
    loop {
        let ws_url = config
            .lock()
            .map(|c| c.effective_ws_url())
            .unwrap_or_default();
        match WsProvider::connect_with_helius(&ws_url, helius.clone()).await {
            Ok(provider) => {
                tracing::info!("analysis provider connected");
                return Some(provider);
            }
            Err(error) => {
                attempt += 1;
                // 429: back off for minutes — hammering keeps Helius's
                // rate limiter hot (same policy as the listener).
                let secs = if error.to_string().contains("429") {
                    300
                } else {
                    2u64.saturating_pow(attempt.min(5)).min(60)
                };
                tracing::warn!(%error, attempt, secs, "analysis provider connect failed");
                sleep(Duration::from_secs(secs)).await;
            }
        }
    }
}
