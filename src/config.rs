use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct Config {
    pub bitcoin: BitcoinConfig,
    pub signals: SignalConfig,
    pub ui: UiConfig,
    pub database: DatabaseConfig,
    pub notifications: NotificationConfig,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct BitcoinConfig {
    pub rpc_host: String,
    pub rpc_port: u16,
    pub rpc_user: Option<String>,
    pub rpc_password: Option<String>,
    pub zmq_rawtx: String,
    pub zmq_hashblock: String,
    pub zmq_sequence: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct SignalConfig {
    pub weights: HashMap<String, f64>,
    pub min_score_persist: f64,
    pub alert_thresholds: AlertThresholds,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct AlertThresholds {
    pub critical: f64,
    pub high: f64,
    pub medium: f64,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct UiConfig {
    pub max_feed_entries: usize,
    pub stats_update_interval_txs: usize,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct NotificationConfig {
    pub enabled: bool,
    pub min_score: f64,
    pub cooldown_seconds: u64,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct DatabaseConfig {
    pub path: String,
    pub exchange_csv: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            bitcoin: BitcoinConfig::default(),
            signals: SignalConfig::default(),
            ui: UiConfig::default(),
            database: DatabaseConfig::default(),
            notifications: NotificationConfig::default(),
        }
    }
}

impl Default for BitcoinConfig {
    fn default() -> Self {
        Self {
            rpc_host: "127.0.0.1".into(),
            rpc_port: 8332,
            rpc_user: None,
            rpc_password: None,
            zmq_rawtx: "tcp://127.0.0.1:28333".into(),
            zmq_hashblock: "tcp://127.0.0.1:28332".into(),
            zmq_sequence: Some("tcp://127.0.0.1:28336".into()),
        }
    }
}

impl Default for SignalConfig {
    fn default() -> Self {
        Self {
            weights: HashMap::new(),
            min_score_persist: 10.0,
            alert_thresholds: AlertThresholds::default(),
        }
    }
}

impl Default for AlertThresholds {
    fn default() -> Self {
        Self {
            critical: 80.0,
            high: 60.0,
            medium: 40.0,
        }
    }
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            max_feed_entries: 500,
            stats_update_interval_txs: 100,
        }
    }
}

impl Default for NotificationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            min_score: 60.0,
            cooldown_seconds: 30,
        }
    }
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            path: "data/utxo_cache.db".into(),
            exchange_csv: Some("data/exchange_addresses.csv".into()),
        }
    }
}

impl Config {
    /// Load config from a TOML file. Falls back to defaults if file doesn't exist.
    pub fn load(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref();
        if !path.exists() {
            tracing::info!("Config file {} not found, using defaults", path.display());
            return Self::default();
        }
        match std::fs::read_to_string(path) {
            Ok(contents) => match toml::from_str(&contents) {
                Ok(config) => {
                    tracing::info!("Config loaded from {}", path.display());
                    config
                }
                Err(e) => {
                    tracing::warn!("Failed to parse {}: {e}, using defaults", path.display());
                    Self::default()
                }
            },
            Err(e) => {
                tracing::warn!("Failed to read {}: {e}, using defaults", path.display());
                Self::default()
            }
        }
    }
}
