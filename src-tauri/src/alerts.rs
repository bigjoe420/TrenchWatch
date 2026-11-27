//! Alert fan-out: after the analysis stage persists a launch's verdict,
//! fire a desktop notification and/or a Discord webhook when the worst
//! severity meets the configured threshold (M5).
//!
//! Receipts discipline applies here too: the alert text is derived from the
//! same flag explanations the dashboard shows — never a bare score, never a
//! paraphrase that could drift from the evidence.

use std::collections::HashSet;
use std::sync::Mutex;
use std::time::Duration;

use serde_json::json;
use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

use crate::config::AlertConfig;
use crate::risk::{LaunchAnalysis, Severity};

/// In-memory alert configuration; the settings panel updates this live and
/// persists to `config.toml` at the same time.
pub type SharedAlertConfig = std::sync::Arc<Mutex<AlertConfig>>;

const DEDUPE_CAP: usize = 10_000;

pub struct Alerts {
    config: SharedAlertConfig,
    app: AppHandle,
    http: reqwest::Client,
    /// Signatures already alerted on, so a re-assessment never double-fires.
    fired: Mutex<HashSet<String>>,
}

impl Alerts {
    pub fn new(config: SharedAlertConfig, app: AppHandle) -> Self {
        Self {
            config,
            app,
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .expect("alert http client"),
            fired: Mutex::new(HashSet::new()),
        }
    }

    /// Evaluate one analysis and fire every configured channel that qualifies.
    pub async fn notify(&self, analysis: &LaunchAnalysis) {
        let config = match self.config.lock() {
            Ok(config) => config.clone(),
            Err(_) => return,
        };
        if !should_alert(analysis, &config) {
            return;
        }
        if !self.mark_fired(&analysis.signature) {
            return; // already alerted for this signature
        }
        tracing::info!(signature = %analysis.signature, "alert fired");

        if config.desktop_notifications {
            self.desktop(analysis);
        }
        if !config.discord_webhook_url.is_empty() {
            self.discord(analysis, &config.discord_webhook_url).await;
        }
    }

    fn mark_fired(&self, signature: &str) -> bool {
        let mut fired = match self.fired.lock() {
            Ok(fired) => fired,
            Err(_) => return false,
        };
        if !fired.insert(signature.to_string()) {
            return false;
        }
        if fired.len() > DEDUPE_CAP {
            fired.clear(); // crude but bounded; re-firing after 10k is acceptable
        }
        true
    }

    fn desktop(&self, analysis: &LaunchAnalysis) {
        let worst = worst_severity(analysis).unwrap_or(Severity::Info);
        let mint = analysis
            .mint
            .as_deref()
            .map(short)
            .unwrap_or_else(|| "unknown mint".to_string());
        let title = format!("{} — {}", severity_word(worst), mint);
        let body = analysis
            .flags
            .iter()
            .max_by_key(|flag| flag.severity)
            .map(|flag| flag.explanation.clone())
            .unwrap_or_else(|| format!("No risk flags found as of slot {}.", analysis.slot));

        let result = self
            .app
            .notification()
            .builder()
            .title(&title)
            .body(truncate(&body, 220))
            .show();
        if let Err(error) = result {
            tracing::warn!(%error, "desktop notification failed");
        }
    }

    async fn discord(&self, analysis: &LaunchAnalysis, webhook: &str) {
        let payload = discord_payload(analysis);
        let result = self.http.post(webhook).json(&payload).send().await;
        match result {
            Ok(response) if response.status().is_success() => {}
            Ok(response) => {
                tracing::warn!(status = %response.status(), "discord webhook rejected alert");
            }
            Err(error) => {
                tracing::warn!(%error, "discord webhook unreachable");
            }
        }
    }
}

/// Pure gate: alerts fire only when the worst flag severity reaches the
/// configured threshold. No flags → no alert, whatever the threshold.
pub fn should_alert(analysis: &LaunchAnalysis, config: &AlertConfig) -> bool {
    match worst_severity(analysis) {
        Some(severity) => severity >= config.min_severity,
        None => false,
    }
}

pub fn worst_severity(analysis: &LaunchAnalysis) -> Option<Severity> {
    analysis.flags.iter().map(|flag| flag.severity).max()
}

