pub mod zmq_sub;

use reqwest::Client;
use serde_json::{json, Value};

/// Simple Bitcoin Core JSON-RPC client.
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
    pub async fn getmempoolinfo(&self) -> Result<Value, RpcError> {
        self.call("getmempoolinfo", vec![]).await
    }

    /// Get blockchain info (chain, blocks, headers, etc.).
    pub async fn getblockchaininfo(&self) -> Result<Value, RpcError> {
        self.call("getblockchaininfo", vec![]).await
    }
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
