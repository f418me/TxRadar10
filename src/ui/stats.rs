use dioxus::prelude::*;

#[component]
pub fn MempoolStats(
    mempool_size: Signal<usize>,
    block_height: Signal<u32>,
    pending_count: Signal<usize>,
    total_vsize: Signal<usize>,
    total_fees: Signal<u64>,
    fee_histogram: Signal<Vec<(String, usize)>>,
) -> Element {
    let fees_btc = *total_fees.read() as f64 / 100_000_000.0;
    let vsize_mb = *total_vsize.read() as f64 / 1_000_000.0;

    // Find max bucket count for bar scaling
    let histogram = fee_histogram.read();
    let max_count = histogram.iter().map(|(_, c)| *c).max().unwrap_or(1).max(1);

    rsx! {
        div {
            h2 { style: "color: #f7931a;", "📊 Mempool" }
            div { style: "background: #16213e; padding: 12px; border-radius: 4px;",
                p { "Tracked txs: {mempool_size}" }
                if *block_height.read() > 0 {
                    p { "Last block: {block_height}" }
                }
                p { "Pending: {pending_count}" }
                p { "Total vSize: {vsize_mb:.2} MB" }
                p { "Total fees: {fees_btc:.4} BTC" }

                if !histogram.is_empty() {
                    h3 { style: "color: #f7931a; margin-top: 8px; font-size: 13px;",
                        "Fee Rate Distribution (sat/vB)"
                    }
                    div { style: "font-size: 12px;",
                        for (label, count) in histogram.iter() {
                            {
                                let bar_width = (*count as f64 / max_count as f64 * 100.0) as u32;
                                let min_w = if *count > 0 { 2 } else { 0 };
                                rsx! {
                                    div { style: "display: flex; align-items: center; margin: 2px 0;",
                                        span { style: "width: 50px; text-align: right; margin-right: 8px; color: #888;",
                                            "{label}"
                                        }
                                        div { style: "flex: 1; background: #0a0a1a; border-radius: 2px; height: 14px;",
                                            div {
                                                style: "width: {bar_width}%; background: #f7931a; height: 100%; border-radius: 2px; min-width: {min_w}px;",
                                            }
                                        }
                                        span { style: "width: 40px; text-align: right; margin-left: 4px; color: #aaa; font-size: 11px;",
                                            "{count}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
