//! Subscription manager: connects to the endpoint, subscribes to the
//! pool-creation programs, filters raw mentions down to actual pool-creation
//! transactions, and forwards [`LaunchEvent`]s into the pipeline.
//!
//! Reconnect policy: exponential backoff with deterministic jitter, and
//! every status is plain language — the UI shows these strings verbatim
//! (user-friendly is a requirement, not a nicety).

use serde::Serialize;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::task::JoinHandle;
use tokio::time::sleep;

use crate::pipeline::PipelineHandle;
use crate::provider::{ChainProvider, LogNotification, WsProvider};

/// Programs whose logs we watch, with human labels for the UI.
pub const POOL_PROGRAMS: &[(&str, &str)] = &[
    (
        "675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8",
        "Raydium AMM v4",
    ),
    (
        "whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc",
        "Orca Whirlpools",
    ),
    ("6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P", "Pump.fun"),
    ("pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA", "PumpSwap"),
];

/// Per-program log fragments that mark a pool-creation instruction. Raw
/// mentions fire for *every* transaction touching these programs (swaps
/// included), so without this filter the pipeline would drown.
const CREATION_MARKERS: &[(&str, &[&str])] = &[
    (
        "675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8",
        &["Instruction: Initialize2"],
    ),
    (
        "whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc",
        &["Instruction: InitializePool"],
    ),
    (
        "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P",
        &["Instruction: Create"],
    ),
    (
        "pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA",
        &["Instruction: CreatePool"],
    ),
];

/// A new-pool launch as detected on-chain. Evidence stays raw: signature,
/// slot, and the actual program logs, so anyone can re-verify.
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct LaunchEvent {
    pub signature: String,
    pub slot: u64,
    pub program: String,
    pub program_label: String,
    pub logs: Vec<String>,
    pub detected_unix_ms: u64,
}

/// Listener state, surfaced verbatim to the UI.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum ListenerStatus {
    Connecting,
    Live { since_slot: u64 },
    Reconnecting { attempt: u32, in_secs: u64 },
    Failed { reason: String },
}

impl ListenerStatus {
    pub fn plain_text(&self) -> String {
        match self {
            ListenerStatus::Connecting => "Connecting to the Solana endpoint…".to_string(),
            ListenerStatus::Live { .. } => "Connected — watching for new pools".to_string(),
            ListenerStatus::Reconnecting { attempt, in_secs } => {
                format!("Connection dropped — reconnecting in {in_secs}s (attempt {attempt})")
            }
            ListenerStatus::Failed { reason } => format!("Listener stopped: {reason}"),
        }
    }
}

fn backoff_secs(attempt: u32) -> u64 {
    let base = 2u64.saturating_pow(attempt.min(5));
    let capped = base.min(60);
    // Deterministic jitter (no extra rand dep): spreads reconnect storms.
    let jitter = u64::from(attempt).wrapping_mul(997) % 3;
    capped + jitter
}

fn is_pool_creation(program: &str, logs: &[String]) -> bool {
    let matches_marker = CREATION_MARKERS
        .iter()
        .find(|(marker_program, _)| *marker_program == program)
        .map(|(_, markers)| {
            logs.iter().any(|log| {
                // Suffix match, not substring: third-party aggregators CPI-ing
                // into the launchpad emit logs like "Instruction:
                // CreateAtaWithCloseAuthority", which would false-positive
                // the "Instruction: Create" marker if matched by substring.
                let log = log.trim_end();
                markers.iter().any(|marker| log.ends_with(marker))
            })
        })
        .unwrap_or(false);
    // Failed transactions still emit "Instruction: <X>" logs — a launch
    // that reverted created no pool, so it is not a launch.
    let transaction_failed = logs.iter().any(|log| log.contains(" failed:"));
    matches_marker && !transaction_failed
}

fn label_for(program: &str) -> &'static str {
    POOL_PROGRAMS
        .iter()
        .find(|(id, _)| id == &program)
        .map(|(_, label)| *label)
        .unwrap_or("unknown program")
}

/// Convert a raw notification into a launch event, applying the
/// pool-creation filter. Public so the `sniff` example uses the exact same
/// logic as the shipped listener (one filter, not two).
pub fn to_launch_event(notification: LogNotification) -> Option<LaunchEvent> {
    if !is_pool_creation(&notification.program, &notification.logs) {
        return None;
    }
    let detected_unix_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    Some(LaunchEvent {
        signature: notification.signature,
        slot: notification.slot,
        program_label: label_for(&notification.program).to_string(),
        program: notification.program,
        logs: notification.logs,
        detected_unix_ms,
    })
}

