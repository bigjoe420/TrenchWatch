//! Live proof harness for the risk engine.
//!
//! Connects to mainnet-beta over the public websocket, collects pool-creation
//! launches exactly like the app's listener does, runs each through
//! `enrich::analyze_launch`, and prints the resulting evidence-backed
//! analysis as JSON. Exits after `SAMPLE_COUNT` launches or `RUN_LIMIT`.
//!
//! Run with: `cargo run --example analyze`

use std::time::Duration;

use trenchwatch_lib::enrich::analyze_launch;
use trenchwatch_lib::listener::{to_launch_event, LaunchEvent, POOL_PROGRAMS};
use trenchwatch_lib::provider::{ChainProvider, WsProvider};

const WS_URL: &str = "wss://api.mainnet-beta.solana.com";
const SAMPLE_COUNT: usize = 2;
const RUN_LIMIT: Duration = Duration::from_secs(150);

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let mut listener = WsProvider::connect(WS_URL).await?;
    let programs: Vec<&str> = POOL_PROGRAMS.iter().map(|(program, _)| *program).collect();
    listener
        .subscribe_program_logs(&programs, "confirmed")
        .await?;
    tracing::info!(
        programs = programs.len(),
        "subscribed to pool-creation logs"
    );

    let mut enricher = WsProvider::connect(WS_URL).await?;
    let deadline = tokio::time::Instant::now() + RUN_LIMIT;

    let mut collected: Vec<LaunchEvent> = Vec::new();
    while collected.len() < SAMPLE_COUNT {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            tracing::warn!(
                got = collected.len(),
                "run limit reached before sample count"
            );
            break;
        }
        let notification = match tokio::time::timeout(remaining, listener.next_event()).await {
            Ok(result) => result?,
            Err(_) => break,
        };
        let Some(event) = to_launch_event(notification) else {
            continue;
        };
        if collected
            .iter()
            .any(|seen| seen.signature == event.signature)
        {
            continue;
        }
        tracing::info!(
            slot = event.slot,
            program = %event.program,
            signature = %event.signature,
            "launch detected"
        );
        collected.push(event);
    }

    if collected.is_empty() {
        anyhow::bail!("no launches collected — cannot prove the analysis path");
    }

    for event in &collected {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let outcome = analyze_launch(&mut enricher, event, now).await;
        if !outcome.provider_ok {
            // Transient fetch failure: the analysis records honest
            // not_assessed reasons, which is still valid proof output.
            tracing::warn!("provider reported errors during this analysis");
        }
        println!("---\n{}", serde_json::to_string_pretty(&outcome.analysis)?);
    }

    println!("proof complete: {} launch analyses", collected.len());
    Ok(())
}
