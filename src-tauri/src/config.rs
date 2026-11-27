//! Configuration loading. Values come from `config.toml` next to the app data
//! directory; `config.example.toml` in the repo documents every key.
//!
//! Security note (docs/THREAT_MODEL.md): the only outbound network targets
//! are the RPC `ws_url`, the optional Discord webhook, and — when an API
//! key is configured — `mainnet.helius-rpc.com` for metered read-only
//! enrichment. None is fetched from remote content; all are
//! operator-supplied configuration.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::risk::Severity;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    pub rpc: RpcConfig,
    #[serde(default)]
    pub monitor: MonitorConfig,
    #[serde(default)]
    pub alerts: AlertConfig,
    #[serde(default)]
    pub storage: StorageConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RpcConfig {
    /// WebSocket URL of a Solana RPC endpoint (operator-supplied, config-only).
    pub ws_url: String,
    /// Optional provider API key (e.g. Helius). Empty = public endpoint only.
    pub api_key: String,
    /// Daily credit budget for enrichment calls (0 = unlimited).
    pub daily_credit_cap: u64,
}

impl Default for RpcConfig {
    fn default() -> Self {
        Self {
            ws_url: "wss://api.mainnet-beta.solana.com".to_string(),
            api_key: String::new(),
            daily_credit_cap: 100_000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MonitorConfig {
    pub min_liquidity_usd: f64,
}

impl Default for MonitorConfig {
    fn default() -> Self {
        Self {
            min_liquidity_usd: 0.0,
        }
    }
}

/// Everything alert-related, editable from the in-app settings panel.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AlertConfig {
    /// Desktop notifications for flagged launches at or above `min_severity`.
    pub desktop_notifications: bool,
    /// Lowest severity that triggers an alert. Default: critical only.
    pub min_severity: Severity,
    /// Optional Discord webhook URL. Empty = disabled.
    pub discord_webhook_url: String,
    /// Optional separate webhook for flag (risk) alerts. Empty = flag alerts
    /// fall back to `discord_webhook_url`. Graduation alerts always use
    /// `discord_webhook_url` so the two channels stay cleanly separated.
    pub discord_critical_webhook_url: String,
    /// Graduation alerts: a token moving from its launchpad to a real DEX
    /// pool (e.g. Pump.fun → PumpSwap) is headline news, so it alerts even
    /// when no risk flag fired. Default: on.
    pub graduation_alerts: bool,
    /// Set once the first-run setup wizard has been completed (or skipped).
    /// Default false so a fresh install is walked through the Helius key and
    /// Discord webhook before the dashboard takes over.
    pub setup_completed: bool,
}

impl Default for AlertConfig {
    fn default() -> Self {
        Self {
            desktop_notifications: true,
            min_severity: Severity::Critical,
            discord_webhook_url: String::new(),
            discord_critical_webhook_url: String::new(),
            graduation_alerts: true,
            setup_completed: false,
        }
    }
}

impl AlertConfig {
    /// A webhook URL is accepted only when empty or an https Discord webhook.
    pub fn validate(&self) -> Result<(), String> {
        for url in [&self.discord_webhook_url, &self.discord_critical_webhook_url] {
            let url = url.trim();
            if url.is_empty() {
                continue;
            }
            if !(url.starts_with("https://discord.com/api/webhooks/")
                || url.starts_with("https://discordapp.com/api/webhooks/"))
            {
                return Err(
                    "Discord webhook must start with https://discord.com/api/webhooks/".to_string(),
                );
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct StorageConfig {
    pub store_dir: String,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            store_dir: "store".to_string(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("config parse error in {path}: {source}")]
    Parse {
        path: std::path::PathBuf,
        #[source]
        source: Box<toml::de::Error>,
    },
    #[error("config write error in {path}: {source}")]
    Serde {
        path: std::path::PathBuf,
        #[source]
        source: Box<toml::ser::Error>,
    },
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Load `config.toml` from the given directory, falling back to
/// `config.example.toml`, and finally to built-in defaults — the app never
/// requires a config file or secrets to start.
pub fn load(app_data_dir: &Path) -> Result<Config, ConfigError> {
    let real = app_data_dir.join("config.toml");
    let example = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .join("config.example.toml");
    let path = if real.exists() {
        real
    } else if example.exists() {
        example
    } else {
        return Ok(Config::default());
    };

    let text = std::fs::read_to_string(&path)?;
    toml::from_str(&text).map_err(|source| ConfigError::Parse {
        path,
        source: Box::new(source),
    })
}

impl Config {
    /// Persist to `config.toml` in the given directory (file-backed,
    /// gitignored; the settings panel edits through this path).
    pub fn save(&self, app_data_dir: &Path) -> Result<PathBuf, ConfigError> {
        std::fs::create_dir_all(app_data_dir)?;
        let path = app_data_dir.join("config.toml");
        let text = toml::to_string_pretty(self).map_err(|source| ConfigError::Serde {
            path: path.clone(),
            source: Box::new(source),
        })?;
        std::fs::write(&path, text)?;
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_example_config() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let cfg = load(root).expect("example config must parse");
        assert!(cfg.rpc.ws_url.starts_with("wss://"));
        assert!(cfg.alerts.desktop_notifications);
        assert_eq!(cfg.alerts.min_severity, Severity::Critical);
        assert!(cfg.alerts.discord_webhook_url.is_empty());
        assert!(!cfg.alerts.setup_completed); // fresh installs get the wizard
    }

    #[test]
    fn save_roundtrips_through_load() {
        let dir = std::env::temp_dir().join(format!("tw-config-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let cfg = Config {
            alerts: AlertConfig {
                discord_webhook_url: "https://discord.com/api/webhooks/123/abc".to_string(),
                min_severity: Severity::Warn,
                ..AlertConfig::default()
            },
            ..Config::default()
        };
        let path = cfg.save(&dir).expect("save");
        assert!(path.ends_with("config.toml"));
        let loaded = load(&dir).expect("reload");
        assert_eq!(
            loaded.alerts.discord_webhook_url,
            "https://discord.com/api/webhooks/123/abc"
        );
        assert_eq!(loaded.alerts.min_severity, Severity::Warn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn webhook_validation_accepts_only_discord_https() {
        let mut cfg = AlertConfig {
            discord_webhook_url: "https://discord.com/api/webhooks/1/x".into(),
            ..AlertConfig::default()
        };
        assert!(cfg.validate().is_ok());
        cfg.discord_webhook_url = "http://evil.example.com/hook".into();
        assert!(cfg.validate().is_err());
        cfg.discord_webhook_url.clear();
        assert!(cfg.validate().is_ok());
    }
}
