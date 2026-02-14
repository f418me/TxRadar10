use tokio::sync::mpsc;
use tracing::{error, info, warn};

use crate::core::MempoolEvent;

/// ZMQ subscriber configuration.
pub struct ZmqConfig {
    pub rawtx_endpoint: String,
    pub hashblock_endpoint: String,
    /// ZMQ sequence endpoint for tx removal notifications.
    /// Requires Bitcoin Core `-zmqpubsequence=tcp://...` config.
    /// TODO: Subscribe to `sequence` topic to receive TxRemoved events
    /// with removal reason (confirmed/replaced/evicted). This enables
    /// full mempool state tracking including RBF replacement chains.
    #[allow(dead_code)]
    pub sequence_endpoint: Option<String>,
}

impl Default for ZmqConfig {
    fn default() -> Self {
        Self {
            rawtx_endpoint: "tcp://127.0.0.1:28333".into(),
            hashblock_endpoint: "tcp://127.0.0.1:28332".into(),
            sequence_endpoint: None, // TODO: enable when zmqpubsequence is configured
        }
    }
}

/// Start ZMQ subscriber in a blocking thread (zmq crate is synchronous).
/// Sends MempoolEvents into the provided channel.
pub fn start_zmq_subscriber(
    config: ZmqConfig,
    tx: mpsc::UnboundedSender<MempoolEvent>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let ctx = zmq::Context::new();

        // Subscribe to rawtx
        let rawtx_sock = ctx.socket(zmq::SUB).expect("failed to create rawtx socket");
        rawtx_sock
            .connect(&config.rawtx_endpoint)
            .unwrap_or_else(|e| panic!("failed to connect rawtx at {}: {e}", config.rawtx_endpoint));
        rawtx_sock.set_subscribe(b"rawtx").expect("subscribe rawtx");
        info!(endpoint = %config.rawtx_endpoint, "ZMQ rawtx subscriber connected");

        // Subscribe to hashblock
        let hashblock_sock = ctx.socket(zmq::SUB).expect("failed to create hashblock socket");
        hashblock_sock
            .connect(&config.hashblock_endpoint)
            .unwrap_or_else(|e| panic!("failed to connect hashblock at {}: {e}", config.hashblock_endpoint));
        hashblock_sock.set_subscribe(b"hashblock").expect("subscribe hashblock");
        info!(endpoint = %config.hashblock_endpoint, "ZMQ hashblock subscriber connected");

        // Poll both sockets
        let mut items = [
            rawtx_sock.as_poll_item(zmq::POLLIN),
            hashblock_sock.as_poll_item(zmq::POLLIN),
        ];

        loop {
            match zmq::poll(&mut items, 1000) {
                Ok(_) => {}
                Err(e) => {
                    error!("ZMQ poll error: {e}");
                    std::thread::sleep(std::time::Duration::from_secs(1));
                    continue;
                }
            }

            // Check rawtx
            if items[0].is_readable() {
                match rawtx_sock.recv_multipart(zmq::DONTWAIT) {
                    Ok(msg) if msg.len() >= 2 && msg[0] == b"rawtx" => {
                        let body = &msg[1];
                        use bitcoin::hashes::{sha256d, Hash};
                        let txid_hash = sha256d::Hash::hash(body);
                        let mut txid = [0u8; 32];
                        txid.copy_from_slice(txid_hash.as_ref());

                        if tx.send(MempoolEvent::TxAdded { txid, raw: body.to_vec() }).is_err() {
                            info!("Channel closed, stopping ZMQ subscriber");
                            return;
                        }
                    }
                    Ok(msg) => {
                        warn!("Unexpected rawtx message format, parts: {}", msg.len());
                    }
                    Err(e) => {
                        if e != zmq::Error::EAGAIN {
                            error!("ZMQ rawtx recv error: {e}");
                        }
                    }
                }
            }

            // Check hashblock
            if items[1].is_readable() {
                match hashblock_sock.recv_multipart(zmq::DONTWAIT) {
                    Ok(msg) if msg.len() >= 2 && msg[0] == b"hashblock" => {
                        let body = &msg[1];
                        if body.len() == 32 {
                            let mut block_hash = [0u8; 32];
                            block_hash.copy_from_slice(body);
                            // We don't know height from ZMQ hashblock alone,
                            // set to 0 — pipeline can look it up via RPC if needed.
                            if tx.send(MempoolEvent::BlockConnected { block_hash, height: 0 }).is_err() {
                                info!("Channel closed, stopping ZMQ subscriber");
                                return;
                            }
                        }
                    }
                    Ok(_) => {}
                    Err(e) => {
                        if e != zmq::Error::EAGAIN {
                            error!("ZMQ hashblock recv error: {e}");
                        }
                    }
                }
            }
        }
    })
}
