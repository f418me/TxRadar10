pub mod zmq_sub;

use reqwest::Client;
use serde_json::{json, Value};
use std::path::PathBuf;

/// Simple Bitcoin Core JSON-RPC client.
#[derive(Clone)]
pub struct BitcoinRpc {
    url: String,
    client: Client,
    auth: String, // base64 encoded user:pass
}

impl BitcoinRpc {
    pub fn new(host: &str, port: u16, user: &str, pass: &str) -> Self {
        use base64::{engine::general_purpose::STANDARD, Engine};
        let auth = STANDARD.encode(format!("{user}:{pass}"));
        Self {
            url: format!("http://{host}:{port}"),
            client: Client::new(),
            auth,
        }
    }

    /// Create RPC client from bitcoin.conf or cookie auth, with host/port from config.
    pub fn from_config_with_defaults(host: &str, port: u16) -> Self {
        let host = host.to_string();
        let port = port;

        // Try cookie auth first
        let cookie_path = dirs_cookie_path();
        if let Ok(cookie) = std::fs::read_to_string(&cookie_path) {
            let cookie = cookie.trim();
            if let Some((_user, _pass)) = cookie.split_once(':') {
                tracing::info!("Using cookie auth from {}", cookie_path.display());
                return Self::new(&host, port, _user, _pass);
            }
        }

        // Try bitcoin.conf
        let conf_path = bitcoin_conf_path();
        if let Ok(contents) = std::fs::read_to_string(&conf_path) {
            let mut user = None;
            let mut pass = None;
            for line in contents.lines() {
                let line = line.trim();
                if let Some(val) = line.strip_prefix("rpcuser=") {
                    user = Some(val.to_string());
                }
                if let Some(val) = line.strip_prefix("rpcpassword=") {
                    pass = Some(val.to_string());
                }
                if let Some(val) = line.strip_prefix("rpcport=") {
                    if let Ok(p) = val.parse::<u16>() {
                        return Self::new(
                            &host,
                            p,
                            &user.unwrap_or_else(|| "bitcoinrpc".into()),
                            &pass.unwrap_or_default(),
                        );
                    }
                }
            }
            if let (Some(u), Some(p)) = (user, pass) {
                tracing::info!("Using RPC credentials from bitcoin.conf");
                return Self::new(&host, port, &u, &p);
            }
        }

        // Fallback defaults
        tracing::warn!("Using default RPC credentials (bitcoinrpc)");
        Self::new(&host, port, "bitcoinrpc", "bitcoinrpc")
    }

    pub async fn call(&self, method: &str, params: Vec<Value>) -> Result<Value, RpcError> {
        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params,
        });

        let resp = self
            .client
            .post(&self.url)
            .header("Authorization", format!("Basic {}", self.auth))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(RpcError::Http)?;

        let json: Value = resp.json().await.map_err(RpcError::Http)?;

        if let Some(err) = json.get("error").and_then(|e| {
            if e.is_null() {
                None
            } else {
                Some(e.clone())
            }
        }) {
            return Err(RpcError::Rpc(err));
        }

        Ok(json["result"].clone())
    }

    /// Get raw transaction with optional verbosity.
    pub async fn getrawtransaction(
        &self,
        txid: &str,
        verbose: bool,
    ) -> Result<Value, RpcError> {
        self.call(
            "getrawtransaction",
            vec![json!(txid), json!(verbose)],
        )
        .await
    }

    /// Get mempool info (size, bytes, usage, fees).
    #[allow(dead_code)]
    pub async fn getmempoolinfo(&self) -> Result<Value, RpcError> {
        self.call("getmempoolinfo", vec![]).await
    }

    /// Get blockchain info (chain, blocks, headers, etc.).
    #[allow(dead_code)]
    pub async fn getblockchaininfo(&self) -> Result<Value, RpcError> {
        self.call("getblockchaininfo", vec![]).await
    }
}

fn dirs_cookie_path() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        dirs_home().join("Library/Application Support/Bitcoin/.cookie")
    }
    #[cfg(target_os = "linux")]
    {
        dirs_home().join(".bitcoin/.cookie")
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        dirs_home().join(".bitcoin/.cookie")
    }
}

fn bitcoin_conf_path() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        dirs_home().join("Library/Application Support/Bitcoin/bitcoin.conf")
    }
    #[cfg(target_os = "linux")]
    {
        dirs_home().join(".bitcoin/bitcoin.conf")
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        dirs_home().join(".bitcoin/bitcoin.conf")
    }
}

fn dirs_home() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"))
}

#[derive(Debug)]
pub enum RpcError {
    Http(reqwest::Error),
    Rpc(Value),
}

impl std::fmt::Display for RpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RpcError::Http(e) => write!(f, "HTTP error: {e}"),
            RpcError::Rpc(e) => write!(f, "RPC error: {e}"),
        }
    }
}

impl std::error::Error for RpcError {}
