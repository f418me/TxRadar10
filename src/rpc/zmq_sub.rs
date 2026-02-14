use tokio::sync::mpsc;
use tracing::{error, info, warn};

use crate::core::MempoolEvent;
use crate::core::RemovalReason;

/// ZMQ subscriber configuration.
pub struct ZmqConfig {
    pub rawtx_endpoint: String,
    pub hashblock_endpoint: String,
    /// ZMQ sequence endpoint for tx removal and block notifications.
    pub sequence_endpoint: Option<String>,
}

impl Default for ZmqConfig {
    fn default() -> Self {
        Self {
            rawtx_endpoint: "tcp://127.0.0.1:28333".into(),
            hashblock_endpoint: "tcp://127.0.0.1:28332".into(),
            sequence_endpoint: Some("tcp://127.0.0.1:28336".into()),
        }
    }
}

/// Parse a ZMQ sequence message body.
/// Format: 32-byte hash + 1-byte label ('A'/'R'/'C'/'D') + 8-byte LE sequence number.
fn parse_sequence_message(body: &[u8]) -> Option<([u8; 32], u8, u64)> {
    if body.len() != 32 + 1 + 8 {
        return None;
    }
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&body[..32]);
    let label = body[32];
    let seq = u64::from_le_bytes(body[33..41].try_into().ok()?);
    Some((hash, label, seq))
}

/// Start ZMQ subscriber in a blocking thread (zmq crate is synchronous).
/// Sends MempoolEvents into the provided channel.
///
/// Strategy: `rawtx` for TxAdded (has full tx data inline),
/// `sequence` for TxRemoved + Block events only.
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

        // Optionally subscribe to sequence
        let sequence_sock = config.sequence_endpoint.as_ref().and_then(|endpoint| {
            let sock = match ctx.socket(zmq::SUB) {
                Ok(s) => s,
                Err(e) => {
                    warn!("Failed to create sequence socket: {e}, continuing without sequence topic");
                    return None;
                }
            };
            if let Err(e) = sock.connect(endpoint) {
                warn!("Failed to connect sequence at {endpoint}: {e}, continuing without sequence topic");
                return None;
            }
            if let Err(e) = sock.set_subscribe(b"sequence") {
                warn!("Failed to subscribe to sequence topic: {e}");
                return None;
            }
            info!(endpoint = %endpoint, "ZMQ sequence subscriber connected");
            Some(sock)
        });

        // Track last sequence number for missed-event detection
        let mut last_seq: Option<u64> = None;

        loop {
            // Build poll items dynamically based on whether sequence socket exists
            let poll_result = if let Some(ref seq_sock) = sequence_sock {
                let mut items = [
                    rawtx_sock.as_poll_item(zmq::POLLIN),
                    hashblock_sock.as_poll_item(zmq::POLLIN),
                    seq_sock.as_poll_item(zmq::POLLIN),
                ];
                let res = zmq::poll(&mut items, 1000);
                res.map(|_| (items[0].is_readable(), items[1].is_readable(), items[2].is_readable()))
            } else {
                let mut items = [
                    rawtx_sock.as_poll_item(zmq::POLLIN),
                    hashblock_sock.as_poll_item(zmq::POLLIN),
                ];
                let res = zmq::poll(&mut items, 1000);
                res.map(|_| (items[0].is_readable(), items[1].is_readable(), false))
            };

            let (rawtx_ready, hashblock_ready, sequence_ready) = match poll_result {
                Ok(flags) => flags,
                Err(e) => {
                    error!("ZMQ poll error: {e}");
                    std::thread::sleep(std::time::Duration::from_secs(1));
                    continue;
                }
            };

            // Check rawtx
            if rawtx_ready {
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
            if hashblock_ready {
                match hashblock_sock.recv_multipart(zmq::DONTWAIT) {
                    Ok(msg) if msg.len() >= 2 && msg[0] == b"hashblock" => {
                        let body = &msg[1];
                        if body.len() == 32 {
                            let mut block_hash = [0u8; 32];
                            block_hash.copy_from_slice(body);
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

            // Check sequence
            if sequence_ready {
                if let Some(ref seq_sock) = sequence_sock {
                    match seq_sock.recv_multipart(zmq::DONTWAIT) {
                        Ok(msg) if msg.len() >= 2 && msg[0] == b"sequence" => {
                            let body = &msg[1];
                            if let Some((hash, label, seq)) = parse_sequence_message(body) {
                                // Missed-event detection
                                if let Some(prev) = last_seq {
                                    if seq != prev + 1 {
                                        warn!(
                                            "ZMQ sequence gap detected: expected {}, got {} (missed {} events)",
                                            prev + 1, seq, seq.saturating_sub(prev + 1)
                                        );
                                    }
                                }
                                last_seq = Some(seq);

                                match label {
                                    b'A' => {
                                        // TxAdded from sequence — we ignore this since rawtx
                                        // provides the full tx data. No action needed.
                                    }
                                    b'R' => {
                                        if tx.send(MempoolEvent::TxRemoved {
                                            txid: hash,
                                            reason: RemovalReason::Unknown,
                                        }).is_err() {
                                            info!("Channel closed, stopping ZMQ subscriber");
                                            return;
                                        }
                                    }
                                    b'C' => {
                                        if tx.send(MempoolEvent::BlockConnected {
                                            block_hash: hash,
                                            height: 0,
                                        }).is_err() {
                                            info!("Channel closed, stopping ZMQ subscriber");
                                            return;
                                        }
                                    }
                                    b'D' => {
                                        if tx.send(MempoolEvent::BlockDisconnected {
                                            block_hash: hash,
                                            height: 0,
                                        }).is_err() {
                                            info!("Channel closed, stopping ZMQ subscriber");
                                            return;
                                        }
                                    }
                                    other => {
                                        warn!("Unknown sequence label: 0x{other:02x}");
                                    }
                                }
                            } else {
                                warn!("Invalid sequence message body length: {}", body.len());
                            }
                        }
                        Ok(msg) => {
                            warn!("Unexpected sequence message format, parts: {}", msg.len());
                        }
                        Err(e) => {
                            if e != zmq::Error::EAGAIN {
                                error!("ZMQ sequence recv error: {e}");
                            }
                        }
                    }
                }
            }
        }
    })
}
