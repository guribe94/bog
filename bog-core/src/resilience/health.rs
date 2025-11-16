//! Health Monitoring for Market Feed
//!
//! Monitors the health of the market data feed and signals when
//! the system is ready to trade.

use std::time::{Duration, Instant};
use crate::resilience::{GapDetector, StaleDataBreaker, StaleDataState};

/// Configuration for health monitoring
#[derive(Debug, Clone)]
pub struct HealthConfig {
    /// Minimum time before considering ready (allow stabilization)
    pub warmup_duration: Duration,
    /// Maximum acceptable gap size
    pub max_gap_size: u64,
    /// Stale data configuration
    pub stale_data_config: crate::resilience::StaleDataConfig,
}

impl Default for HealthConfig {
    fn default() -> Self {
        Self {
            warmup_duration: Duration::from_millis(500),
            max_gap_size: 100,
            stale_data_config: Default::default(),
        }
    }
}

/// Overall health status of the feed
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthStatus {
    /// System is initializing
    Initializing,
    /// System is ready to trade
    Ready,
    /// Data is stale, trading suspended
    Stale,
    /// Connection lost, trading halted
    Offline,
}

/// Health monitor combining multiple checks
pub struct FeedHealth {
    config: HealthConfig,
    gap_detector: GapDetector,
    stale_breaker: StaleDataBreaker,
    start_time: Instant,
    message_count: u64,
}

impl FeedHealth {
    /// Create new health monitor
    pub fn new(config: HealthConfig) -> Self {
        Self {
            gap_detector: GapDetector::new(),
            stale_breaker: StaleDataBreaker::new(config.stale_data_config.clone()),
            start_time: Instant::now(),
            config,
            message_count: 0,
        }
    }

    /// Report fresh message received
    pub fn report_message(&mut self, sequence: u64) {
        self.message_count += 1;
        let _gap_size = self.gap_detector.check(sequence);
        self.stale_breaker.mark_fresh();
    }

    /// Report no data available in this poll
    pub fn report_empty_poll(&mut self) {
        self.stale_breaker.mark_empty_poll();
    }

    /// Get overall health status
    pub fn status(&self) -> HealthStatus {
        // Check if still warming up
        if self.start_time.elapsed() < self.config.warmup_duration {
            return HealthStatus::Initializing;
        }

        // Check stale data state
        match self.stale_breaker.state() {
            StaleDataState::Fresh => HealthStatus::Ready,
            StaleDataState::Stale => HealthStatus::Stale,
            StaleDataState::Offline => HealthStatus::Offline,
        }
    }

    /// Check if ready to trade
    pub fn is_ready(&self) -> bool {
        self.status() == HealthStatus::Ready
    }

    /// Get number of messages processed
    pub fn message_count(&self) -> u64 {
        self.message_count
    }

    /// Get time since startup
    pub fn uptime(&self) -> Duration {
        self.start_time.elapsed()
    }

    /// Get last gap size
    pub fn last_gap_size(&self) -> u64 {
        self.gap_detector.last_gap_size()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_state_is_initializing() {
        let health = FeedHealth::new(HealthConfig::default());
        assert_eq!(health.status(), HealthStatus::Initializing);
        assert!(!health.is_ready());
    }

    #[test]
    fn test_becomes_ready_after_warmup() {
        let config = HealthConfig {
            warmup_duration: Duration::from_millis(10),
            ..Default::default()
        };
        let mut health = FeedHealth::new(config);

        // Report message
        health.report_message(1);

        // Wait for warmup
        std::thread::sleep(Duration::from_millis(20));

        assert_eq!(health.status(), HealthStatus::Ready);
        assert!(health.is_ready());
    }

    #[test]
    fn test_message_count_increments() {
        let mut health = FeedHealth::new(HealthConfig::default());

        assert_eq!(health.message_count(), 0);

        health.report_message(1);
        assert_eq!(health.message_count(), 1);

        health.report_message(2);
        assert_eq!(health.message_count(), 2);
    }

    #[test]
    fn test_stale_state_propagates() {
        let config = HealthConfig {
            warmup_duration: Duration::from_millis(10),
            stale_data_config: crate::resilience::StaleDataConfig {
                max_age: Duration::from_millis(50),
                max_empty_polls: 1000,
            },
            ..Default::default()
        };
        let mut health = FeedHealth::new(config);

        health.report_message(1);
        std::thread::sleep(Duration::from_millis(20));

        // At this point should be ready
        assert!(health.is_ready());

        // Wait longer and report empty poll
        std::thread::sleep(Duration::from_millis(100));
        health.report_empty_poll();

        // Should now be stale
        assert_eq!(health.status(), HealthStatus::Stale);
    }
}
