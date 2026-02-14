pub mod alerts;
pub mod history;
pub mod stats;

use dioxus::prelude::*;

use crate::core::ScoredTx;
use crate::core::mempool::RemovalStats;
use crate::core::pipeline::PipelineOutput;
use crate::db::SignalRecord;

/// Root UI component.
#[component]
pub fn App() -> Element {
    let mut alert_txs = use_signal(Vec::<ScoredTx>::new);
    let mut tx_count = use_signal(|| 0u64);
    let mut block_height = use_signal(|| 0u32);
    let mut pending_count = use_signal(|| 0usize);
    let mut total_vsize = use_signal(|| 0usize);
    let mut total_fees = use_signal(|| 0u64);
    let mut fee_histogram = use_signal(Vec::<(String, usize)>::new);
    let mut removal_stats = use_signal(RemovalStats::default);
    let mut history_signals = use_signal(Vec::<SignalRecord>::new);
    let mut signal_stats = use_signal(history::SignalStats::default);

    use_coroutine(move |_: UnboundedReceiver<()>| async move {
        let Some(mut rx) = crate::take_ui_rx() else {
            tracing::error!("Failed to take UI receiver");
            return;
        };

        let db = crate::take_ui_db();
        tracing::info!("UI coroutine started, listening for pipeline output");

        let mut tx_since_refresh: u64 = 0;
        let mut local_tx_count: u64 = 0;
        let mut last_ui_update = tokio::time::Instant::now();
        let ui_interval = tokio::time::Duration::from_secs(1);

        // Buffer for high-score txs between UI updates
        let mut new_alerts: Vec<ScoredTx> = Vec::new();

        loop {
            let output = tokio::select! {
                msg = rx.recv() => msg,
                _ = tokio::time::sleep_until(last_ui_update + ui_interval) => {
                    // Periodic UI flush
                    if !new_alerts.is_empty() {
                        let mut writer = alert_txs.write();
                        writer.extend(new_alerts.drain(..));
                        // Keep last 200 alerts
                        if writer.len() > 200 {
                            let drain = writer.len() - 200;
                            writer.drain(0..drain);
                        }
                    }
                    tx_count.set(local_tx_count);
                    last_ui_update = tokio::time::Instant::now();
                    continue;
                }
            };

            let Some(output) = output else { break };

            match output {
                PipelineOutput::NewTx(tx) => {
                    local_tx_count += 1;
                    tx_since_refresh += 1;

                    // Only buffer alerts (High + Critical) for UI
                    if tx.composite_score >= 40.0 {
                        new_alerts.push(tx);
                    }

                    // Refresh history from DB periodically
                    if tx_since_refresh >= 500 {
                        tx_since_refresh = 0;
                        if let Some(ref db) = db {
                            refresh_history(db, &mut history_signals, &mut signal_stats);
                        }
                    }
                }
                PipelineOutput::BlockConnected { height } => {
                    if height > 0 {
                        block_height.set(height);
                    }
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
            p { style: "color: #666; font-size: 12px; margin-bottom: 16px;",
                "Txs processed: {tx_count}"
            }

            div { style: "display: flex; gap: 16px;",
                // Left: Stats + Alerts
                div { style: "flex: 1;",
                    stats::MempoolStats {
                        block_height,
                        pending_count,
                        total_vsize,
                        total_fees,
                        fee_histogram,
                        removal_stats,
                    }
                    alerts::AlertPanel { txs: alert_txs }
                }

                // Right: History
                div { style: "flex: 1;",
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
    if let Ok(recent) = db.get_signals_above_score(10.0, 50) {
        history_signals.set(recent);
    }

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
