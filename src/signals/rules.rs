use crate::core::AnalyzedTx;

/// A scoring rule that evaluates a single aspect of a transaction.
/// Returns a normalized value 0.0-1.0 (or negative for penalty rules like CoinJoin).
pub trait Rule {
    fn name(&self) -> &str;
    fn weight(&self) -> f64;
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
    ]
}

// --- Individual Rules ---

struct TxValueRule;
impl Rule for TxValueRule {
    fn name(&self) -> &str { "tx_value" }
    fn weight(&self) -> f64 { 6.0 }
    fn evaluate(&self, tx: &AnalyzedTx) -> f64 {
        let btc = tx.total_input_value as f64 / 100_000_000.0;
        // Sigmoid-like: 0 at 0 BTC, ~0.5 at 10 BTC, ~0.9 at 100 BTC
        1.0 - 1.0 / (1.0 + btc / 10.0)
    }
}

struct UtxoAgeRule;
impl Rule for UtxoAgeRule {
    fn name(&self) -> &str { "utxo_age" }
    fn weight(&self) -> f64 { 8.0 }
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
    fn weight(&self) -> f64 { 9.0 }
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
    fn weight(&self) -> f64 { 4.0 }
    fn evaluate(&self, tx: &AnalyzedTx) -> f64 {
        // Many inputs → consolidation signal (but ambiguous)
        let count = tx.input_count as f64;
        1.0 - 1.0 / (1.0 + count / 20.0)
    }
}

struct FeeRateRule;
impl Rule for FeeRateRule {
    fn name(&self) -> &str { "fee_rate" }
    fn weight(&self) -> f64 { 3.0 }
    fn evaluate(&self, tx: &AnalyzedTx) -> f64 {
        // High fee rate = urgency. ~0.5 at 50 sat/vB, ~0.9 at 500 sat/vB
        1.0 - 1.0 / (1.0 + tx.fee_rate / 50.0)
    }
}

struct RbfRule;
impl Rule for RbfRule {
    fn name(&self) -> &str { "rbf_flag" }
    fn weight(&self) -> f64 { 2.0 }
    fn evaluate(&self, tx: &AnalyzedTx) -> f64 {
        if tx.is_rbf_signaling { 0.5 } else { 0.0 }
    }
}
