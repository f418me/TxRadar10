use std::collections::HashMap;

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

/// In-memory lookup for fast address matching.
pub struct TagLookup {
    map: HashMap<String, AddressTag>,
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
        Self { map }
    }

    /// Create an empty lookup.
    pub fn empty() -> Self {
        Self {
            map: HashMap::new(),
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
}