/// Spawn the listen/reconnect loop. Runs until the app shuts down.
pub fn spawn(
    ws_url: String,
    sink: PipelineHandle,
    status: tokio::sync::watch::Sender<ListenerStatus>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let program_ids: Vec<&str> = POOL_PROGRAMS.iter().map(|(id, _)| *id).collect();
        let mut attempt: u32 = 0;

        loop {
            let _ = status.send(ListenerStatus::Connecting);
            let mut provider = match WsProvider::connect(&ws_url).await {
                Ok(provider) => provider,
                Err(error) => {
                    attempt += 1;
                    let secs = backoff_secs(attempt);
                    tracing::warn!(%error, attempt, "connect failed");
                    let _ = status.send(ListenerStatus::Reconnecting {
                        attempt,
                        in_secs: secs,
                    });
                    sleep(Duration::from_secs(secs)).await;
                    continue;
                }
            };

            if let Err(error) = provider
                .subscribe_program_logs(&program_ids, "confirmed")
                .await
            {
                attempt += 1;
                let secs = backoff_secs(attempt);
                tracing::warn!(%error, attempt, "subscribe failed");
                let _ = status.send(ListenerStatus::Reconnecting {
                    attempt,
                    in_secs: secs,
                });
                sleep(Duration::from_secs(secs)).await;
                continue;
            }

            attempt = 0;
            tracing::info!(url = provider.url(), "subscribed to pool programs");

            loop {
                match provider.next_event().await {
                    Ok(notification) => {
                        if let Some(event) = to_launch_event(notification) {
                            let slot = event.slot;
                            let _ = status.send(ListenerStatus::Live { since_slot: slot });
                            if sink.submit(event).await.is_err() {
                                tracing::error!("pipeline closed; listener stopping");
                                let _ = status.send(ListenerStatus::Failed {
                                    reason: "internal queue closed".to_string(),
                                });
                                return;
                            }
                        }
                    }
                    Err(error) => {
                        attempt += 1;
                        let secs = backoff_secs(attempt);
                        tracing::warn!(%error, attempt, "stream error");
                        let _ = status.send(ListenerStatus::Reconnecting {
                            attempt,
                            in_secs: secs,
                        });
                        sleep(Duration::from_secs(secs)).await;
                        break;
                    }
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_swaps_and_keeps_pool_creation() {
        let program = "675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8";
        let swap = LogNotification {
            signature: "sig".into(),
            slot: 1,
            program: program.into(),
            logs: vec!["Program log: Instruction: SwapBaseIn".into()],
            subscription_id: 0,
        };
        assert!(to_launch_event(swap).is_none());

        let creation = LogNotification {
            signature: "sig".into(),
            slot: 1,
            program: program.into(),
            logs: vec![
                "Program 675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8 invoke [1]".into(),
                "Program log: Instruction: Initialize2".into(),
            ],
            subscription_id: 0,
        };
        assert!(to_launch_event(creation).is_some());

        let reverted = LogNotification {
            signature: "sig".into(),
            slot: 1,
            program: program.into(),
            logs: vec![
                "Program log: Instruction: Initialize2".into(),
                "Program 675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8 failed: custom program error: 0x1".into(),
            ],
            subscription_id: 0,
        };
        assert!(to_launch_event(reverted).is_none());
    }

    #[test]
    fn create_suffix_does_not_match_longer_instruction_names() {
        // A LaunchLab-style aggregator CPI-ing into pump.fun creates token
        // accounts and logs "Instruction: CreateAtaWithCloseAuthority" —
        // that is an account-setup log on a swap tx, not a token launch.
        let pump = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P";
        let aggregator_tx = LogNotification {
            signature: "sig".into(),
            slot: 1,
            program: pump.into(),
            logs: vec![
                "Program log: Instruction: CreateAtaWithCloseAuthority".into(),
                "Program log: Instruction: SwapTobV3".into(),
            ],
            subscription_id: 0,
        };
        assert!(to_launch_event(aggregator_tx).is_none());

        let real_create = LogNotification {
            signature: "sig".into(),
            slot: 2,
            program: pump.into(),
            logs: vec![
                "Program 6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P invoke [1]".into(),
                "Program log: Instruction: Create".into(),
            ],
            subscription_id: 0,
        };
        assert!(to_launch_event(real_create).is_some());
    }

    #[test]
    fn backoff_grows_and_caps() {
        assert_eq!(backoff_secs(0), 1);
        assert!((2..=5).contains(&backoff_secs(1))); // base 2 + 0..2s jitter
        assert!((8..=10).contains(&backoff_secs(3)));
        assert!(backoff_secs(100) <= 62); // capped at 60 + jitter
    }
}
