use crate::core::AnalyzedTx;

/// A scoring rule that evaluates a single aspect of a transaction.
/// Returns a normalized value 0.0-1.0 (or negative for penalty rules like CoinJoin).
pub trait Rule {
    fn name(&self) -> &str;
    fn default_weight(&self) -> f64;
    fn evaluate(&self, tx: &AnalyzedTx) -> f64;
}

/// Return all default rules with initial weights.
pub fn default_rules() -> Vec<Box<dyn Rule + Send + Sync>> {
    vec![
        Box::new(TxValueRule),
        Box::new(UtxoAgeRule),
        Box::new(CoinDaysDestroyedRule),
        Box::new(InputCountRule),
        Box::new(FeeRateRule),
        Box::new(RbfRule),
        Box::new(ExchangeFlowRule),
        Box::new(CoinJoinRule),
    ]
}

// --- Individual Rules ---

struct TxValueRule;
impl Rule for TxValueRule {
    fn name(&self) -> &str { "tx_value" }
    fn default_weight(&self) -> f64 { 6.0 }
    fn evaluate(&self, tx: &AnalyzedTx) -> f64 {
        let btc = tx.total_input_value as f64 / 100_000_000.0;
        // Sigmoid-like: 0 at 0 BTC, ~0.5 at 10 BTC, ~0.9 at 100 BTC
        1.0 - 1.0 / (1.0 + btc / 10.0)
    }
}

struct UtxoAgeRule;
impl Rule for UtxoAgeRule {
    fn name(&self) -> &str { "utxo_age" }
    fn default_weight(&self) -> f64 { 8.0 }
    fn evaluate(&self, tx: &AnalyzedTx) -> f64 {
        match tx.oldest_input_time {
            Some(time) => {
                let age_days = (chrono::Utc::now() - time).num_days() as f64;
                // Sigmoid: ~0.5 at 365 days, ~0.9 at 2000 days
                1.0 - 1.0 / (1.0 + age_days / 365.0)
            }
            None => 0.0, // unresolved prevouts
        }
    }
}

struct CoinDaysDestroyedRule;
impl Rule for CoinDaysDestroyedRule {
    fn name(&self) -> &str { "cdd" }
    fn default_weight(&self) -> f64 { 9.0 }
    fn evaluate(&self, tx: &AnalyzedTx) -> f64 {
        match tx.coin_days_destroyed {
            Some(cdd) => {
                // Sigmoid: ~0.5 at 1000 CDD, ~0.9 at 10000 CDD
                1.0 - 1.0 / (1.0 + cdd / 1000.0)
            }
            None => 0.0,
        }
    }
}

struct InputCountRule;
impl Rule for InputCountRule {
    fn name(&self) -> &str { "input_count" }
    fn default_weight(&self) -> f64 { 4.0 }
    fn evaluate(&self, tx: &AnalyzedTx) -> f64 {
        // Many inputs → consolidation signal (but ambiguous)
        let count = tx.input_count as f64;
        1.0 - 1.0 / (1.0 + count / 20.0)
    }
}

struct FeeRateRule;
impl Rule for FeeRateRule {
    fn name(&self) -> &str { "fee_rate" }
    fn default_weight(&self) -> f64 { 3.0 }
    fn evaluate(&self, tx: &AnalyzedTx) -> f64 {
        // High fee rate = urgency. ~0.5 at 50 sat/vB, ~0.9 at 500 sat/vB
        1.0 - 1.0 / (1.0 + tx.fee_rate / 50.0)
    }
}

struct RbfRule;
impl Rule for RbfRule {
    fn name(&self) -> &str { "rbf_flag" }
    fn default_weight(&self) -> f64 { 2.0 }
    fn evaluate(&self, tx: &AnalyzedTx) -> f64 {
        if tx.is_rbf_signaling { 0.5 } else { 0.0 }
    }
}

