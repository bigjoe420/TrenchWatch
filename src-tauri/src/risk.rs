//! The risk engine: pure functions over fetched state (docs/ARCHITECTURE.md
//! §3). No network access, no ambient clock — every verdict is reproducible
//! from its receipts, and every signal that cannot be assessed says so
//! explicitly instead of pretending (no black-box scores, no silent gaps).

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::provider::MintAccount;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalKind {
    MintFreezeAuthority,
    LpBurnOrLock,
    DevFreshFunding,
    DevRepeatLauncher,
    HolderConcentration,
    ThinLiquidity,
}

impl SignalKind {
    pub fn field_guide_anchor(self) -> &'static str {
        match self {
            SignalKind::MintFreezeAuthority => "docs/FIELD_GUIDE.md#s1-mint--freeze-authority",
            SignalKind::LpBurnOrLock => "docs/FIELD_GUIDE.md#s2-lp-tokens-not-burned-or-locked",
            SignalKind::DevFreshFunding => {
                "docs/FIELD_GUIDE.md#s3-fresh-or-mixer-funded-dev-wallet"
            }
            SignalKind::DevRepeatLauncher => {
                "docs/FIELD_GUIDE.md#s4-dev-with-a-history-of-launches"
            }
            SignalKind::HolderConcentration => "docs/FIELD_GUIDE.md#s5-holder-concentration",
            SignalKind::ThinLiquidity => "docs/FIELD_GUIDE.md#s6-thin-liquidity",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warn,
    Critical,
}

/// One signal's verdict with its full receipt. This is the only output the
/// engine produces — an aggregated score is *derived* from these, never
/// stored without them.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskFlag {
    pub signal: SignalKind,
    pub severity: Severity,
    /// Plain language, readable by someone who has never used Solana.
    pub explanation: String,
    /// The on-chain data the explanation rests on.
    pub evidence: serde_json::Value,
    pub field_guide_ref: String,
    pub slot: u64,
    pub observed_unix_ms: u64,
}

/// Honest record of a signal we deliberately did not assess.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotAssessed {
    pub signal: SignalKind,
    pub reason: String,
}

/// Signals whose inputs need wallet-history or pool-state enrichment that
/// ships in the Helius phase — recorded so the UI can say *why* there is no
/// verdict rather than leaving a hole.
pub fn deferred_signals() -> Vec<NotAssessed> {
    vec![
        NotAssessed {
            signal: SignalKind::LpBurnOrLock,
            reason: "pool-state parsing ships with the enrichment phase — \
                     not yet verifiable from public-RPC data alone"
                .to_string(),
        },
        NotAssessed {
            signal: SignalKind::DevFreshFunding,
            reason: "dev-wallet funding history requires wallet-history \
                     enrichment — planned, not yet wired"
                .to_string(),
        },
        NotAssessed {
            signal: SignalKind::DevRepeatLauncher,
            reason: "dev-wallet history requires wallet-history enrichment — \
                     planned, not yet wired"
                .to_string(),
        },
        NotAssessed {
            signal: SignalKind::ThinLiquidity,
            reason: "pool balances require pool-state parsing — planned, not \
                     yet wired"
                .to_string(),
        },
    ]
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HolderSnapshot {
    pub address: String,
    pub amount_raw: u64,
}

/// Everything the live signals need, fetched once per launch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnalysisInput {
    pub mint_address: String,
    pub mint: MintAccount,
    pub holders: Vec<HolderSnapshot>,
}

#[derive(Debug, Clone, Copy)]
pub struct AnalysisContext<'a> {
    pub signature: &'a str,
    pub slot: u64,
    pub observed_unix_ms: u64,
}

/// Launchpads whose programs legitimately hold mint/freeze authority over
/// their tokens (bonding-curve custody). Authority held by a *launchpad
/// program* is a design property; authority held by a *wallet* is a risk.
pub const LAUNCHPAD_PROGRAMS: &[(&str, &str)] = &[
    ("6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P", "Pump.fun"),
    ("pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA", "PumpSwap"),
];

pub const TOP10_WARN_PCT: f64 = 50.0;
pub const TOP10_CRITICAL_PCT: f64 = 80.0;

fn launchpad_name(authority: &str) -> Option<&'static str> {
    LAUNCHPAD_PROGRAMS
        .iter()
        .find(|(program, _)| program == &authority)
        .map(|(_, name)| *name)
}

