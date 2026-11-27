//! Configuration loading. Values come from `config.toml` next to the app data
//! directory; `config.example.toml` in the repo documents every key.
//!
//! Security note (docs/THREAT_MODEL.md): the only outbound network targets
//! are the RPC `ws_url` and the optional Discord webhook. Neither is fetched
//! from remote content — both are operator-supplied configuration.

use std::path::Path;

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub rpc: RpcConfig,
    #[serde(default)]
    pub monitor: MonitorConfig,
    #[serde(default)]
    pub alerts: AlertConfig,
    #[serde(default)]
    pub storage: StorageConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct RpcConfig {
    /// WebSocket URL of a Solana RPC endpoint (operator-supplied, config-only).
    pub ws_url: String,
}

impl Default for RpcConfig {
    fn default() -> Self {
        Self {
            ws_url: "wss://api.mainnet-beta.solana.com".to_string(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            rpc: RpcConfig::default(),
            monitor: MonitorConfig::default(),
            alerts: AlertConfig::default(),
            storage: StorageConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct MonitorConfig {
    pub desktop_notifications: bool,
    pub min_liquidity_usd: f64,
}

impl Default for MonitorConfig {
    fn default() -> Self {
        Self {
            desktop_notifications: true,
            min_liquidity_usd: 0.0,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct AlertConfig {
    /// Optional Discord webhook URL. Empty = disabled.
    pub discord_webhook_url: String,
}

#[derive(Debug, Clone, Deserialize)]
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
        source: toml::de::Error,
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
    toml::from_str(&text).map_err(|source| ConfigError::Parse { path, source })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_example_config() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let cfg = load(root).expect("example config must parse");
        assert!(cfg.rpc.ws_url.starts_with("wss://"));
        assert!(cfg.monitor.desktop_notifications);
        assert!(cfg.alerts.discord_webhook_url.is_empty());
    }
}
