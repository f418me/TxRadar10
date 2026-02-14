use dioxus::prelude::*;

#[component]
pub fn MempoolStats(mempool_size: Signal<usize>) -> Element {
    rsx! {
        div {
            h2 { style: "color: #f7931a;", "📊 Mempool" }
            div { style: "background: #16213e; padding: 12px; border-radius: 4px;",
                p { "Tracked txs: {mempool_size}" }
            }
        }
    }
}
