//! Stale Data Circuit Breaker
//!
//! Prevents trading on stale market data by detecting when:
//! - Market data hasn't been updated for N seconds (configurable)
//! - Huginn has disconnected or is blocked
//! - Network is degraded
//!
//! Uses a simple state machine to track data freshness.

use std::time::{Duration, Instant};

/// Configuration for stale data detection
#[derive(Debug, Clone)]
pub struct StaleDataConfig {
    /// Maximum age of data before considered stale (default: 5 seconds)
    pub max_age: Duration,
    /// Maximum number of consecutive empty polls before alert (default: 1000)
    pub max_empty_polls: u64,
}

impl Default for StaleDataConfig {
    fn default() -> Self {
        Self {
            max_age: Duration::from_secs(5),
            max_empty_polls: 1000,
        }
    }
}

/// State of the stale data detector
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StaleDataState {
    /// Data is fresh and trading is OK
    Fresh,
    /// Data is stale, trading should stop
    Stale,
    /// Huginn is down or disconnected
    Offline,
}

/// Detects stale market data and halts trading
///
/// Target: <5ns inline check
#[derive(Debug)]
pub struct StaleDataBreaker {
    config: StaleDataConfig,
    state: StaleDataState,
    last_update: Instant,
    consecutive_empty_polls: u64,
}

impl StaleDataBreaker {
    /// Create new stale data breaker
    pub fn new(config: StaleDataConfig) -> Self {
        Self {
            config,
            state: StaleDataState::Fresh,
            last_update: Instant::now(),
            consecutive_empty_polls: 0,
        }
    }

    /// Check if data is fresh (called before every trade)
    ///
    /// Returns true if data is fresh, false if stale/offline
    /// Must be <5ns for hot path suitability
    #[inline(always)]
    pub fn is_fresh(&self) -> bool {
        self.state == StaleDataState::Fresh
    }

    /// Report that fresh data was received
    ///
    /// Reset stale detection counters
    #[inline]
    pub fn mark_fresh(&mut self) {
        self.last_update = Instant::now();
        self.consecutive_empty_polls = 0;
        self.state = StaleDataState::Fresh;
    }

    /// Report that no data was available (empty poll)
    ///
    /// Increment stale detection counter.
    /// Note: Empty polls are normal when consumer catches up to producer.
    /// We only mark as stale/offline based on actual data age, not poll count alone.
    #[inline]
    pub fn mark_empty_poll(&mut self) {
        self.consecutive_empty_polls += 1;

        // Check actual data age - this is the real indicator of staleness
        let age = self.last_update.elapsed();

        // Only transition to Offline if BOTH conditions are met:
        // 1. Too many empty polls (producer might be dead)
        // 2. Data is actually old (not just consumer caught up)
        if self.consecutive_empty_polls > self.config.max_empty_polls && age >= self.config.max_age
        {
            self.state = StaleDataState::Offline;
            return;
        }

        // Only transition to Stale if data is actually old
        // This prevents false positives when consumer is just caught up with producer
        if age >= self.config.max_age {
            self.state = StaleDataState::Stale;
        }
        // If data is fresh (age < max_age), keep current state (likely Fresh)
    }

    /// Get current state
    pub fn state(&self) -> StaleDataState {
        self.state
    }

    /// Check if data is stale
    pub fn is_stale(&self) -> bool {
        self.state == StaleDataState::Stale
    }

    /// Check if Huginn is offline
    pub fn is_offline(&self) -> bool {
        self.state == StaleDataState::Offline
    }

    /// Get time since last fresh data
    pub fn time_since_update(&self) -> Duration {
        self.last_update.elapsed()
    }

