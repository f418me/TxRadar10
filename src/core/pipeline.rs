use chrono::Utc;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use crate::core::tx::{is_rbf_signaling, parse_raw_tx, vsize};
use crate::core::{AnalyzedTx, MempoolEvent, ScoredTx};
use crate::signals::SignalEngine;

/// Run the pipeline: receive MempoolEvents, analyze, score, forward to UI.
pub async fn run_pipeline(
    mut rx: mpsc::UnboundedReceiver<MempoolEvent>,
    ui_tx: mpsc::UnboundedSender<PipelineOutput>,
) {
    let engine = SignalEngine::new();
    let mut tx_count: u64 = 0;
    let mut block_count: u64 = 0;

    info!("Pipeline started, waiting for ZMQ events...");

    while let Some(event) = rx.recv().await {
        match event {
            MempoolEvent::TxAdded { txid: _, raw } => {
                let parsed = match parse_raw_tx(&raw) {
                    Ok(tx) => tx,
                    Err(e) => {
                        debug!("Failed to parse raw tx: {e}");
                        continue;
                    }
                };

                let tx_vsize = vsize(&parsed);
                let rbf = is_rbf_signaling(&parsed);
                let txid_str = parsed.compute_txid().to_string();

                // Without prevout resolution, we can't know input values.
                // Set fee/value to 0 for now — prevout resolution comes later.
                let analyzed = AnalyzedTx {
                    txid: txid_str,
                    raw_size: raw.len(),
                    vsize: tx_vsize,
                    total_input_value: 0,
                    total_output_value: parsed.output.iter().map(|o| o.value.to_sat()).sum(),
                    fee: 0,
                    fee_rate: 0.0,
                    input_count: parsed.input.len(),
                    output_count: parsed.output.len(),
                    oldest_input_height: None,
                    oldest_input_time: None,
                    coin_days_destroyed: None,
                    is_rbf_signaling: rbf,
                    seen_at: Utc::now(),
                    prevouts_resolved: false,
                };

                let scored = engine.score(&analyzed);
                tx_count += 1;

                if tx_count % 1000 == 0 {
                    info!("Pipeline processed {tx_count} txs, {block_count} blocks");
                }

                if ui_tx.send(PipelineOutput::NewTx(scored)).is_err() {
                    info!("UI channel closed, stopping pipeline");
                    break;
                }
            }
            MempoolEvent::BlockConnected { block_hash: _, height } => {
                block_count += 1;
                info!("Block connected: height={height} (total blocks seen: {block_count})");
                let _ = ui_tx.send(PipelineOutput::BlockConnected { height });
            }
            MempoolEvent::BlockDisconnected { block_hash: _, height } => {
                warn!("Block disconnected: height={height}");
            }
            MempoolEvent::TxRemoved { txid: _, reason } => {
                debug!("Tx removed: {reason:?}");
            }
        }
    }

    info!("Pipeline shutting down after {tx_count} txs, {block_count} blocks");
}

/// Messages from pipeline to UI.
#[derive(Debug, Clone)]
pub enum PipelineOutput {
    NewTx(ScoredTx),
    BlockConnected { height: u32 },
}
