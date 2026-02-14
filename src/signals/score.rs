use crate::core::RuleScore;

/// Compute composite score (0-100) from individual rule scores.
pub fn compute_composite(scores: &[RuleScore]) -> f64 {
    let total_weighted: f64 = scores.iter().map(|s| s.weighted_score).sum();
    let max_possible: f64 = scores.iter().map(|s| s.weight.abs()).sum();

    if max_possible == 0.0 {
        return 0.0;
    }

    (total_weighted / max_possible * 100.0).clamp(0.0, 100.0)
}