    /// Reset breaker (for recovery)
    pub fn reset(&mut self) {
        self.last_update = Instant::now();
        self.consecutive_empty_polls = 0;
        self.state = StaleDataState::Fresh;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_state_is_fresh() {
        let breaker = StaleDataBreaker::new(StaleDataConfig::default());
        assert!(breaker.is_fresh());
        assert_eq!(breaker.state(), StaleDataState::Fresh);
    }

    #[test]
    fn test_mark_fresh_resets_state() {
        // Use custom config with low threshold so we can trigger non-fresh state
        let config = StaleDataConfig {
            max_age: Duration::from_millis(50),
            max_empty_polls: 5,
        };
        let mut breaker = StaleDataBreaker::new(config);

        // Wait for data to become stale first
        std::thread::sleep(Duration::from_millis(100));

        // Mark enough empty polls to trigger offline state
        // Now BOTH conditions are met: data is old AND many empty polls
        for _ in 0..6 {
            breaker.mark_empty_poll();
        }
        assert!(!breaker.is_fresh());
        assert!(breaker.is_offline());

        breaker.mark_fresh();
        assert!(breaker.is_fresh());
        assert_eq!(breaker.consecutive_empty_polls, 0);
    }

    #[test]
    fn test_empty_polls_increment() {
        let mut breaker = StaleDataBreaker::new(StaleDataConfig::default());
        assert_eq!(breaker.consecutive_empty_polls, 0);

        breaker.mark_empty_poll();
        assert_eq!(breaker.consecutive_empty_polls, 1);

        breaker.mark_empty_poll();
        assert_eq!(breaker.consecutive_empty_polls, 2);
    }

    #[test]
    fn test_offline_detection() {
        let config = StaleDataConfig {
            max_age: Duration::from_millis(50),
            max_empty_polls: 10,
        };
        let mut breaker = StaleDataBreaker::new(config);

        // Wait for data to become stale
        std::thread::sleep(Duration::from_millis(100));

        // Mark 11 empty polls - now both conditions (stale + many polls) are met
        for _ in 0..11 {
            breaker.mark_empty_poll();
        }

        assert!(breaker.is_offline());
        assert_eq!(breaker.state(), StaleDataState::Offline);
    }

    #[test]
    fn test_empty_polls_dont_cause_stale_when_data_recent() {
        let config = StaleDataConfig {
            max_age: Duration::from_secs(5),
            max_empty_polls: 10,
        };
        let mut breaker = StaleDataBreaker::new(config);

        // Receive fresh data
        breaker.mark_fresh();

        // Many empty polls immediately after (consumer caught up)
        for _ in 0..15 {
            breaker.mark_empty_poll();
        }

        // Should still be fresh because data age < max_age
        // This is the key fix: empty polls alone don't cause staleness
        assert!(breaker.is_fresh());
    }

    #[test]
    fn test_stale_detection_by_age() {
        let config = StaleDataConfig {
            max_age: Duration::from_millis(100),
            max_empty_polls: 10000,
        };
        let mut breaker = StaleDataBreaker::new(config);

        // Simulate time passing
        std::thread::sleep(Duration::from_millis(150));

        // Mark empty poll (triggers stale check)
        breaker.mark_empty_poll();

        assert!(breaker.is_stale());
        assert_eq!(breaker.state(), StaleDataState::Stale);
    }

    #[test]
    fn test_is_fresh_inline_safe() {
        let breaker = StaleDataBreaker::new(StaleDataConfig::default());

        // This should be inlineable without issues
        let result = breaker.is_fresh();
        assert!(result);
    }

    #[test]
    fn test_reset_clears_state() {
        let config = StaleDataConfig {
            max_age: Duration::from_millis(50),
            max_empty_polls: 10,
        };
        let mut breaker = StaleDataBreaker::new(config);

        // Wait for data to become stale first
        std::thread::sleep(Duration::from_millis(100));

        // Mark offline - now BOTH conditions are met
        for _ in 0..11 {
            breaker.mark_empty_poll();
        }
        assert!(breaker.is_offline());

        // Reset
        breaker.reset();

        assert!(breaker.is_fresh());
        assert_eq!(breaker.consecutive_empty_polls, 0);
    }
}
