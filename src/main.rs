mod core;
mod db;
mod rpc;
mod signals;
mod ui;

use tracing_subscriber::EnvFilter;

fn main() {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("txradar10=info".parse().unwrap()))
        .init();

    tracing::info!("⚡ TxRadar10 starting...");

    // Launch Dioxus desktop app
    dioxus::launch(ui::App);
}
