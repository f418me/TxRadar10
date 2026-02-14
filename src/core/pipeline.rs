use chrono::{DateTime, TimeZone, Utc};
use tokio::sync::mpsc;
#[allow(unused_imports)]
use tracing::{debug, error, info, warn};

use std::sync::Arc;

use crate::core::mempool::MempoolState;
use crate::core::tx::{is_rbf_signaling, parse_raw_tx, vsize};
use crate::core::{AnalyzedTx, MempoolEvent, RemovalReason, ScoredTx};
use crate::db::SharedDatabase;
use crate::rpc::BitcoinRpc;
use crate::signals::SignalEngine;
use crate::tags::TagLookup;

/// Resolved prevout info for a single input.
#[derive(Debug)]
struct ResolvedPrevout {
    value: u64,           // satoshis
    block_height: u32,
    block_time: i64,      // unix timestamp
}

/// Resolve a single prevout: cache first, then RPC.
async fn resolve_prevout(
    prev_txid: &str,
    prev_vout: u32,
    db: &SharedDatabase,
    rpc: &BitcoinRpc,
) -> Option<ResolvedPrevout> {
    // 1) Check SQLite cache
    match db.get_utxo(prev_txid, prev_vout) {
        Ok(Some((value, _script_type, block_height, block_time))) => {
            return Some(ResolvedPrevout { value, block_height, block_time });
        }
        Ok(None) => {} // not cached
        Err(e) => {
            debug!("DB cache lookup error for {prev_txid}:{prev_vout}: {e}");
        }
    }

    // 2) RPC call
    let result = rpc.getrawtransaction(prev_txid, true).await;
    match result {
        Ok(tx_json) => {
            let vouts = tx_json.get("vout")?;
            let vout_obj = vouts.get(prev_vout as usize)?;
            let value_btc = vout_obj.get("value")?.as_f64()?;
            let value_sats = (value_btc * 100_000_000.0).round() as u64;

            let script_type = vout_obj
                .get("scriptPubKey")
                .and_then(|s| s.get("type"))
                .and_then(|t| t.as_str())
                .unwrap_or("unknown")
                .to_string();

            // Block info (may be null for unconfirmed)
            let block_height = tx_json
                .get("blockheight")
                .or_else(|| tx_json.get("height"))
                .and_then(|h| h.as_u64())
                .unwrap_or(0) as u32;
            let block_time = tx_json
                .get("blocktime")
                .and_then(|t| t.as_i64())
                .unwrap_or(0);

            // Cache it
            if let Err(e) = db.cache_utxo(prev_txid, prev_vout, value_sats, &script_type, block_height, block_time) {
                debug!("Failed to cache UTXO {prev_txid}:{prev_vout}: {e}");
            }

            Some(ResolvedPrevout {
                value: value_sats,
                block_height,
                block_time,
            })
        }
        Err(e) => {
            debug!("RPC getrawtransaction failed for {prev_txid}: {e}");
            None
        }
    }
}

/// Resolve all prevouts for a parsed transaction. Returns enriched fields.
async fn resolve_all_prevouts(
    parsed: &bitcoin::Transaction,
    db: &SharedDatabase,
    rpc: &BitcoinRpc,
) -> (u64, Option<DateTime<Utc>>, Option<u32>, Option<f64>, usize) {
    // Returns: (total_input_value, oldest_input_time, oldest_input_height, cdd, resolved_count)
    let mut total_input_value: u64 = 0;
    let mut oldest_time: Option<i64> = None;
    let mut oldest_height: Option<u32> = None;
    let mut cdd: f64 = 0.0;
    let mut resolved_count: usize = 0;
    let now = Utc::now();

    for input in &parsed.input {
        // Skip coinbase inputs
        // Skip coinbase (null txid)
        let null_txid: [u8; 32] = [0u8; 32];
        if AsRef::<[u8; 32]>::as_ref(&input.previous_output.txid) == &null_txid {
            continue;
        }

        let prev_txid = input.previous_output.txid.to_string();
        let prev_vout = input.previous_output.vout;

        if let Some(prevout) = resolve_prevout(&prev_txid, prev_vout, db, rpc).await {
            total_input_value += prevout.value;
            resolved_count += 1;

            if prevout.block_time > 0 {
                // Track oldest
                match oldest_time {
                    Some(ot) if prevout.block_time < ot => {
                        oldest_time = Some(prevout.block_time);
                    }
                    None => {
                        oldest_time = Some(prevout.block_time);
                    }
                    _ => {}
                }
                match oldest_height {
                    Some(oh) if prevout.block_height < oh => {
                        oldest_height = Some(prevout.block_height);
                    }
                    None if prevout.block_height > 0 => {
                        oldest_height = Some(prevout.block_height);
                    }
                    _ => {}
                }

                // CDD: value_btc * age_days
                let input_time = Utc.timestamp_opt(prevout.block_time, 0).single();
                if let Some(it) = input_time {
                    let age_days = (now - it).num_seconds() as f64 / 86400.0;
                    if age_days > 0.0 {
                        let value_btc = prevout.value as f64 / 100_000_000.0;
                        cdd += value_btc * age_days;
                    }
                }
            }
        }
    }

    let oldest_dt = oldest_time.and_then(|t| Utc.timestamp_opt(t, 0).single());
    let cdd_opt = if resolved_count > 0 && cdd > 0.0 { Some(cdd) } else { None };

    (total_input_value, oldest_dt, oldest_height, cdd_opt, resolved_count)
}

