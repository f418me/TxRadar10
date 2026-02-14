pub mod alerts;
pub mod feed;
pub mod stats;

use dioxus::prelude::*;

use crate::core::ScoredTx;

/// Root UI component.
#[component]
pub fn App() -> Element {
    // Shared signal state — will be populated from the backend channel
    let scored_txs = use_signal(Vec::<ScoredTx>::new);
    let mempool_size = use_signal(|| 0usize);

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

                // Right: Alerts + Stats
                div { style: "flex: 1;",
                    stats::MempoolStats { mempool_size }
                    alerts::AlertPanel { txs: scored_txs }
                }
            }
        }
    }
}
