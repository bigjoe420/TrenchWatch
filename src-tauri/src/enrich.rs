//! Fetch-and-assess glue: turns a raw [`LaunchEvent`] into a
//! [`LaunchAnalysis`] by pulling the few read-only facts the engine needs
//! (creation tx → mint → mint account + largest holders).
//!
//! All network access lives here (and in `provider`) — `risk` stays pure.

use serde_json::Value;

use crate::listener::LaunchEvent;
use crate::provider::{ChainProvider, WsProvider};
use crate::risk::{self, AnalysisInput, HolderSnapshot, LaunchAnalysis, NotAssessed};

/// One analysis pass plus whether the provider still looks healthy
/// (persistent transport errors mean the next pass should reconnect first).
pub struct EnrichmentOutcome {
    pub analysis: LaunchAnalysis,
    pub provider_ok: bool,
}

/// Identify the token mint created by a pool-creation transaction: the first
/// token balance that appears in `postTokenBalances` without a matching
/// `preTokenBalances` entry.
/// The SPL wrapped-SOL mint. Pool creation always moves WSOL (rent, seed
/// liquidity), so it shows up as a "new" balance in the creation tx — it
/// is never the token being launched.
const WRAPPED_SOL_MINT: &str = "So11111111111111111111111111111111111111112";

fn extract_new_mint(transaction: &Value) -> Option<String> {
    let meta = transaction.get("meta")?;
    let post = meta.get("postTokenBalances")?.as_array()?;
    let pre_mints: Vec<&str> = meta
        .get("preTokenBalances")
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| entry.get("mint").and_then(Value::as_str))
                .collect()
        })
        .unwrap_or_default();
    post.iter()
        .filter_map(|entry| entry.get("mint").and_then(Value::as_str))
        .find(|mint| !pre_mints.contains(mint) && *mint != WRAPPED_SOL_MINT)
        .map(str::to_string)
}

pub async fn analyze_launch(
    provider: &mut WsProvider,
    event: &LaunchEvent,
    now_unix_ms: u64,
) -> EnrichmentOutcome {
    let context = risk::AnalysisContext {
        signature: &event.signature,
        slot: event.slot,
        observed_unix_ms: now_unix_ms,
    };

    let tx = match provider.get_parsed_transaction(&event.signature).await {
        Ok(tx) => tx,
        Err(error) => {
            tracing::warn!(%error, signature = %event.signature, "getTransaction failed");
            let mut analysis = risk::assess(None, &context, vec![]);
            analysis.not_assessed.push(NotAssessed {
                signal: risk::SignalKind::HolderConcentration,
                reason: format!("endpoint error while reading the creation transaction: {error}"),
            });
            return EnrichmentOutcome {
                analysis,
                provider_ok: false,
            };
        }
    };

    let Some(tx) = tx else {
        return EnrichmentOutcome {
            analysis: risk::assess(None, &context, vec![]),
            provider_ok: true,
        };
    };

    let Some(mint_address) = extract_new_mint(&tx) else {
        return EnrichmentOutcome {
            analysis: risk::assess(None, &context, vec![]),
            provider_ok: true,
        };
    };

    let mint = match provider.get_mint(&mint_address).await {
        Ok(mint) => mint,
        Err(error) => {
            tracing::warn!(%error, %mint_address, "getAccountInfo(mint) failed");
            return EnrichmentOutcome {
                analysis: risk::assess(None, &context, vec![]),
                provider_ok: false,
            };
        }
    };

    let Some(mint) = mint else {
        return EnrichmentOutcome {
            analysis: risk::assess(None, &context, vec![]),
            provider_ok: true,
        };
    };

    let holders = provider
        .get_holders(&mint_address)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|holder| HolderSnapshot {
            address: holder.address,
            amount_raw: holder.amount_raw,
        })
        .collect::<Vec<_>>();

    let input = AnalysisInput {
        mint_address,
        mint,
        holders,
    };
    EnrichmentOutcome {
        analysis: risk::assess(Some(&input), &context, vec![]),
        provider_ok: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn finds_mint_that_appears_post_only() {
        let tx = json!({
            "meta": {
                "preTokenBalances": [
                    { "mint": "SoMeOldMint111", "uiTokenAmount": {} }
                ],
                "postTokenBalances": [
                    { "mint": "SoMeOldMint111", "uiTokenAmount": {} },
                    { "mint": "NewMint111", "uiTokenAmount": {} }
                ]
            }
        });
        assert_eq!(extract_new_mint(&tx).as_deref(), Some("NewMint111"));
    }

    #[test]
    fn wrapped_sol_is_never_the_launched_mint() {
        // Pools are always seeded with WSOL; a creation tx where WSOL is the
        // only "new" balance has no discoverable token mint.
        let tx = json!({
            "meta": {
                "preTokenBalances": [],
                "postTokenBalances": [
                    { "mint": WRAPPED_SOL_MINT, "uiTokenAmount": {} }
                ]
            }
        });
        assert!(extract_new_mint(&tx).is_none());
    }

    #[test]
    fn no_mint_when_nothing_new() {
        let tx = json!({
            "meta": {
                "preTokenBalances": [{ "mint": "SameMint111", "uiTokenAmount": {} }],
                "postTokenBalances": [{ "mint": "SameMint111", "uiTokenAmount": {} }]
            }
        });
        assert!(extract_new_mint(&tx).is_none());
    }
}
