mod core;
mod db;
mod rpc;
mod signals;
mod ui;

use std::path::Path;
use tokio::sync::mpsc;
use tracing_subscriber::EnvFilter;

use crate::core::pipeline::PipelineOutput;
use crate::db::SharedDatabase;
use crate::rpc::BitcoinRpc;
use crate::rpc::zmq_sub::{ZmqConfig, start_zmq_subscriber};

fn main() {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env().add_directive("txradar10=info".parse().unwrap()),
        )
        .init();

    tracing::info!("⚡ TxRadar10 starting...");

    // Open UTXO cache database
    let db_dir = Path::new("data");
    std::fs::create_dir_all(db_dir).expect("Failed to create data/ directory");
    let db = SharedDatabase::open(&db_dir.join("utxo_cache.db"))
        .expect("Failed to open UTXO cache database");
    tracing::info!("UTXO cache database opened at data/utxo_cache.db");

    // Create RPC client (reads credentials from bitcoin.conf or env)
    let rpc = BitcoinRpc::from_config();
    tracing::info!("Bitcoin RPC client configured");

    // ZMQ → Pipeline channel
    let (zmq_tx, zmq_rx) = mpsc::unbounded_channel();

    // Pipeline → UI channel
    let (ui_tx, ui_rx) = mpsc::unbounded_channel::<PipelineOutput>();

    // Store UI receiver in a global so the Dioxus app can grab it
    UI_RX.set(std::sync::Mutex::new(Some(ui_rx))).ok();

    // Start ZMQ subscriber thread
    let _zmq_handle = start_zmq_subscriber(ZmqConfig::default(), zmq_tx);
    tracing::info!("ZMQ subscriber started");

    // Start pipeline in a tokio runtime on a separate thread
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
        rt.block_on(core::pipeline::run_pipeline(zmq_rx, ui_tx, db, rpc));
    });
    tracing::info!("Pipeline thread started");

    // Launch Dioxus desktop app (blocks)
    dioxus::launch(ui::App);
}

/// One-shot global to pass the UI receiver into the Dioxus app.
static UI_RX: std::sync::OnceLock<std::sync::Mutex<Option<mpsc::UnboundedReceiver<PipelineOutput>>>> =
    std::sync::OnceLock::new();

/// Take the UI receiver (can only be called once).
pub fn take_ui_rx() -> Option<mpsc::UnboundedReceiver<PipelineOutput>> {
    UI_RX.get()?.lock().ok()?.take()
}
