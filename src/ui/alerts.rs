use dioxus::prelude::*;

use crate::core::{AlertLevel, ScoredTx};

#[component]
pub fn AlertPanel(txs: Signal<Vec<ScoredTx>>) -> Element {
    let binding = txs.read();
    let alerts: Vec<&ScoredTx> = binding
        .iter()
        .filter(|tx| matches!(tx.alert_level, AlertLevel::Critical | AlertLevel::High))
        .collect();

    rsx! {
        div { style: "margin-top: 16px;",
            h2 { style: "color: #f7931a;", "🚨 Alerts ({alerts.len()})" }
            if alerts.is_empty() {
                p { style: "color: #666;", "No high-priority signals yet." }
            }
            for tx in alerts.iter().rev().take(20) {
                AlertRow { tx: (*tx).clone() }
            }
        }
    }
}

#[component]
fn AlertRow(tx: ScoredTx) -> Element {
    let btc = tx.tx.total_input_value as f64 / 100_000_000.0;
    let btc_display = if btc >= 1.0 {
        format!("{btc:.4}")
    } else if btc >= 0.001 {
        format!("{btc:.6}")
    } else {
        format!("{btc:.8}")
    };
    let txid_full = tx.tx.txid.clone();

    rsx! {
        div {
            style: "background: #2a1a00; border-left: 3px solid #f7931a; padding: 8px; margin: 4px 0; border-radius: 4px;",
            div { style: "font-weight: bold;",
                "{tx.alert_level.emoji()} Score {tx.composite_score:.0} — {btc_display} BTC"
            }
            div { style: "font-size: 11px; color: #888; cursor: pointer; user-select: all;",
                title: "Click to copy",
                onclick: move |_| {
                    let js = format!("navigator.clipboard.writeText('{txid_full}')");
                    document::eval(&js);
                },
                "{tx.tx.txid}"
            }
            div { style: "font-size: 11px; color: #aaa; margin-top: 4px;",
                for rule in tx.rule_scores.iter().filter(|r| r.weighted_score > 0.1) {
                    span { style: "margin-right: 8px;",
                        "{rule.rule_name}: {rule.weighted_score:.1}"
                    }
                }
            }
        }
    }
}
