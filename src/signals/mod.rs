pub mod rules;
pub mod score;

use std::collections::HashMap;

use crate::config::AlertThresholds;
use crate::core::{AlertLevel, AnalyzedTx, RuleScore, ScoredTx};
use rules::Rule;

/// The signal engine applies all rules and computes a composite score.
pub struct SignalEngine {
    rules: Vec<Box<dyn Rule + Send + Sync>>,
    weight_overrides: HashMap<String, f64>,
    thresholds: AlertThresholds,
}

impl SignalEngine {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {
            rules: rules::default_rules(),
            weight_overrides: HashMap::new(),
            thresholds: AlertThresholds::default(),
        }
    }

    pub fn with_config(weights: HashMap<String, f64>, thresholds: AlertThresholds) -> Self {
        Self {
            rules: rules::default_rules(),
            weight_overrides: weights,
            thresholds,
        }
    }

    pub fn score(&self, tx: &AnalyzedTx) -> ScoredTx {
        let rule_scores: Vec<RuleScore> = self
            .rules
            .iter()
            .map(|rule| {
                let raw_value = rule.evaluate(tx);
                let weight = self
                    .weight_overrides
                    .get(rule.name())
                    .copied()
                    .unwrap_or_else(|| rule.default_weight());
                RuleScore {
                    rule_name: rule.name().to_string(),
                    raw_value,
                    weight,
                    weighted_score: raw_value * weight,
                }
            })
            .collect();

        let composite = score::compute_composite(&rule_scores);
        let alert_level = AlertLevel::from_score_with_thresholds(
            composite,
            self.thresholds.critical,
            self.thresholds.high,
            self.thresholds.medium,
        );

        ScoredTx {
            tx: tx.clone(),
            composite_score: composite,
            rule_scores,
            alert_level,
        }
    }
}