/// CoinJoin detection — negative weight to reduce false positives.
/// CoinJoin transactions are privacy txs, not directional signals.
struct CoinJoinRule;
impl Rule for CoinJoinRule {
    fn name(&self) -> &str { "coinjoin" }
    fn default_weight(&self) -> f64 { -6.0 }
    fn evaluate(&self, tx: &AnalyzedTx) -> f64 {
        if tx.is_coinjoin {
            tx.coinjoin_confidence.clamp(0.0, 1.0)
        } else {
            0.0
        }
    }
}

/// Exchange flow detection — the highest-weight signal.
/// Outputs going to known exchanges indicate potential sell pressure.
/// Inputs from exchanges (withdrawals) reduce the score.
struct ExchangeFlowRule;
impl Rule for ExchangeFlowRule {
    fn name(&self) -> &str { "exchange_flow" }
    fn default_weight(&self) -> f64 { 10.0 }
    fn evaluate(&self, tx: &AnalyzedTx) -> f64 {
        if tx.to_exchange {
            // Score weighted by confidence of the tag match
            tx.to_exchange_confidence.clamp(0.0, 1.0)
        } else if tx.from_exchange {
            // Withdrawal from exchange — reduces alarm (negative contribution)
            -(tx.from_exchange_confidence.clamp(0.0, 1.0) * 0.5)
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_test_tx() -> AnalyzedTx {
        AnalyzedTx {
            txid: "deadbeef".to_string(),
            raw_size: 250,
            vsize: 200,
            total_input_value: 0,
            total_output_value: 0,
            fee: 2000,
            fee_rate: 10.0,
            input_count: 1,
            output_count: 2,
            oldest_input_height: None,
            oldest_input_time: None,
            coin_days_destroyed: None,
            is_rbf_signaling: false,
            seen_at: Utc::now(),
            prevouts_resolved: false,
            to_exchange: false,
            to_exchange_confidence: 0.0,
            from_exchange: false,
            from_exchange_confidence: 0.0,
            is_coinjoin: false,
            coinjoin_confidence: 0.0,
        }
    }

    #[test]
    fn tx_value_zero() {
        let rule = TxValueRule;
        let tx = make_test_tx();
        assert!((rule.evaluate(&tx) - 0.0).abs() < 0.001);
    }

    #[test]
    fn tx_value_midpoint() {
        let rule = TxValueRule;
        let mut tx = make_test_tx();
        tx.total_input_value = 10_0000_0000; // 10 BTC
        let score = rule.evaluate(&tx);
        assert!((score - 0.5).abs() < 0.01, "Expected ~0.5, got {score}");
    }

    #[test]
    fn tx_value_high() {
        let rule = TxValueRule;
        let mut tx = make_test_tx();
        tx.total_input_value = 1000_0000_0000; // 1000 BTC
        let score = rule.evaluate(&tx);
        assert!(score > 0.98, "Expected ~1.0, got {score}");
    }

    #[test]
    fn utxo_age_none() {
        let rule = UtxoAgeRule;
        let tx = make_test_tx();
        assert_eq!(rule.evaluate(&tx), 0.0);
    }

    #[test]
    fn utxo_age_one_year() {
        let rule = UtxoAgeRule;
        let mut tx = make_test_tx();
        tx.oldest_input_time = Some(Utc::now() - chrono::Duration::days(365));
        let score = rule.evaluate(&tx);
        assert!((score - 0.5).abs() < 0.05, "Expected ~0.5, got {score}");
    }

    #[test]
    fn cdd_none() {
        let rule = CoinDaysDestroyedRule;
        let tx = make_test_tx();
        assert_eq!(rule.evaluate(&tx), 0.0);
    }

    #[test]
    fn cdd_midpoint() {
        let rule = CoinDaysDestroyedRule;
        let mut tx = make_test_tx();
        tx.coin_days_destroyed = Some(1000.0);
        let score = rule.evaluate(&tx);
        assert!((score - 0.5).abs() < 0.01);
    }

    #[test]
    fn cdd_high() {
        let rule = CoinDaysDestroyedRule;
        let mut tx = make_test_tx();
        tx.coin_days_destroyed = Some(100_000.0);
        assert!(rule.evaluate(&tx) > 0.98);
    }

    #[test]
    fn cdd_zero_value() {
        let rule = CoinDaysDestroyedRule;
        let mut tx = make_test_tx();
        tx.coin_days_destroyed = Some(0.0);
        assert!((rule.evaluate(&tx)).abs() < 0.001);
    }

    #[test]
    fn input_count_single() {
        let rule = InputCountRule;
        let mut tx = make_test_tx();
        tx.input_count = 1;
        assert!(rule.evaluate(&tx) < 0.1);
    }

    #[test]
    fn input_count_midpoint() {
        let rule = InputCountRule;
        let mut tx = make_test_tx();
        tx.input_count = 20;
        let score = rule.evaluate(&tx);
        assert!((score - 0.5).abs() < 0.01);
    }

    #[test]
    fn fee_rate_zero() {
        let rule = FeeRateRule;
        let mut tx = make_test_tx();
        tx.fee_rate = 0.0;
        assert!((rule.evaluate(&tx)).abs() < 0.001);
    }

    #[test]
    fn fee_rate_midpoint() {
        let rule = FeeRateRule;
        let mut tx = make_test_tx();
        tx.fee_rate = 50.0;
        let score = rule.evaluate(&tx);
        assert!((score - 0.5).abs() < 0.01);
    }

    #[test]
    fn rbf_signaling() {
        let rule = RbfRule;
        let mut tx = make_test_tx();
        tx.is_rbf_signaling = true;
        assert_eq!(rule.evaluate(&tx), 0.5);
    }

    #[test]
    fn rbf_not_signaling() {
        let rule = RbfRule;
        let tx = make_test_tx();
        assert_eq!(rule.evaluate(&tx), 0.0);
    }

    #[test]
    fn coinjoin_not_detected() {
        let rule = CoinJoinRule;
        let tx = make_test_tx();
        assert_eq!(rule.evaluate(&tx), 0.0);
    }

    #[test]
    fn coinjoin_high_confidence() {
        let rule = CoinJoinRule;
        let mut tx = make_test_tx();
        tx.is_coinjoin = true;
        tx.coinjoin_confidence = 0.95;
        assert!((rule.evaluate(&tx) - 0.95).abs() < 0.001);
    }

    #[test]
    fn coinjoin_clamped_above_one() {
        let rule = CoinJoinRule;
        let mut tx = make_test_tx();
        tx.is_coinjoin = true;
        tx.coinjoin_confidence = 1.5;
        assert_eq!(rule.evaluate(&tx), 1.0);
    }

    #[test]
    fn exchange_flow_to_exchange() {
        let rule = ExchangeFlowRule;
        let mut tx = make_test_tx();
        tx.to_exchange = true;
        tx.to_exchange_confidence = 0.8;
        assert!((rule.evaluate(&tx) - 0.8).abs() < 0.001);
    }

    #[test]
    fn exchange_flow_from_exchange() {
        let rule = ExchangeFlowRule;
        let mut tx = make_test_tx();
        tx.from_exchange = true;
        tx.from_exchange_confidence = 1.0;
        let score = rule.evaluate(&tx);
        assert!((score - (-0.5)).abs() < 0.001);
    }

    #[test]
    fn exchange_flow_neither() {
        let rule = ExchangeFlowRule;
        let tx = make_test_tx();
        assert_eq!(rule.evaluate(&tx), 0.0);
    }

    #[test]
    fn exchange_flow_both_prefers_to() {
        let rule = ExchangeFlowRule;
        let mut tx = make_test_tx();
        tx.to_exchange = true;
        tx.to_exchange_confidence = 0.9;
        tx.from_exchange = true;
        tx.from_exchange_confidence = 0.8;
        assert!((rule.evaluate(&tx) - 0.9).abs() < 0.001);
    }

    #[test]
    fn default_rules_count() {
        let rules = default_rules();
        assert_eq!(rules.len(), 8);
    }

    #[test]
    fn all_rules_names_unique() {
        let rules = default_rules();
        let mut names: Vec<&str> = rules.iter().map(|r| r.name()).collect();
        let len = names.len();
        names.sort();
        names.dedup();
        assert_eq!(len, names.len());
    }
}
