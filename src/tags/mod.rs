use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use bitcoin::{Address, Network, Transaction};
use serde::{Deserialize, Serialize};

use crate::db::SharedDatabase;

/// A tag identifying an address as belonging to a known entity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AddressTag {
    pub address: String,
    pub entity: String,
    pub entity_type: String,
    pub confidence: f64,
    pub source: Option<String>,
}

/// Direction of exchange flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FlowDirection {
    ToExchange,
    FromExchange,
}

/// A match between a transaction output/input and a known address.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TagMatch {
    pub address: String,
    pub tag: AddressTag,
    pub direction: FlowDirection,
}

/// Confidence multiplier for cluster-derived tags.
const CLUSTER_CONFIDENCE_FACTOR: f64 = 0.7;

/// In-memory lookup for fast address matching.
pub struct TagLookup {
    map: HashMap<String, AddressTag>,
    db: Option<SharedDatabase>,
    cluster_tags_discovered: AtomicU64,
}

impl TagLookup {
    /// Load all tags from the database into memory.
    pub fn load_from_db(db: &SharedDatabase) -> Self {
        let tags = db.all_tags().unwrap_or_default();
        let mut map = HashMap::with_capacity(tags.len());
        for tag in tags {
            map.insert(tag.address.clone(), tag);
        }
        tracing::info!("TagLookup loaded {} address tags into memory", map.len());
        Self {
            map,
            db: Some(db.clone()),
            cluster_tags_discovered: AtomicU64::new(0),
        }
    }

    /// Create an empty lookup.
    pub fn empty() -> Self {
        Self {
            map: HashMap::new(),
            db: None,
            cluster_tags_discovered: AtomicU64::new(0),
        }
    }

    /// Create an empty lookup with a database handle (for testing).
    #[cfg(test)]
    pub fn empty_with_db(db: SharedDatabase) -> Self {
        Self {
            map: HashMap::new(),
            db: Some(db),
            cluster_tags_discovered: AtomicU64::new(0),
        }
    }

    /// Number of loaded tags.
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Look up a single address.
    pub fn get(&self, address: &str) -> Option<&AddressTag> {
        self.map.get(address)
    }

    /// Check all outputs of a transaction against known addresses.
    pub fn check_outputs(&self, tx: &Transaction) -> Vec<TagMatch> {
        let mut matches = Vec::new();
        for output in &tx.output {
            if let Ok(addr) = Address::from_script(&output.script_pubkey, Network::Bitcoin) {
                let addr_str = addr.to_string();
                if let Some(tag) = self.map.get(&addr_str) {
                    matches.push(TagMatch {
                        address: addr_str,
                        tag: tag.clone(),
                        direction: FlowDirection::ToExchange,
                    });
                }
            }
        }
        matches
    }

    /// Check all inputs of a transaction against known addresses (requires prevout scripts).
    /// Since we don't have prevout scripts in the raw tx, this checks witness program / 
    /// script_sig patterns. In practice, input address extraction from raw tx is limited.
    /// We accept pre-resolved input addresses instead.
    pub fn check_input_addresses(&self, addresses: &[String]) -> Vec<TagMatch> {
        let mut matches = Vec::new();
        for addr_str in addresses {
            if let Some(tag) = self.map.get(addr_str) {
                matches.push(TagMatch {
                    address: addr_str.clone(),
                    tag: tag.clone(),
                    direction: FlowDirection::FromExchange,
                });
            }
        }
        matches
    }

    /// Expand tags using Common-Input-Ownership Heuristic (CIOH).
    ///
    /// If any input address has a known tag, all other input addresses get tagged
    /// with the same entity at reduced confidence. Skipped for CoinJoin transactions.
    ///
    /// Returns the number of new tags created.
    pub fn expand_from_tx(&mut self, input_addresses: &[String], is_coinjoin: bool) -> usize {
        // CoinJoin guard — CRITICAL: never cluster CoinJoin inputs
        if is_coinjoin {
            return 0;
        }

        // Need at least 2 inputs for clustering to make sense
        if input_addresses.len() < 2 {
            return 0;
        }

        // Find the best (highest confidence) existing tag among inputs
        let best_tag = input_addresses
            .iter()
            .filter_map(|addr| self.map.get(addr))
            .max_by(|a, b| a.confidence.partial_cmp(&b.confidence).unwrap_or(std::cmp::Ordering::Equal))
            .cloned();

        let best_tag = match best_tag {
            Some(t) => t,
            None => return 0, // no known tags among inputs
        };

        let derived_confidence = best_tag.confidence * CLUSTER_CONFIDENCE_FACTOR;
        let mut new_count = 0;

        for addr in input_addresses {
            // Skip if already tagged with equal or higher confidence
            if let Some(existing) = self.map.get(addr) {
                if existing.confidence >= derived_confidence {
                    continue;
                }
            }

            let new_tag = AddressTag {
                address: addr.clone(),
                entity: best_tag.entity.clone(),
                entity_type: best_tag.entity_type.clone(),
                confidence: derived_confidence,
                source: Some("cluster_heuristic".to_string()),
            };

            // Insert into in-memory map
            self.map.insert(addr.clone(), new_tag.clone());

            // Persist to DB
            if let Some(ref db) = self.db {
                if let Err(e) = db.insert_tag_if_higher(&new_tag) {
                    tracing::warn!("Failed to persist cluster tag for {}: {e}", addr);
                }
            }

            new_count += 1;
        }

        if new_count > 0 {
            let total = self.cluster_tags_discovered.fetch_add(new_count as u64, Ordering::Relaxed) + new_count as u64;
            tracing::info!("Cluster expansion: {new_count} new tags from tx (total discovered: {total})");
        }

        new_count
    }

    /// Insert a tag directly into the in-memory map (for setup/testing).
    pub fn insert(&mut self, tag: AddressTag) {
        self.map.insert(tag.address.clone(), tag);
    }

    /// Total number of tags discovered via cluster heuristic.
    pub fn cluster_tags_count(&self) -> u64 {
        self.cluster_tags_discovered.load(Ordering::Relaxed)
    }
}
