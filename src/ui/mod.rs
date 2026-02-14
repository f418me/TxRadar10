pub mod alerts;
pub mod feed;
pub mod history;
pub mod stats;

use dioxus::prelude::*;

use crate::core::ScoredTx;
use crate::core::mempool::RemovalStats;
use crate::core::pipeline::PipelineOutput;
use crate::db::SignalRecord;

fn max_ui_txs() -> usize {
    crate::get_config().ui.max_feed_entries
}

/// Root UI component.
#[component]
pub fn App() -> Element {
    let mut scored_txs = use_signal(Vec::<ScoredTx>::new);
    let mut mempool_size = use_signal(|| 0usize);
    let mut block_height = use_signal(|| 0u32);
    let mut pending_count = use_signal(|| 0usize);
    let mut total_vsize = use_signal(|| 0usize);
    let mut total_fees = use_signal(|| 0u64);
    let mut fee_histogram = use_signal(Vec::<(String, usize)>::new);
    let mut removal_stats = use_signal(RemovalStats::default);
    let mut history_signals = use_signal(Vec::<SignalRecord>::new);
    let mut signal_stats = use_signal(history::SignalStats::default);

    // Spawn a coroutine that reads from the pipeline channel
    use_coroutine(move |_: UnboundedReceiver<()>| async move {
        let Some(mut rx) = crate::take_ui_rx() else {
            tracing::error!("Failed to take UI receiver");
            return;
        };

        // Take the shared DB handle for history queries
        let db = crate::take_ui_db();

        tracing::info!("UI coroutine started, listening for pipeline output");

        let mut tx_since_history_refresh: u64 = 0;

        while let Some(output) = rx.recv().await {
            match output {
                PipelineOutput::NewTx(tx) => {
                    scored_txs.write().push(tx);
                    // Trim to keep UI responsive
                    let len = scored_txs.read().len();
                    let max_txs = max_ui_txs();
                    if len > max_txs {
                        let drain_count = len - max_txs;
                        scored_txs.write().drain(0..drain_count);
                    }
                    mempool_size.set(scored_txs.read().len());
                    tx_since_history_refresh += 1;

                    // Refresh history from DB every 100 txs
                    if tx_since_history_refresh >= 100 {
                        tx_since_history_refresh = 0;
                        if let Some(ref db) = db {
                            refresh_history(db, &mut history_signals, &mut signal_stats);
                        }
                    }
                }
                PipelineOutput::BlockConnected { height } => {
                    if height > 0 {
                        block_height.set(height);
                    }
                    // Refresh history on each block
                    if let Some(ref db) = db {
                        refresh_history(db, &mut history_signals, &mut signal_stats);
                    }
                }
                PipelineOutput::MempoolStats {
                    pending_count: pc,
                    total_vsize: tv,
                    total_fees: tf,
                    fee_histogram: fh,
                    removal_stats: rs,
                } => {
                    pending_count.set(pc);
                    total_vsize.set(tv);
                    total_fees.set(tf);
                    fee_histogram.set(fh);
                    removal_stats.set(rs);
                }
            }
        }
    });

    rsx! {
        div { class: "app",
            style: "font-family: monospace; background: #1a1a2e; color: #e0e0e0; min-height: 100vh; padding: 16px;",

            h1 { style: "color: #f7931a; margin-bottom: 8px;",
                "⚡ TxRadar10"
            }

            div { style: "display: flex; gap: 16px;",
                // Left: Live feed
                div { style: "flex: 2;",
                    feed::TxFeed { txs: scored_txs }
                }

                // Right: Alerts + Stats + History
                div { style: "flex: 1;",
                    stats::MempoolStats {
                        mempool_size,
                        block_height,
                        pending_count,
                        total_vsize,
                        total_fees,
                        fee_histogram,
                        removal_stats,
                    }
                    alerts::AlertPanel { txs: scored_txs }
                    history::HistoryPanel {
                        signals: history_signals,
                        signal_stats,
                    }
                }
            }
        }
    }
}

fn refresh_history(
    db: &crate::db::SharedDatabase,
    history_signals: &mut Signal<Vec<SignalRecord>>,
    signal_stats: &mut Signal<history::SignalStats>,
) {
    // Get recent high-score signals
    if let Ok(recent) = db.get_signals_above_score(10.0, 50) {
        history_signals.set(recent);
    }

    // Get stats
    let total_count = db.get_signal_count().unwrap_or(0);

    let now = chrono::Utc::now();
    let one_hour_ago = now - chrono::Duration::hours(1);
    let one_day_ago = now - chrono::Duration::hours(24);

    let last_hour_count = db
        .get_signals_by_timerange(one_hour_ago, now)
        .map(|v| v.len())
        .unwrap_or(0);
    let last_24h_signals = db
        .get_signals_by_timerange(one_day_ago, now)
        .unwrap_or_default();
    let last_24h_count = last_24h_signals.len();
    let avg_score = if last_24h_count > 0 {
        last_24h_signals.iter().map(|s| s.score).sum::<f64>() / last_24h_count as f64
    } else {
        0.0
    };

    signal_stats.set(history::SignalStats {
        total_count,
        last_hour_count,
        last_24h_count,
        avg_score,
    });
}
