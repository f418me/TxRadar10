use dioxus::prelude::*;

use crate::core::ScoredTx;

#[component]
pub fn TxFeed(txs: Signal<Vec<ScoredTx>>) -> Element {
    rsx! {
        div {
            h2 { style: "color: #f7931a;", "Live Feed" }
            div { style: "max-height: 80vh; overflow-y: auto;",
                for tx in txs.read().iter().rev().take(100) {
                    TxRow { tx: tx.clone() }
                }
                if txs.read().is_empty() {
                    p { style: "color: #666;", "Waiting for mempool transactions..." }
                }
            }
        }
    }
}

/// Format BTC value with appropriate decimal places (up to 8, trailing zeros trimmed).
fn format_btc(sats: u64) -> String {
    let btc = sats as f64 / 100_000_000.0;
    if btc >= 1.0 {
        format!("{btc:.4}")
    } else if btc >= 0.001 {
        format!("{btc:.6}")
    } else {
        format!("{btc:.8}")
    }
}

#[component]
fn TxRow(tx: ScoredTx) -> Element {
    let btc_display = format_btc(tx.tx.total_input_value);
    let txid_full = tx.tx.txid.clone();
    let bg = match tx.alert_level {
        crate::core::AlertLevel::Critical => "#3a0000",
        crate::core::AlertLevel::High => "#3a2600",
        crate::core::AlertLevel::Medium => "#3a3a00",
        crate::core::AlertLevel::Low => "#1a1a2e",
    };

    rsx! {
        div {
            style: "background: {bg}; padding: 8px; margin: 4px 0; border-radius: 4px; font-size: 13px;",
            div { style: "display: flex; justify-content: space-between; align-items: center;",
                span { style: "display: flex; align-items: center; gap: 4px;",
                    "{tx.alert_level.emoji()} "
                    if tx.tx.is_coinjoin {
                        span { style: "color: #8888ff;", "🔄 " }
                    }
                    span {
                        style: "color: #888; cursor: pointer; user-select: all;",
                        title: "{txid_full}",
                        "{&tx.tx.txid[..16]}…"
                    }
                    button {
                        style: "background: none; border: 1px solid #555; color: #888; font-size: 10px; padding: 1px 4px; border-radius: 3px; cursor: pointer;",
                        title: "Copy full txid",
                        onclick: move |_| {
                            let js = format!("navigator.clipboard.writeText('{txid_full}')");
                            document::eval(&js);
                        },
                        "📋"
                    }
                }
                span { style: "font-weight: bold;",
                    "{btc_display} BTC"
                }
            }
            div { style: "display: flex; justify-content: space-between; color: #888; font-size: 11px;",
                span { "Score: {tx.composite_score:.0}" }
                span { "{tx.tx.fee_rate:.1} sat/vB" }
                span { "ins: {tx.tx.input_count} outs: {tx.tx.output_count}" }
            }
        }
    }
}
