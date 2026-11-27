//! Fetch-and-assess glue: turns a raw [`LaunchEvent`] into a
//! [`LaunchAnalysis`] by pulling the few read-only facts the engine needs
//! (creation tx → mint → mint account + largest holders).
//!
//! All network access lives here (and in `provider`) — `risk` stays pure.

use serde_json::Value;

use crate::listener::LaunchEvent;
use crate::provider::{ChainProvider, WsProvider};
use crate::risk::{self, AnalysisInput, HolderSnapshot, LaunchAnalysis, NotAssessed, PriorLaunch};

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

/// Identify the wallet that paid for the creation tx (the deployer): the
/// account key the parsed tx marks as a signer. jsonParsed txs list keys as
/// objects (`{pubkey, signer: true}`); the plain-string fallback covers
/// other encodings, where the fee payer — always a signer — is first.
fn extract_signer(transaction: &Value) -> Option<String> {
    let keys = transaction
        .get("transaction")?
        .get("message")?
        .get("accountKeys")?
        .as_array()?;
    for key in keys {
        if key.get("signer").and_then(Value::as_bool).unwrap_or(false) {
            return key
                .get("pubkey")
                .and_then(Value::as_str)
                .map(str::to_string);
        }
    }
    keys.first().and_then(Value::as_str).map(str::to_string)
}

pub async fn analyze_launch(
    provider: &mut WsProvider,
    event: &LaunchEvent,
    now_unix_ms: u64,
    wallet_history: &(dyn Fn(&str) -> Vec<PriorLaunch> + Sync),
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

    let signer = extract_signer(&tx);

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

    // Local reputation: everything this wallet launched before, per our own
    // stored receipts — no third-party score involved.
    let history = signer.as_deref().map(wallet_history).unwrap_or_default();

    let input = AnalysisInput {
        mint_address,
        mint,
        holders,
        dev_wallet: signer,
    };
    EnrichmentOutcome {
        analysis: risk::assess_with_history(Some(&input), &context, &history, vec![]),
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

    #[test]
    fn signer_is_the_flagged_key_in_parsed_form() {
        let tx = json!({
            "transaction": {
                "message": {
                    "accountKeys": [
                        { "pubkey": "DeployerWallet111", "signer": true, "writable": true },
                        { "pubkey": "LaunchpadProgram111", "signer": false, "writable": true },
                        { "pubkey": "MintAccount111", "signer": false, "writable": false }
                    ]
                }
            }
        });
        assert_eq!(extract_signer(&tx).as_deref(), Some("DeployerWallet111"));
    }

    #[test]
    fn signer_falls_back_to_first_key_in_string_form() {
        let tx = json!({
            "transaction": {
                "message": {
                    "accountKeys": ["DeployerWallet111", "Other111"]
                }
            }
        });
        assert_eq!(extract_signer(&tx).as_deref(), Some("DeployerWallet111"));
    }

    #[test]
    fn no_signer_without_transaction_body() {
        let tx = json!({ "meta": {} });
        assert!(extract_signer(&tx).is_none());
    }
}
