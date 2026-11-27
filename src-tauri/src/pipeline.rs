//! Bounded hand-off between the listener and the store.
//!
//! Under saturation we never block the chain listener and never drop the
//! newest event silently — full-queue submissions are counted and
//! surfaced as "degraded" in the UI (docs/THREAT_MODEL.md §3.A1).

use std::collections::HashSet;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::listener::LaunchEvent;
use crate::store::{EventStore, StoreError};

const DEDUPE_WINDOW: usize = 8_192;

#[derive(Debug, thiserror::Error)]
pub enum PipelineError {
    #[error("queue is full — event dropped (listener is producing faster than we can persist)")]
    Full,
}

/// Listener-facing submission handle.
#[derive(Clone)]
pub struct PipelineHandle {
    tx: mpsc::Sender<LaunchEvent>,
}

impl PipelineHandle {
    pub async fn submit(&self, event: LaunchEvent) -> Result<(), PipelineError> {
        self.tx.try_send(event).map_err(|_| PipelineError::Full)
    }
}

/// Spawn the drain loop: dedupe by signature, persist to the store, then
/// hand the launch to the analysis stage.
pub fn spawn(
    store: Arc<Mutex<EventStore>>,
    capacity: usize,
    dropped: Arc<AtomicU64>,
    analysis_tx: mpsc::Sender<LaunchEvent>,
) -> (PipelineHandle, JoinHandle<()>) {
    let (tx, mut rx) = mpsc::channel::<LaunchEvent>(capacity);

    let task = tokio::spawn(async move {
        let mut seen: HashSet<String> = HashSet::new();
        let mut ring: VecDeque<String> = VecDeque::new();
        let mut analysis_closed = false;

        while let Some(event) = rx.recv().await {
            if !seen.insert(event.signature.clone()) {
                continue; // duplicate notification (retry or reorg replay)
            }
            ring.push_back(event.signature.clone());
            if ring.len() > DEDUPE_WINDOW {
                if let Some(oldest) = ring.pop_front() {
                    seen.remove(&oldest);
                }
            }

            let result = store
                .lock()
                .map_err(|_| StoreError::Poisoned)
                .and_then(|mut guard| guard.append(&event));
            if let Err(error) = result {
                dropped.fetch_add(1, Ordering::Relaxed);
                tracing::warn!(%error, signature = %event.signature, "persist failed");
                continue;
            }

            if !analysis_closed && analysis_tx.send(event).await.is_err() {
                analysis_closed = true;
                tracing::warn!("analysis stage closed; launches will not be scored");
            }
        }
        tracing::info!("pipeline drained; sink stopping");
    });

    (PipelineHandle { tx }, task)
}

pub fn dropped_count(dropped: &AtomicU64) -> u64 {
    dropped.load(Ordering::Relaxed)
}
