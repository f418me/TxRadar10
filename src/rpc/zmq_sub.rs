use tokio::sync::mpsc;
use tracing::{error, info, warn};

use crate::core::MempoolEvent;

/// ZMQ subscriber configuration.
pub struct ZmqConfig {
    pub rawtx_endpoint: String,
    pub sequence_endpoint: String,
}

impl Default for ZmqConfig {
    fn default() -> Self {
        Self {
            rawtx_endpoint: "tcp://127.0.0.1:28333".into(),
            sequence_endpoint: "tcp://127.0.0.1:28336".into(),
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
            .expect("failed to connect rawtx");
        rawtx_sock.set_subscribe(b"rawtx").expect("subscribe rawtx");

        info!(endpoint = %config.rawtx_endpoint, "ZMQ rawtx subscriber connected");

        // TODO: Add sequence subscriber for ordered events + missed event detection
        // For MVP, we start with rawtx only

        loop {
            // ZMQ multipart: [topic, body, sequence_number]
            let msg = match rawtx_sock.recv_multipart(0) {
                Ok(m) => m,
                Err(e) => {
                    error!("ZMQ recv error: {e}");
                    continue;
                }
            };

            if msg.len() < 2 {
                warn!("unexpected ZMQ message format, parts: {}", msg.len());
                continue;
            }

            let topic = &msg[0];
            let body = &msg[1];

            if topic == b"rawtx" {
                // Extract txid from the raw tx (double SHA256)
                use bitcoin::hashes::{sha256d, Hash};
                let txid_hash = sha256d::Hash::hash(body);
                let mut txid = [0u8; 32];
                txid.copy_from_slice(txid_hash.as_ref());

                let event = MempoolEvent::TxAdded {
                    txid,
                    raw: body.to_vec(),
                };

                if tx.send(event).is_err() {
                    info!("channel closed, stopping ZMQ subscriber");
                    break;
                }
            }
        }
    })
}
