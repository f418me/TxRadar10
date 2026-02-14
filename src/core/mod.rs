pub mod mempool;
pub mod pipeline;
pub mod tx;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A mempool lifecycle event from ZMQ sequence topic.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum MempoolEvent {
    TxAdded { txid: [u8; 32], raw: Vec<u8> },
    TxRemoved { txid: [u8; 32], reason: RemovalReason },
    BlockConnected { block_hash: [u8; 32], height: u32 },
    BlockDisconnected { block_hash: [u8; 32], height: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum RemovalReason {
    Confirmed,
    Replaced,
    Evicted,
    Conflict,
    Unknown,
}

/// A transaction enriched with prevout data and scoring context.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnalyzedTx {
    pub txid: String,
    pub raw_size: usize,
    pub vsize: usize,
    pub total_input_value: u64,
    pub total_output_value: u64,
    pub fee: u64,
    pub fee_rate: f64, // sat/vB
    pub input_count: usize,
    pub output_count: usize,
    pub oldest_input_height: Option<u32>,
    pub oldest_input_time: Option<DateTime<Utc>>,
    pub coin_days_destroyed: Option<f64>,
    pub is_rbf_signaling: bool,
    pub seen_at: DateTime<Utc>,
    pub prevouts_resolved: bool,
    /// Whether any output goes to a known exchange address.
    pub to_exchange: bool,
    /// Highest confidence of exchange tag matches on outputs.
    pub to_exchange_confidence: f64,
    /// Whether any input comes from a known exchange address.
    pub from_exchange: bool,
    /// Highest confidence of exchange tag matches on inputs.
    pub from_exchange_confidence: f64,
}

/// A scored transaction ready for UI display.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScoredTx {
    pub tx: AnalyzedTx,
    pub composite_score: f64, // 0-100
    pub rule_scores: Vec<RuleScore>,
    pub alert_level: AlertLevel,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuleScore {
    pub rule_name: String,
    pub raw_value: f64,
    pub weight: f64,
    pub weighted_score: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlertLevel {
    Critical, // ≥80
    High,     // ≥60
    Medium,   // ≥40
    Low,      // <40
}

impl AlertLevel {
    pub fn from_score(score: f64) -> Self {
        if score >= 80.0 {
            AlertLevel::Critical
        } else if score >= 60.0 {
            AlertLevel::High
        } else if score >= 40.0 {
            AlertLevel::Medium
        } else {
            AlertLevel::Low
        }
    }

    pub fn emoji(&self) -> &'static str {
        match self {
            AlertLevel::Critical => "🔴",
            AlertLevel::High => "🟠",
            AlertLevel::Medium => "🟡",
            AlertLevel::Low => "⚪",
        }
    }
}
