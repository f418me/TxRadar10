pub mod rules;
pub mod score;

use crate::core::{AlertLevel, AnalyzedTx, RuleScore, ScoredTx};
use rules::Rule;

/// The signal engine applies all rules and computes a composite score.
pub struct SignalEngine {
    rules: Vec<Box<dyn Rule + Send + Sync>>,
}

impl SignalEngine {
    pub fn new() -> Self {
        Self {
            rules: rules::default_rules(),
        }
    }

    pub fn score(&self, tx: &AnalyzedTx) -> ScoredTx {
        let rule_scores: Vec<RuleScore> = self
            .rules
            .iter()
            .map(|rule| {
                let raw_value = rule.evaluate(tx);
                let weight = rule.weight();
                RuleScore {
                    rule_name: rule.name().to_string(),
                    raw_value,
                    weight,
                    weighted_score: raw_value * weight,
                }
            })
            .collect();

        let composite = score::compute_composite(&rule_scores);
        let alert_level = AlertLevel::from_score(composite);

        ScoredTx {
            tx: tx.clone(),
            composite_score: composite,
            rule_scores,
            alert_level,
        }
    }
}
