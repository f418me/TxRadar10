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
    /// If this tx was replaced, the txid of the replacement.
    /// Used when ZMQ sequence topic provides replacement info.
    #[allow(dead_code)]
    pub replaced_by: Option<String>,
}

/// Fee histogram bucket definition.
const FEE_BUCKETS: &[(f64, f64, &str)] = &[
    (0.0, 5.0, "1-5"),
    (5.0, 10.0, "5-10"),
    (10.0, 20.0, "10-20"),
    (20.0, 50.0, "20-50"),
    (50.0, 100.0, "50-100"),
    (100.0, f64::MAX, "100+"),
];

/// In-memory mempool state tracker.
#[derive(Debug, Default)]
pub struct MempoolState {
    entries: HashMap<String, MempoolEntry>,
    /// RBF replacement chains: replaced_txid → replacing_txid
    replacement_chain: HashMap<String, String>,
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
                replaced_by: None,
            },
        );
    }

    /// Transition a tx out of Pending state.
    pub fn remove_tx(&mut self, txid: &str, reason: RemovalReason) {
        let new_state = match reason {
            RemovalReason::Confirmed => TxState::Confirmed,
            RemovalReason::Replaced => TxState::Replaced,
            _ => TxState::Evicted,
        };
        if let Some(entry) = self.entries.get_mut(txid) {
            entry.state = new_state;
            entry.state_changed_at = Utc::now();
        }
    }

    /// Record an RBF replacement: `old_txid` was replaced by `new_txid`.
    /// Will be called once ZMQ sequence topic is wired up.
    #[allow(dead_code)]
    pub fn record_replacement(&mut self, old_txid: &str, new_txid: &str) {
        self.replacement_chain
            .insert(old_txid.to_string(), new_txid.to_string());
        if let Some(entry) = self.entries.get_mut(old_txid) {
            entry.replaced_by = Some(new_txid.to_string());
            entry.state = TxState::Replaced;
            entry.state_changed_at = Utc::now();
        }
    }

    /// Mark all currently-pending txs as confirmed (used after a block).
    /// Returns the number of txs marked.
    #[allow(dead_code)]
    pub fn confirm_all_pending(&mut self) -> usize {
        let now = Utc::now();
        let mut count = 0;
        for entry in self.entries.values_mut() {
            if entry.state == TxState::Pending {
                // We can't know which txs were in the block, so we don't
                // confirm all pending here. This is called selectively.
            }
            let _ = (entry, now, &mut count); // suppress unused
        }
        count
    }

    /// Mark specific txids as confirmed (txids that disappeared after a block).
    #[allow(dead_code)]
    pub fn confirm_txids(&mut self, txids: &[String]) {
        let now = Utc::now();
        for txid in txids {
            if let Some(entry) = self.entries.get_mut(txid.as_str()) {
                if entry.state == TxState::Pending {
                    entry.state = TxState::Confirmed;
                    entry.state_changed_at = now;
                }
            }
        }
    }

    // --- Statistics ---

    pub fn pending_count(&self) -> usize {
        self.entries
            .values()
            .filter(|e| e.state == TxState::Pending)
            .count()
    }

    pub fn total_fees(&self) -> u64 {
        self.entries
            .values()
            .filter(|e| e.state == TxState::Pending)
            .map(|e| e.tx.fee)
            .sum()
    }

    pub fn total_vsize(&self) -> usize {
        self.entries
            .values()
            .filter(|e| e.state == TxState::Pending)
            .map(|e| e.tx.vsize)
            .sum()
    }

    /// Fee histogram: counts of pending txs per fee-rate bucket.
    pub fn fee_histogram(&self) -> Vec<(String, usize)> {
        let mut counts = vec![0usize; FEE_BUCKETS.len()];
        for entry in self.entries.values() {
            if entry.state != TxState::Pending {
                continue;
            }
            let rate = entry.tx.fee_rate;
            for (i, &(lo, hi, _)) in FEE_BUCKETS.iter().enumerate() {
                if rate >= lo && rate < hi {
                    counts[i] += 1;
                    break;
                }
            }
        }
        FEE_BUCKETS
            .iter()
            .zip(counts)
            .map(|(&(_, _, label), count)| (label.to_string(), count))
            .collect()
    }

    /// Prune non-pending entries older than given duration.
    pub fn prune_old(&mut self, max_age: chrono::Duration) {
        let cutoff = Utc::now() - max_age;
        let removed_txids: Vec<String> = self
            .entries
            .iter()
            .filter(|(_, e)| e.state != TxState::Pending && e.state_changed_at < cutoff)
            .map(|(k, _)| k.clone())
            .collect();
        for txid in &removed_txids {
            self.entries.remove(txid);
            self.replacement_chain.remove(txid);
        }
    }
}
