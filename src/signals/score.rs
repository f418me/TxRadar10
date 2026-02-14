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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_score(name: &str, raw: f64, weight: f64) -> RuleScore {
        RuleScore {
            rule_name: name.to_string(),
            raw_value: raw,
            weight,
            weighted_score: raw * weight,
        }
    }

    #[test]
    fn empty_scores() {
        assert_eq!(compute_composite(&[]), 0.0);
    }

    #[test]
    fn single_full_score() {
        let scores = vec![make_score("test", 1.0, 10.0)];
        assert!((compute_composite(&scores) - 100.0).abs() < 0.01);
    }

    #[test]
    fn single_half_score() {
        let scores = vec![make_score("test", 0.5, 10.0)];
        assert!((compute_composite(&scores) - 50.0).abs() < 0.01);
    }

    #[test]
    fn multiple_scores() {
        let scores = vec![
            make_score("a", 1.0, 6.0),  // 6.0
            make_score("b", 0.5, 4.0),  // 2.0
        ];
        // total_weighted = 8.0, max_possible = 10.0 → 80.0
        assert!((compute_composite(&scores) - 80.0).abs() < 0.01);
    }

    #[test]
    fn negative_weight_reduces_score() {
        let scores = vec![
            make_score("a", 1.0, 10.0),   // 10.0
            make_score("cj", 1.0, -6.0),  // -6.0
        ];
        // total_weighted = 4.0, max_possible = 16.0 → 25.0
        assert!((compute_composite(&scores) - 25.0).abs() < 0.01);
    }

    #[test]
    fn clamped_to_zero() {
        let scores = vec![
            make_score("a", 0.0, 10.0),
            make_score("cj", 1.0, -6.0),
        ];
        // total_weighted = -6.0, clamped to 0
        assert_eq!(compute_composite(&scores), 0.0);
    }

    #[test]
    fn zero_weights() {
        let scores = vec![make_score("a", 1.0, 0.0)];
        assert_eq!(compute_composite(&scores), 0.0);
    }
}