fn assess_authorities(input: &AnalysisInput, context: &AnalysisContext<'_>) -> Option<RiskFlag> {
    let mint_authority = input.mint.mint_authority.as_deref();
    let freeze_authority = input.mint.freeze_authority.as_deref();

    let mint_launchpad = mint_authority.and_then(launchpad_name);
    let freeze_launchpad = freeze_authority.and_then(launchpad_name);

    let (explanation, severity) = match (
        mint_authority,
        mint_launchpad,
        freeze_authority,
        freeze_launchpad,
    ) {
        (None, _, None, _) => return None, // both revoked — the good case
        (Some(_), Some(name), None, _) => (
            format!(
                "Mint authority is held by the {name} program, not a person. \
                 That is the standard custody model for {name} tokens: the program \
                 can mint per its rules, but no individual wallet can print supply. \
                 Freeze authority is revoked."
            ),
            Severity::Info,
        ),
        (None, _, Some(_), Some(name)) => (
            format!(
                "Freeze authority is held by the {name} program. The program can \
                 freeze token accounts until graduation; no individual wallet holds \
                 it. Mint authority is revoked."
            ),
            Severity::Info,
        ),
        (Some(_), Some(name), Some(_), Some(_)) => (
            format!(
                "Both authorities sit with the {name} program — standard launchpad \
                 custody, no individual wallet controls this token."
            ),
            Severity::Info,
        ),
        (Some(authority), None, None, _) => (
            format!(
                "Mint authority is NOT revoked — wallet {authority} can print an \
                 unlimited number of new tokens at any time, diluting every holder. \
                 Freeze authority is revoked."
            ),
            Severity::Critical,
        ),
        (None, _, Some(authority), None) => (
            format!(
                "Freeze authority is NOT revoked — wallet {authority} can freeze \
                 anyone's token accounts at will, blocking them from selling. Mint \
                 authority is revoked."
            ),
            Severity::Critical,
        ),
        (Some(_), _, Some(_), _) => (
            "Both mint AND freeze authorities are active wallets — the deployer can \
             print supply and freeze holders. This is the highest-risk authority \
             configuration."
                .to_string(),
            Severity::Critical,
        ),
    };

    Some(RiskFlag {
        signal: SignalKind::MintFreezeAuthority,
        severity,
        explanation,
        evidence: json!({
            "mint": input.mint_address,
            "mint_authority": mint_authority,
            "freeze_authority": freeze_authority,
            "launchpad_custody": mint_launchpad.or(freeze_launchpad),
            "supply_ui": input.mint.supply_raw as f64 / 10f64.powi(i32::from(input.mint.decimals)),
        }),
        field_guide_ref: SignalKind::MintFreezeAuthority
            .field_guide_anchor()
            .to_string(),
        slot: context.slot,
        observed_unix_ms: context.observed_unix_ms,
    })
}

fn assess_concentration(input: &AnalysisInput, context: &AnalysisContext<'_>) -> Option<RiskFlag> {
    if input.holders.is_empty() || input.mint.supply_raw == 0 {
        return None; // enrich records the not_assessed reason
    }
    let total: f64 = input
        .holders
        .iter()
        .map(|holder| holder.amount_raw as f64)
        .sum();
    let top10: f64 = input
        .holders
        .iter()
        .take(10)
        .map(|holder| holder.amount_raw as f64)
        .sum();
    let top10_pct = (top10 / input.mint.supply_raw as f64) * 100.0;
    let largest_pct = total.max(1.0) / input.mint.supply_raw as f64 * 100.0;

    if top10_pct <= TOP10_WARN_PCT {
        return None;
    }
    let severity = if top10_pct >= TOP10_CRITICAL_PCT {
        Severity::Critical
    } else {
        Severity::Warn
    };
    Some(RiskFlag {
        signal: SignalKind::HolderConcentration,
        severity,
        explanation: format!(
            "The {count} largest token accounts hold {top10_pct:.1}% of the total \
             supply. When a few wallets control most of a token, they can crash the \
             price by selling together. Note: this count can include the pool and \
             lockup accounts — check the holder list below before concluding.",
            count = input.holders.len().min(10),
        ),
        evidence: json!({
            "mint": input.mint_address,
            "top10_pct": top10_pct,
            "largest_account_pct_of_supply": largest_pct,
            "largest_accounts": input.holders.iter().take(10).map(|holder| json!({
                "address": holder.address,
                "pct_of_supply": holder.amount_raw as f64 / input.mint.supply_raw as f64 * 100.0,
            })).collect::<Vec<_>>(),
        }),
        field_guide_ref: SignalKind::HolderConcentration
            .field_guide_anchor()
            .to_string(),
        slot: context.slot,
        observed_unix_ms: context.observed_unix_ms,
    })
}

