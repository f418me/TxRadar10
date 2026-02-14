use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::config::NotificationConfig;
use crate::core::ScoredTx;

/// Desktop notification sender with cooldown to prevent spam.
pub struct Notifier {
    enabled: bool,
    min_score: f64,
    cooldown: Duration,
    last_sent: Mutex<Option<Instant>>,
}

impl Notifier {
    pub fn new(config: &NotificationConfig) -> Self {
        Self {
            enabled: config.enabled,
            min_score: config.min_score,
            cooldown: Duration::from_secs(config.cooldown_seconds),
            last_sent: Mutex::new(None),
        }
    }

    /// Try to send a desktop notification for a scored transaction.
    /// Returns true if a notification was sent, false if skipped.
    pub fn notify(&self, scored_tx: &ScoredTx) -> bool {
        if !self.enabled {
            return false;
        }
        if scored_tx.composite_score < self.min_score {
            return false;
        }
        if !self.check_cooldown() {
            return false;
        }

        self.send_notification(scored_tx);
        true
    }

    /// Check and update cooldown. Returns true if enough time has passed.
    fn check_cooldown(&self) -> bool {
        let mut last = self.last_sent.lock().unwrap();
        let now = Instant::now();
        if let Some(prev) = *last {
            if now.duration_since(prev) < self.cooldown {
                return false;
            }
        }
        *last = Some(now);
        true
    }

    /// Fire-and-forget: send the actual desktop notification.
    fn send_notification(&self, scored_tx: &ScoredTx) {
        let title = format!("⚡ TxRadar10 — {:?}", scored_tx.alert_level);
        let btc_value = scored_tx.tx.total_input_value as f64 / 100_000_000.0;
        let txid_short = &scored_tx.tx.txid[..8.min(scored_tx.tx.txid.len())];
        let mut body = format!("{:.0} | {btc_value:.4} BTC | {txid_short}", scored_tx.composite_score);
        if scored_tx.tx.to_exchange {
            body.push_str(" → Exchange detected");
        }

        // Fire-and-forget in a background thread to never block the pipeline
        std::thread::spawn(move || {
            if let Err(e) = notify_rust::Notification::new()
                .summary(&title)
                .body(&body)
                .show()
            {
                tracing::debug!("Desktop notification failed: {e}");
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::NotificationConfig;
    use crate::core::{AlertLevel, AnalyzedTx, ScoredTx};
    use chrono::Utc;

    fn make_scored(score: f64, to_exchange: bool) -> ScoredTx {
        ScoredTx {
            tx: AnalyzedTx {
                txid: "aabbccdd11223344".to_string(),
                raw_size: 250,
                vsize: 200,
                total_input_value: 500_000_000,
                total_output_value: 499_000_000,
                fee: 1_000_000,
                fee_rate: 50.0,
                input_count: 2,
                output_count: 2,
                oldest_input_height: None,
                oldest_input_time: None,
                coin_days_destroyed: None,
                is_rbf_signaling: false,
                seen_at: Utc::now(),
                prevouts_resolved: true,
                to_exchange,
                to_exchange_confidence: if to_exchange { 0.9 } else { 0.0 },
                from_exchange: false,
                from_exchange_confidence: 0.0,
                is_coinjoin: false,
                coinjoin_confidence: 0.0,
            },
            composite_score: score,
            rule_scores: vec![],
            alert_level: AlertLevel::from_score(score),
        }
    }

    #[test]
    fn cooldown_blocks_rapid_notifications() {
        let config = NotificationConfig {
            enabled: true,
            min_score: 60.0,
            cooldown_seconds: 30,
        };
        let notifier = Notifier::new(&config);

        // First call should pass cooldown
        assert!(notifier.check_cooldown());
        // Second call immediately should be blocked
        assert!(!notifier.check_cooldown());
    }

    #[test]
    fn cooldown_zero_allows_all() {
        let config = NotificationConfig {
            enabled: true,
            min_score: 60.0,
            cooldown_seconds: 0,
        };
        let notifier = Notifier::new(&config);
        assert!(notifier.check_cooldown());
        assert!(notifier.check_cooldown());
    }

    #[test]
    fn disabled_notifier_skips() {
        let config = NotificationConfig {
            enabled: false,
            min_score: 60.0,
            cooldown_seconds: 0,
        };
        let notifier = Notifier::new(&config);
        let tx = make_scored(90.0, false);
        assert!(!notifier.notify(&tx));
    }

    #[test]
    fn below_min_score_skips() {
        let config = NotificationConfig {
            enabled: true,
            min_score: 60.0,
            cooldown_seconds: 0,
        };
        let notifier = Notifier::new(&config);
        let tx = make_scored(50.0, false);
        assert!(!notifier.notify(&tx));
    }
}
