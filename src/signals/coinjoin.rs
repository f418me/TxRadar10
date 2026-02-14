use bitcoin::Transaction;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Known Whirlpool pool denominations in satoshis.
const WHIRLPOOL_POOLS: &[u64] = &[
    100_000,       // 0.001 BTC
    1_000_000,     // 0.01 BTC
    5_000_000,     // 0.05 BTC
    50_000_000,    // 0.5 BTC
];

/// Result of CoinJoin detection analysis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoinJoinResult {
    pub is_coinjoin: bool,
    pub confidence: f64,
    pub pattern: CoinJoinPattern,
}

/// Detected CoinJoin pattern type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CoinJoinPattern {
    WhirlpoolPool,
    WasabiLike,
    EqualOutput,
    Unknown,
}

impl Default for CoinJoinResult {
    fn default() -> Self {
        Self {
            is_coinjoin: false,
            confidence: 0.0,
            pattern: CoinJoinPattern::Unknown,
        }
    }
}

/// Detect whether a transaction is likely a CoinJoin.
///
/// Conservative: prefers false negatives over false positives.
/// No IO, runs in <100μs.
pub fn detect_coinjoin(tx: &Transaction) -> CoinJoinResult {
    let input_count = tx.input.len();
    let output_count = tx.output.len();

    // Quick exit: CoinJoin needs multiple participants
    if input_count < 3 || output_count < 3 {
        return CoinJoinResult::default();
    }

    // Count output values
    let mut value_counts: HashMap<u64, usize> = HashMap::new();
    for output in &tx.output {
        let sats = output.value.to_sat();
        *value_counts.entry(sats).or_insert(0) += 1;
    }

    // Find the most common output value with ≥3 occurrences
    let (best_value, best_count) = value_counts
        .iter()
        .filter(|(_, count)| **count >= 3)
        .max_by_key(|(_, count)| **count)
        .map(|(v, c)| (*v, *c))
        .unwrap_or((0, 0));

    if best_count < 3 {
        return CoinJoinResult::default();
    }

    // Equal outputs must be >50% of all outputs
    let equal_ratio = best_count as f64 / output_count as f64;
    if equal_ratio <= 0.5 {
        return CoinJoinResult::default();
    }

    // Many inputs + many outputs strengthens the signal
    let many_io = input_count >= 5 && output_count >= 5;

    // Check Whirlpool: exactly 5 equal outputs at a known pool size
    if best_count == 5 && WHIRLPOOL_POOLS.contains(&best_value) && many_io {
        return CoinJoinResult {
            is_coinjoin: true,
            confidence: 0.95,
            pattern: CoinJoinPattern::WhirlpoolPool,
        };
    }

    // Check Wasabi-like: many equal outputs (≥5), round denominations
    let is_round = best_value % 100_000 == 0 && best_value > 0; // multiple of 0.001 BTC
    if best_count >= 5 && many_io {
        let confidence = if is_round { 0.85 } else { 0.75 };
        let pattern = if is_round && best_count >= 10 {
            CoinJoinPattern::WasabiLike
        } else {
            CoinJoinPattern::EqualOutput
        };
        return CoinJoinResult {
            is_coinjoin: true,
            confidence,
            pattern,
        };
    }

    // Weaker signal: ≥3 equal outputs >50%, but not many IO
    // Only flag if equal_ratio is very high (>70%) — conservative
    if equal_ratio > 0.7 && best_count >= 3 {
        return CoinJoinResult {
            is_coinjoin: true,
            confidence: 0.5,
            pattern: CoinJoinPattern::EqualOutput,
        };
    }

    CoinJoinResult::default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitcoin::{Amount, ScriptBuf, TxIn, TxOut, Txid};

    fn make_tx(input_count: usize, outputs_sats: &[u64]) -> Transaction {
        let inputs: Vec<TxIn> = (0..input_count)
            .map(|_| TxIn::default())
            .collect();
        let outputs: Vec<TxOut> = outputs_sats
            .iter()
            .map(|&sats| TxOut {
                value: Amount::from_sat(sats),
                script_pubkey: ScriptBuf::new(),
            })
            .collect();
        Transaction {
            version: bitcoin::transaction::Version(2),
            lock_time: bitcoin::locktime::absolute::LockTime::ZERO,
            input: inputs,
            output: outputs,
        }
    }

    #[test]
    fn test_not_coinjoin_simple() {
        let tx = make_tx(1, &[50_000, 100_000]);
        let result = detect_coinjoin(&tx);
        assert!(!result.is_coinjoin);
    }

    #[test]
    fn test_whirlpool_detected() {
        // 5 equal outputs at 0.01 BTC pool, 5 inputs
        let mut outputs = vec![1_000_000; 5];
        outputs.push(50_000); // change
        let tx = make_tx(5, &outputs);
        let result = detect_coinjoin(&tx);
        assert!(result.is_coinjoin);
        assert_eq!(result.pattern, CoinJoinPattern::WhirlpoolPool);
        assert!(result.confidence >= 0.9);
    }

    #[test]
    fn test_wasabi_like_detected() {
        // 20 equal outputs at 0.1 BTC, 15 inputs
        let mut outputs = vec![10_000_000; 20];
        outputs.extend_from_slice(&[500_000, 300_000, 200_000]); // change outputs
        let tx = make_tx(15, &outputs);
        let result = detect_coinjoin(&tx);
        assert!(result.is_coinjoin);
        assert_eq!(result.pattern, CoinJoinPattern::WasabiLike);
    }

    #[test]
    fn test_equal_output_detected() {
        // 8 equal outputs, non-round amount, 6 inputs
        let mut outputs = vec![1_234_567; 8];
        outputs.push(50_000); // change
        let tx = make_tx(6, &outputs);
        let result = detect_coinjoin(&tx);
        assert!(result.is_coinjoin);
        assert_eq!(result.pattern, CoinJoinPattern::EqualOutput);
    }

    #[test]
    fn test_not_coinjoin_few_equal() {
        // Only 2 equal outputs — not enough
        let tx = make_tx(5, &[100_000, 100_000, 200_000, 300_000, 400_000]);
        let result = detect_coinjoin(&tx);
        assert!(!result.is_coinjoin);
    }
}
