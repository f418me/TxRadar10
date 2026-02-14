use bitcoin::consensus::deserialize;
use bitcoin::Transaction;

/// Parse a raw transaction from bytes.
pub fn parse_raw_tx(raw: &[u8]) -> Result<Transaction, bitcoin::consensus::encode::Error> {
    deserialize(raw)
}

/// Check if any input signals RBF (sequence < 0xFFFFFFFE).
pub fn is_rbf_signaling(tx: &Transaction) -> bool {
    tx.input.iter().any(|inp| inp.sequence.0 < 0xFFFFFFFE)
}

/// Calculate vsize (weight / 4, rounded up).
pub fn vsize(tx: &Transaction) -> usize {
    let weight = tx.weight().to_wu() as usize;
    (weight + 3) / 4
}
