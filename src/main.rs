mod config;
mod core;
mod db;
mod rpc;
mod signals;
pub mod tags;
mod ui;

use std::path::Path;
use tokio::sync::mpsc;
use tracing_subscriber::EnvFilter;

use crate::config::Config;
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

    // Load configuration
    let config = Config::load("config.toml");
    tracing::info!("Config: {:?}", config);

    // Open UTXO cache database
    let db_path = Path::new(&config.database.path);
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).expect("Failed to create database directory");
    }
    let db = SharedDatabase::open(db_path)
        .expect("Failed to open UTXO cache database");
    tracing::info!("UTXO cache database opened at {}", config.database.path);

    // Load exchange address tags from CSV if available
    if let Some(ref csv_path_str) = config.database.exchange_csv {
        let csv_path = Path::new(csv_path_str);
        if csv_path.exists() {
            match db.load_tags_from_csv(csv_path) {
                Ok(count) => tracing::info!("Loaded {count} address tags from CSV"),
                Err(e) => tracing::warn!("Failed to load address tags CSV: {e}"),
            }
        }
    }

    // Build in-memory tag lookup
    let tag_lookup = std::sync::Arc::new(crate::tags::TagLookup::load_from_db(&db));

    // Create RPC client
    let rpc = if config.bitcoin.rpc_user.is_some() && config.bitcoin.rpc_password.is_some() {
        BitcoinRpc::new(
            &config.bitcoin.rpc_host,
            config.bitcoin.rpc_port,
            config.bitcoin.rpc_user.as_deref().unwrap(),
            config.bitcoin.rpc_password.as_deref().unwrap(),
        )
    } else {
        BitcoinRpc::from_config_with_defaults(&config.bitcoin.rpc_host, config.bitcoin.rpc_port)
    };
    tracing::info!("Bitcoin RPC client configured");

    // ZMQ → Pipeline channel
    let (zmq_tx, zmq_rx) = mpsc::unbounded_channel();

    // Pipeline → UI channel
    let (ui_tx, ui_rx) = mpsc::unbounded_channel::<PipelineOutput>();

    // Store UI receiver, DB handle, and config in globals so the Dioxus app can grab them
    UI_RX.set(std::sync::Mutex::new(Some(ui_rx))).ok();
    UI_DB.set(std::sync::Mutex::new(Some(db.clone()))).ok();
    UI_CONFIG.set(config.clone()).ok();

    // Build ZMQ config from Config
    let zmq_config = ZmqConfig {
        rawtx_endpoint: config.bitcoin.zmq_rawtx.clone(),
        hashblock_endpoint: config.bitcoin.zmq_hashblock.clone(),
        sequence_endpoint: config.bitcoin.zmq_sequence.clone(),
    };

    // Start ZMQ subscriber thread
    let _zmq_handle = start_zmq_subscriber(zmq_config, zmq_tx);
    tracing::info!("ZMQ subscriber started");

    // Start pipeline in a tokio runtime on a separate thread
    let pipeline_config = config.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
        rt.block_on(core::pipeline::run_pipeline(zmq_rx, ui_tx, db, rpc, tag_lookup, pipeline_config));
    });
    tracing::info!("Pipeline thread started");

    // Launch Dioxus desktop app (blocks)
    dioxus::LaunchBuilder::desktop()
        .with_cfg(
            dioxus::desktop::Config::new()
                .with_window(
                    dioxus::desktop::tao::window::WindowBuilder::new()
                        .with_title("⚡ TxRadar10")
                        .with_inner_size(dioxus::desktop::tao::dpi::LogicalSize::new(1200.0, 800.0))
                )
        )
        .launch(ui::App);
}

/// One-shot global to pass the UI receiver into the Dioxus app.
static UI_RX: std::sync::OnceLock<std::sync::Mutex<Option<mpsc::UnboundedReceiver<PipelineOutput>>>> =
    std::sync::OnceLock::new();

/// One-shot global to pass the DB handle into the Dioxus app.
static UI_DB: std::sync::OnceLock<std::sync::Mutex<Option<db::SharedDatabase>>> =
    std::sync::OnceLock::new();

/// Global config for UI access.
static UI_CONFIG: std::sync::OnceLock<Config> = std::sync::OnceLock::new();

/// Take the UI receiver (can only be called once).
pub fn take_ui_rx() -> Option<mpsc::UnboundedReceiver<PipelineOutput>> {
    UI_RX.get()?.lock().ok()?.take()
}

/// Take the DB handle for the UI (can only be called once).
pub fn take_ui_db() -> Option<db::SharedDatabase> {
    UI_DB.get()?.lock().ok()?.take()
}

/// Get the global config.
pub fn get_config() -> &'static Config {
    UI_CONFIG.get().expect("Config not initialized")
}
