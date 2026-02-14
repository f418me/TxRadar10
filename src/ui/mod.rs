pub mod alerts;
pub mod feed;
pub mod stats;

use dioxus::prelude::*;

use crate::core::ScoredTx;
use crate::core::pipeline::PipelineOutput;

const MAX_UI_TXS: usize = 500;

/// Root UI component.
#[component]
pub fn App() -> Element {
    let mut scored_txs = use_signal(Vec::<ScoredTx>::new);
    let mut mempool_size = use_signal(|| 0usize);
    let mut block_height = use_signal(|| 0u32);

    // Spawn a coroutine that reads from the pipeline channel
    use_coroutine(move |_: UnboundedReceiver<()>| async move {
        let Some(mut rx) = crate::take_ui_rx() else {
            tracing::error!("Failed to take UI receiver");
            return;
        };

        tracing::info!("UI coroutine started, listening for pipeline output");

        while let Some(output) = rx.recv().await {
            match output {
                PipelineOutput::NewTx(tx) => {
                    scored_txs.write().push(tx);
                    // Trim to keep UI responsive
                    let len = scored_txs.read().len();
                    if len > MAX_UI_TXS {
                        let drain_count = len - MAX_UI_TXS;
                        scored_txs.write().drain(0..drain_count);
                    }
                    mempool_size.set(scored_txs.read().len());
                }
                PipelineOutput::BlockConnected { height } => {
                    if height > 0 {
                        block_height.set(height);
                    }
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

                // Right: Alerts + Stats
                div { style: "flex: 1;",
                    stats::MempoolStats { mempool_size, block_height }
                    alerts::AlertPanel { txs: scored_txs }
                }
            }
        }
    }
}