/// The engine's output for one launch: every flag we could assess, every
/// signal we could not (with reasons), and nothing else.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchAnalysis {
    pub signature: String,
    pub slot: u64,
    pub mint: Option<String>,
    pub flags: Vec<RiskFlag>,
    pub not_assessed: Vec<NotAssessed>,
    pub assessed_at_unix_ms: u64,
}

impl LaunchAnalysis {
    pub fn max_severity(&self) -> Option<Severity> {
        self.flags.iter().map(|flag| flag.severity).max()
    }

    /// One-line, plain-language summary for the table row and alerts.
    /// Never says "safe" — only what was and was not found.
    pub fn verdict_text(&self) -> String {
        if self.flags.is_empty() {
            let pending = self.not_assessed.len();
            return format!(
                "No risk flags found as of slot {} — not a guarantee of safety. \
                 {pending} signal(s) not yet assessed.",
                self.slot
            );
        }
        let criticals = self
            .flags
            .iter()
            .filter(|flag| flag.severity == Severity::Critical)
            .count();
        let warnings = self
            .flags
            .iter()
            .filter(|flag| flag.severity == Severity::Warn)
            .count();
        let infos = self
            .flags
            .iter()
            .filter(|flag| flag.severity == Severity::Info)
            .count();
        let mut parts = Vec::new();
        if criticals > 0 {
            parts.push(format!("{criticals} critical"));
        }
        if warnings > 0 {
            parts.push(format!("{warnings} warning"));
        }
        if infos > 0 {
            parts.push(format!("{infos} info"));
        }
        format!(
            "{} raised as of slot {} — open the evidence for details.",
            parts.join(", "),
            self.slot
        )
    }
}

