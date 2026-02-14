#![allow(dead_code)]
use std::collections::HashMap;

use chrono::{DateTime, Utc};

use super::{AnalyzedTx, RemovalReason};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxState {
    Pending,
    Confirmed,
    Replaced,
    Evicted,
}

#[derive(Debug)]
pub struct MempoolEntry {
    pub tx: AnalyzedTx,
    pub state: TxState,
    pub state_changed_at: DateTime<Utc>,
}

/// In-memory mempool state tracker.
#[derive(Debug, Default)]
pub struct MempoolState {
    pub entries: HashMap<String, MempoolEntry>,
    pub last_sequence: u64,
    pub tip_height: u32,
}

impl MempoolState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_tx(&mut self, tx: AnalyzedTx) {
        let txid = tx.txid.clone();
        self.entries.insert(
            txid,
            MempoolEntry {
                tx,
                state: TxState::Pending,
                state_changed_at: Utc::now(),
            },
        );
    }

    pub fn remove_tx(&mut self, txid: &str, reason: RemovalReason) -> Option<MempoolEntry> {
        if let Some(entry) = self.entries.get_mut(txid) {
            entry.state = match reason {
                RemovalReason::Confirmed => TxState::Confirmed,
                RemovalReason::Replaced => TxState::Replaced,
                _ => TxState::Evicted,
            };
            entry.state_changed_at = Utc::now();
        }
        // Keep confirmed/replaced briefly for UI, then prune
        None
    }

    pub fn pending_count(&self) -> usize {
        self.entries
            .values()
            .filter(|e| e.state == TxState::Pending)
            .count()
    }

    /// Prune non-pending entries older than given duration.
    pub fn prune_old(&mut self, max_age: chrono::Duration) {
        let cutoff = Utc::now() - max_age;
        self.entries.retain(|_, e| {
            e.state == TxState::Pending || e.state_changed_at > cutoff
        });
    }
}
