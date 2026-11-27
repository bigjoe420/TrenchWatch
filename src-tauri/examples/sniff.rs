//! Live-listen proof for M2: connects to mainnet-beta, subscribes to the
//! pool-creation programs, and prints pool-creation events as JSON for a
//! fixed window. Run: `cargo run --example sniff -- 90` (seconds).
//!
//! Hits the real public endpoint — keep runs short (public-RPC etiquette).

use std::time::Duration;

use trenchwatch_lib::listener;
use trenchwatch_lib::listener::POOL_PROGRAMS;
use trenchwatch_lib::provider::{ChainProvider, WsProvider};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let seconds: u64 = std::env::args()
        .nth(1)
        .and_then(|arg| arg.parse().ok())
        .unwrap_or(60);

    let ws_url = "wss://api.mainnet-beta.solana.com";
    println!("connecting to {ws_url} …");
    let mut provider = WsProvider::connect(ws_url).await?;

    let programs: Vec<&str> = POOL_PROGRAMS.iter().map(|(id, _)| *id).collect();
    provider
        .subscribe_program_logs(&programs, "confirmed")
        .await?;
    println!(
        "subscribed to {} programs; printing pool-creation events for {seconds}s",
        programs.len()
    );

    let deadline = tokio::time::Instant::now() + Duration::from_secs(seconds);
    let mut seen = 0usize;
    let mut distinct: std::collections::BTreeMap<String, std::collections::BTreeSet<String>> =
        std::collections::BTreeMap::new();
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        match tokio::time::timeout(remaining, provider.next_event()).await {
            Ok(Ok(notification)) => {
                for instruction in notification
                    .logs
                    .iter()
                    .filter_map(|log| log.strip_prefix("Program log: Instruction: "))
                {
                    distinct
                        .entry(notification.program.clone())
                        .or_default()
                        .insert(instruction.to_string());
                }
                if let Some(event) = listener::to_launch_event(notification) {
                    seen += 1;
                    println!(
                        "LAUNCH [{}] slot={} sig={}",
                        event.program_label, event.slot, event.signature
                    );
                }
            }
            Ok(Err(error)) => {
                println!("stream error: {error}");
                break;
            }
            Err(_) => break, // deadline
        }
    }
    println!("done — {seen} pool-creation event(s) in {seconds}s");
    for (program, instructions) in distinct {
        println!("{program}:");
        for instruction in instructions {
            println!("    {instruction}");
        }
    }
    Ok(())
}
