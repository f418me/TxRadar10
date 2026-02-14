use dioxus::prelude::*;

use crate::db::SignalRecord;

#[component]
pub fn HistoryPanel(signals: Signal<Vec<SignalRecord>>, signal_stats: Signal<SignalStats>) -> Element {
    let stats = signal_stats.read();

    rsx! {
        div { style: "margin-top: 16px;",
            h2 { style: "color: #f7931a;", "📜 Signal History" }

            // Stats summary
            div { style: "background: #16213e; padding: 12px; border-radius: 4px; margin-bottom: 8px; font-size: 13px;",
                p { "Total signals stored: {stats.total_count}" }
                p { "Last hour: {stats.last_hour_count}" }
                p { "Last 24h: {stats.last_24h_count}" }
                if stats.avg_score > 0.0 {
                    p { "Avg score: {stats.avg_score:.1}" }
                }
            }

            // Recent high-score signals
            div { style: "max-height: 40vh; overflow-y: auto;",
                for signal in signals.read().iter().take(50) {
                    SignalRow { signal: signal.clone() }
                }
                if signals.read().is_empty() {
                    p { style: "color: #666;", "No signals recorded yet." }
                }
            }
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SignalStats {
    pub total_count: usize,
    pub last_hour_count: usize,
    pub last_24h_count: usize,
    pub avg_score: f64,
}

#[component]
fn SignalRow(signal: SignalRecord) -> Element {
    let btc = signal.total_input_value as f64 / 100_000_000.0;
    let btc_display = if btc >= 1.0 {
        format!("{btc:.4}")
    } else if btc >= 0.001 {
        format!("{btc:.6}")
    } else {
        format!("{btc:.8}")
    };
    let txid_full = signal.txid.clone();
    let txid_display = signal.txid.clone();
    let exchange_badge = if signal.to_exchange { "📤" } else { "" };
    let alert_emoji = match signal.alert_level.as_str() {
        "Critical" => "🔴",
        "High" => "🟠",
        "Medium" => "🟡",
        _ => "⚪",
    };

    rsx! {
        div {
            style: "background: #16213e; padding: 8px; margin: 4px 0; border-radius: 4px; font-size: 12px;",
            div { style: "display: flex; justify-content: space-between; align-items: center;",
                span { style: "display: flex; align-items: center; gap: 4px;",
                    "{alert_emoji} "
                    span {
                        style: "color: #888; cursor: pointer; user-select: all; font-family: monospace; font-size: 11px; word-break: break-all;",
                        onclick: move |_| {
                            let js = format!("navigator.clipboard.writeText('{txid_full}')");
                            document::eval(&js);
                        },
                        title: "Click to copy",
                        "{txid_display}"
                    }
                    " {exchange_badge}"
                }
                span { style: "font-weight: bold;",
                    "Score {signal.score:.0}"
                }
            }
            div { style: "display: flex; justify-content: space-between; color: #888; font-size: 11px;",
                span { "{btc_display} BTC" }
                span { "{signal.fee_rate:.1} sat/vB" }
                span { "{signal.created_at}" }
            }
        }
    }
}