/// `abcdef…wxyz` display form for base58 addresses (ASCII-safe slicing).
fn short(address: &str) -> String {
    if address.len() > 12 {
        format!("{}…{}", &address[..6], &address[address.len() - 4..])
    } else {
        address.to_string()
    }
}

fn severity_word(severity: Severity) -> &'static str {
    match severity {
        Severity::Critical => "CRITICAL launch flagged",
        Severity::Warn => "Warning on new launch",
        Severity::Info => "Note on new launch",
    }
}

/// Discord renders `<>`, `&`, and markdown — escape so receipt text arrives
/// verbatim instead of turning into invisible links or formatting.
pub fn escape_discord(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let cut: String = text.chars().take(max_chars.saturating_sub(1)).collect();
    format!("{cut}…")
}

fn discord_payload(analysis: &LaunchAnalysis) -> serde_json::Value {
    let worst = worst_severity(analysis).unwrap_or(Severity::Info);
    let color = match worst {
        Severity::Critical => 0xff4d6d,
        Severity::Warn => 0xffa100,
        Severity::Info => 0x22d3ee,
    };
    let headline = analysis
        .flags
        .iter()
        .max_by_key(|flag| flag.severity)
        .map(|flag| escape_discord(&flag.explanation))
        .unwrap_or_else(|| format!("No risk flags found as of slot {}.", analysis.slot));
    let pending = analysis.not_assessed.len();

    json!({
        "username": "TrenchWatch",
        "embeds": [{
            "title": format!("{} — slot {}", severity_word(worst), analysis.slot),
            "description": headline,
            "color": color,
            "fields": [
                {"name": "token mint", "value": escape_discord(analysis.mint.as_deref().unwrap_or("not identified")), "inline": true},
                {"name": "not yet assessed", "value": format!("{pending} signal(s)"), "inline": true},
                {"name": "signature", "value": escape_discord(&analysis.signature)},
            ],
            "footer": {"text": "Verdict derived from on-chain receipts — never a bare score."}
        }]
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::MintAccount;
    use crate::risk::{AnalysisContext, AnalysisInput, HolderSnapshot};

    fn analysis_with(severity: Severity) -> LaunchAnalysis {
        let input = AnalysisInput {
            mint_address: "mint111".to_string(),
            mint: MintAccount {
                supply_raw: 1_000_000_000_000,
                decimals: 6,
                mint_authority: Some("auth".to_string()),
                freeze_authority: None,
            },
            holders: vec![HolderSnapshot {
                address: "holder".to_string(),
                amount_raw: 500_000,
            }],
            dev_wallet: Some("auth".to_string()),
        };
        let context = AnalysisContext {
            signature: "sig",
            slot: 42,
            observed_unix_ms: 1_700_000_000_000,
        };
        let mut analysis = crate::risk::assess(Some(&input), &context, vec![]);
        analysis.flags[0].severity = severity;
        analysis
    }

    #[test]
    fn critical_triggers_at_critical_threshold() {
        let analysis = analysis_with(Severity::Critical);
        let mut config = AlertConfig::default();
        assert!(should_alert(&analysis, &config));
        config.min_severity = Severity::Info;
        assert!(should_alert(&analysis, &config));
    }

    #[test]
    fn threshold_excludes_lower_severities() {
        let analysis = analysis_with(Severity::Warn);
        let config = AlertConfig::default(); // critical only
        assert!(!should_alert(&analysis, &config));
    }

    #[test]
    fn no_flags_never_alerts() {
        let context = AnalysisContext {
            signature: "sig",
            slot: 42,
            observed_unix_ms: 1_700_000_000_000,
        };
        let analysis = crate::risk::assess(None, &context, vec![]);
        let config = AlertConfig::default();
        assert!(!should_alert(&analysis, &config));
    }

    #[test]
    fn discord_escapes_markup_and_builds_receipt_fields() {
        assert_eq!(escape_discord("<a>&b"), "&lt;a&gt;&amp;b");
        // A signature laced with Discord markup proves payload-level escaping.
        let mut analysis = analysis_with(Severity::Critical);
        analysis.signature = "<sig>&".to_string();
        let payload = discord_payload(&analysis);
        let text = payload.to_string();
        assert!(text.contains("&lt;sig&gt;&amp;"), "payload must be escaped");
        assert!(text.contains("token mint"));
        assert!(text.contains("on-chain receipts"));
    }
}
