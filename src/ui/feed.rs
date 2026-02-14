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

#[component]
fn TxRow(tx: ScoredTx) -> Element {
    let btc_value = tx.tx.total_input_value as f64 / 100_000_000.0;
    let bg = match tx.alert_level {
        crate::core::AlertLevel::Critical => "#3a0000",
        crate::core::AlertLevel::High => "#3a2600",
        crate::core::AlertLevel::Medium => "#3a3a00",
        crate::core::AlertLevel::Low => "#1a1a2e",
    };

    rsx! {
        div {
            style: "background: {bg}; padding: 8px; margin: 4px 0; border-radius: 4px; font-size: 13px;",
            div { style: "display: flex; justify-content: space-between;",
                span {
                    "{tx.alert_level.emoji()} "
                    if tx.tx.is_coinjoin {
                        span { style: "color: #8888ff;", "🔄 " }
                    }
                    span { style: "color: #888;", "{&tx.tx.txid[..16]}..." }
                }
                span { style: "font-weight: bold;",
                    "{btc_value:.4} BTC"
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