/// Assess everything that can be assessed from `input`. Signals whose data
/// is missing here are expected to arrive already-present in
/// `extra_not_assessed` (the deferred-signals list) or are added here.
pub fn assess(
    input: Option<&AnalysisInput>,
    context: &AnalysisContext<'_>,
    extra_not_assessed: Vec<NotAssessed>,
) -> LaunchAnalysis {
    let mut flags = Vec::new();
    let mut not_assessed = deferred_signals();

    match input {
        Some(input) => {
            if let Some(flag) = assess_authorities(input, context) {
                flags.push(flag);
            }
            if input.holders.is_empty() {
                not_assessed.push(NotAssessed {
                    signal: SignalKind::HolderConcentration,
                    reason: "endpoint returned no token accounts for this mint".to_string(),
                });
            } else if let Some(flag) = assess_concentration(input, context) {
                flags.push(flag);
            }
        }
        None => {
            not_assessed.push(NotAssessed {
                signal: SignalKind::MintFreezeAuthority,
                reason: "could not identify the token mint from the creation \
                        transaction (tx may be pruned or parsing failed)"
                    .to_string(),
            });
            not_assessed.push(NotAssessed {
                signal: SignalKind::HolderConcentration,
                reason: "mint unknown, so holder distribution could not be fetched".to_string(),
            });
        }
    }
    not_assessed.extend(extra_not_assessed);

    LaunchAnalysis {
        signature: context.signature.to_string(),
        slot: context.slot,
        mint: input.map(|input| input.mint_address.clone()),
        flags,
        not_assessed,
        assessed_at_unix_ms: context.observed_unix_ms,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mint_with(mint_authority: Option<&str>, freeze_authority: Option<&str>) -> MintAccount {
        MintAccount {
            supply_raw: 1_000_000_000_000,
            decimals: 6,
            mint_authority: mint_authority.map(str::to_string),
            freeze_authority: freeze_authority.map(str::to_string),
        }
    }

    fn context() -> AnalysisContext<'static> {
        AnalysisContext {
            signature: "testsig",
            slot: 100,
            observed_unix_ms: 1_700_000_000_000,
        }
    }

    fn input(mint: MintAccount, holders: Vec<HolderSnapshot>) -> AnalysisInput {
        AnalysisInput {
            mint_address: "mint111".to_string(),
            mint,
            holders,
        }
    }

    fn holder(amount: u64) -> HolderSnapshot {
        HolderSnapshot {
            address: "holder111".to_string(),
            amount_raw: amount,
        }
    }

    #[test]
    fn revoked_authorities_raise_no_flag() {
        let input = input(mint_with(None, None), vec![holder(10)]);
        let analysis = assess(Some(&input), &context(), vec![]);
        assert!(analysis
            .flags
            .iter()
            .all(|flag| flag.signal != SignalKind::MintFreezeAuthority));
        assert!(analysis.verdict_text().contains("No risk flags found"));
    }

    #[test]
    fn wallet_mint_authority_is_critical_with_receipt() {
        let input = input(mint_with(Some("BadWallet111"), None), vec![holder(10)]);
        let analysis = assess(Some(&input), &context(), vec![]);
        let flag = analysis
            .flags
            .iter()
            .find(|flag| flag.signal == SignalKind::MintFreezeAuthority)
            .expect("authority flag");
        assert_eq!(flag.severity, Severity::Critical);
        assert!(flag.explanation.contains("BadWallet111"));
        assert_eq!(
            flag.evidence["mint_authority"],
            serde_json::json!("BadWallet111")
        );
        assert!(flag.field_guide_ref.contains("FIELD_GUIDE"));
    }

    #[test]
    fn launchpad_custody_is_info_not_critical() {
        let input = input(
            mint_with(Some("6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P"), None),
            vec![holder(10)],
        );
        let analysis = assess(Some(&input), &context(), vec![]);
        let flag = analysis
            .flags
            .iter()
            .find(|flag| flag.signal == SignalKind::MintFreezeAuthority)
            .expect("authority flag");
        assert_eq!(flag.severity, Severity::Info);
        assert!(flag.explanation.contains("Pump.fun"));
    }

    #[test]
    fn concentrated_supply_raises_warn_and_critical() {
        // 70% across the top 10 -> Warn band (50-80%).
        let warn_holders: Vec<HolderSnapshot> = (0..10)
            .map(|index| HolderSnapshot {
                address: format!("holder{index}"),
                amount_raw: 70_000_000_000, // 7% each -> 70%
            })
            .collect();
        let analysis = assess(
            Some(&input(mint_with(None, None), warn_holders)),
            &context(),
            vec![],
        );
        let flag = analysis
            .flags
            .iter()
            .find(|flag| flag.signal == SignalKind::HolderConcentration)
            .expect("warn concentration flag");
        assert_eq!(flag.severity, Severity::Warn);
        assert!(flag.explanation.contains("70.0%"));
        assert_eq!(flag.evidence["top10_pct"], serde_json::json!(70.0));

        // 90% across the top 10 -> Critical band (>= 80%).
        let critical_holders: Vec<HolderSnapshot> = (0..10)
            .map(|index| HolderSnapshot {
                address: format!("holder{index}"),
                amount_raw: 90_000_000_000, // 9% each -> 90%
            })
            .collect();
        let analysis = assess(
            Some(&input(mint_with(None, None), critical_holders)),
            &context(),
            vec![],
        );
        let flag = analysis
            .flags
            .iter()
            .find(|flag| flag.signal == SignalKind::HolderConcentration)
            .expect("critical concentration flag");
        assert_eq!(flag.severity, Severity::Critical);
    }

    #[test]
    fn dispersed_supply_raises_no_flag() {
        let holders: Vec<HolderSnapshot> = (0..20)
            .map(|index| HolderSnapshot {
                address: format!("holder{index}"),
                amount_raw: 1_000_000_000, // 0.1% each
            })
            .collect();
        let input = input(mint_with(None, None), holders);
        let analysis = assess(Some(&input), &context(), vec![]);
        assert!(analysis
            .flags
            .iter()
            .all(|flag| flag.signal != SignalKind::HolderConcentration));
    }

    #[test]
    fn missing_input_reports_not_assessed_not_silence() {
        let analysis = assess(None, &context(), vec![]);
        assert!(analysis.flags.is_empty());
        let reasons: Vec<SignalKind> = analysis.not_assessed.iter().map(|n| n.signal).collect();
        assert!(reasons.contains(&SignalKind::MintFreezeAuthority));
        assert!(reasons.contains(&SignalKind::HolderConcentration));
        // deferred signals are recorded too, with reasons
        assert!(reasons.contains(&SignalKind::LpBurnOrLock));
        assert!(reasons.contains(&SignalKind::DevFreshFunding));
        assert!(reasons.contains(&SignalKind::DevRepeatLauncher));
        assert!(reasons.contains(&SignalKind::ThinLiquidity));
        assert!(analysis.verdict_text().contains("not yet assessed"));
    }

    #[test]
    fn verdict_text_never_claims_safe() {
        let input = input(mint_with(None, None), vec![holder(10)]);
        let analysis = assess(Some(&input), &context(), vec![]);
        let text = analysis.verdict_text();
        assert!(
            !text.to_lowercase().contains("safe")
                || text.to_lowercase().contains("not a guarantee")
        );
    }
}