/// How often to send stats to UI (every N txs or every N seconds).
const STATS_TX_INTERVAL: u64 = 100;
const STATS_TIME_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5);
/// Prune confirmed/evicted entries after 5 minutes.
const PRUNE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(60);
const PRUNE_MAX_AGE: chrono::Duration = chrono::Duration::minutes(5);

fn send_stats(state: &MempoolState, ui_tx: &mpsc::UnboundedSender<PipelineOutput>) {
    let _ = ui_tx.send(PipelineOutput::MempoolStats {
        pending_count: state.pending_count(),
        total_vsize: state.total_vsize(),
        total_fees: state.total_fees(),
        fee_histogram: state.fee_histogram(),
    });
}

/// Run the pipeline: receive MempoolEvents, analyze, score, forward to UI.
pub async fn run_pipeline(
    mut rx: mpsc::UnboundedReceiver<MempoolEvent>,
    ui_tx: mpsc::UnboundedSender<PipelineOutput>,
    db: SharedDatabase,
    rpc: BitcoinRpc,
    tag_lookup: Arc<TagLookup>,
) {
    let engine = SignalEngine::new();
    let mut mempool = MempoolState::new();
    let mut tx_count: u64 = 0;
    let mut block_count: u64 = 0;
    let mut resolved_total: u64 = 0;
    let mut unresolved_total: u64 = 0;
    let mut last_stats_time = std::time::Instant::now();
    let mut last_prune_time = std::time::Instant::now();

    info!("Pipeline started with prevout resolution and mempool state tracking");

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
                let total_output_value: u64 = parsed.output.iter().map(|o| o.value.to_sat()).sum();
                let input_count = parsed.input.len();
                let output_count = parsed.output.len();

                // Resolve prevouts
                let (total_input_value, oldest_input_time, oldest_input_height, coin_days_destroyed, resolved_count) =
                    resolve_all_prevouts(&parsed, &db, &rpc).await;

                let prevouts_resolved = resolved_count == input_count;
                resolved_total += resolved_count as u64;
                unresolved_total += (input_count - resolved_count) as u64;

                // Calculate fee (only if we have input values)
                let fee = if total_input_value > 0 {
                    total_input_value.saturating_sub(total_output_value)
                } else {
                    0
                };
                let fee_rate = if total_input_value > 0 && tx_vsize > 0 {
                    fee as f64 / tx_vsize as f64
                } else {
                    0.0
                };

                // Check outputs against known exchange addresses
                let output_matches = tag_lookup.check_outputs(&parsed);
                let to_exchange = !output_matches.is_empty();
                let to_exchange_confidence = output_matches
                    .iter()
                    .map(|m| m.tag.confidence)
                    .fold(0.0_f64, f64::max);

                // Input address checking would require prevout scripts;
                // for now we don't have them resolved to addresses
                let from_exchange = false;
                let from_exchange_confidence = 0.0;

                let analyzed = AnalyzedTx {
                    txid: txid_str,
                    raw_size: raw.len(),
                    vsize: tx_vsize,
                    total_input_value,
                    total_output_value,
                    fee,
                    fee_rate,
                    input_count,
                    output_count,
                    oldest_input_height,
                    oldest_input_time,
                    coin_days_destroyed,
                    is_rbf_signaling: rbf,
                    seen_at: Utc::now(),
                    prevouts_resolved,
                    to_exchange,
                    to_exchange_confidence,
                    from_exchange,
                    from_exchange_confidence,
                };

                // Add to mempool state
                mempool.add_tx(analyzed.clone());

                let scored = engine.score(&analyzed);
                tx_count += 1;

                if tx_count % 1000 == 0 {
                    info!(
                        "Pipeline: {tx_count} txs, {block_count} blocks, prevouts resolved: {resolved_total}, unresolved: {unresolved_total}, mempool pending: {}",
                        mempool.pending_count()
                    );
                }

                if ui_tx.send(PipelineOutput::NewTx(scored)).is_err() {
                    info!("UI channel closed, stopping pipeline");
                    break;
                }

                // Periodically send stats
                let now = std::time::Instant::now();
                if tx_count % STATS_TX_INTERVAL == 0
                    || now.duration_since(last_stats_time) >= STATS_TIME_INTERVAL
                {
                    send_stats(&mempool, &ui_tx);
                    last_stats_time = now;
                }

                // Periodically prune old entries
                if now.duration_since(last_prune_time) >= PRUNE_INTERVAL {
                    mempool.prune_old(PRUNE_MAX_AGE);
                    last_prune_time = now;
                }
            }
            MempoolEvent::BlockConnected { block_hash: _, height } => {
                block_count += 1;
                info!("Block connected: height={height} (total blocks seen: {block_count})");
                let _ = ui_tx.send(PipelineOutput::BlockConnected { height });
                // After a block, send updated stats
                send_stats(&mempool, &ui_tx);
            }
            MempoolEvent::BlockDisconnected { block_hash: _, height } => {
                warn!("Block disconnected: height={height}");
            }
            MempoolEvent::TxRemoved { txid, reason } => {
                // Convert txid bytes to hex string (reversed for bitcoin display order)
                let txid_hex: String = txid.iter().rev().map(|b| format!("{b:02x}")).collect();
                debug!("Tx removed: {txid_hex} reason={reason:?}");
                mempool.remove_tx(&txid_hex, reason);

                // If replaced, try to record the replacement chain
                if reason == RemovalReason::Replaced {
                    // We don't know the replacing txid from ZMQ alone;
                    // the replacement tracking requires the `sequence` topic.
                    // TODO: wire up ZMQ sequence topic for full RBF tracking
                }
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
    MempoolStats {
        pending_count: usize,
        total_vsize: usize,
        total_fees: u64,
        fee_histogram: Vec<(String, usize)>,
    },
}
