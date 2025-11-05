//! Exponential backoff for retry logic
//!
//! Provides configurable exponential backoff with jitter to prevent
//! thundering herd problems in distributed systems.

use rand::Rng;
use std::time::Duration;

/// Configuration for exponential backoff
#[derive(Debug, Clone)]
pub struct BackoffConfig {
    /// Initial delay before first retry
    pub initial_delay: Duration,
    /// Maximum delay between retries
    pub max_delay: Duration,
    /// Multiplier for each retry (typically 2.0)
    pub multiplier: f64,
    /// Maximum number of retry attempts (None = unlimited)
    pub max_retries: Option<usize>,
    /// Add randomization to prevent thundering herd (0.0 to 1.0)
    pub jitter_factor: f64,
}

impl Default for BackoffConfig {
    fn default() -> Self {
        Self {
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(30),
            multiplier: 2.0,
            max_retries: Some(10),
            jitter_factor: 0.1, // 10% jitter
        }
    }
}

impl BackoffConfig {
    /// Create a configuration for aggressive retries (for testing)
    pub fn aggressive() -> Self {
        Self {
            initial_delay: Duration::from_millis(10),
            max_delay: Duration::from_secs(1),
            multiplier: 1.5,
            max_retries: Some(5),
            jitter_factor: 0.1,
        }
    }

    /// Create a configuration for conservative retries (for production)
    pub fn conservative() -> Self {
        Self {
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(60),
            multiplier: 2.0,
            max_retries: Some(20),
            jitter_factor: 0.2, // 20% jitter
        }
    }

    /// Create a configuration with unlimited retries
    pub fn unlimited() -> Self {
        Self {
            max_retries: None,
            ..Default::default()
        }
    }
}

/// Exponential backoff state machine
pub struct ExponentialBackoff {
    config: BackoffConfig,
    current_attempt: usize,
    current_delay: Duration,
}

impl ExponentialBackoff {
    /// Create a new backoff with default configuration
    pub fn new() -> Self {
        Self::with_config(BackoffConfig::default())
    }

    /// Create a new backoff with custom configuration
    pub fn with_config(config: BackoffConfig) -> Self {
        Self {
            current_delay: config.initial_delay,
            current_attempt: 0,
            config,
        }
    }

    /// Get the next delay duration and advance the backoff state
    ///
    /// Returns None if max retries exceeded
    pub fn next_delay(&mut self) -> Option<Duration> {
        // Check if we've exceeded max retries
        if let Some(max_retries) = self.config.max_retries {
            if self.current_attempt >= max_retries {
                return None;
            }
        }

        // Calculate delay with jitter
        let delay = self.calculate_delay_with_jitter();

        // Update state for next iteration
        self.current_attempt += 1;
        self.current_delay = std::cmp::min(
            Duration::from_secs_f64(self.current_delay.as_secs_f64() * self.config.multiplier),
            self.config.max_delay,
        );

        Some(delay)
    }

    /// Calculate delay with jitter to prevent thundering herd
    fn calculate_delay_with_jitter(&self) -> Duration {
        if self.config.jitter_factor == 0.0 {
            return self.current_delay;
        }

        let mut rng = rand::thread_rng();
        let jitter = rng.gen::<f64>() * self.config.jitter_factor;
        let jitter_multiplier = 1.0 + (jitter - self.config.jitter_factor / 2.0);

        Duration::from_secs_f64(self.current_delay.as_secs_f64() * jitter_multiplier)
    }

    /// Reset the backoff to initial state
    pub fn reset(&mut self) {
        self.current_attempt = 0;
        self.current_delay = self.config.initial_delay;
    }

    /// Get current attempt number
    pub fn attempt_number(&self) -> usize {
        self.current_attempt
    }

    /// Check if more retries are available
    pub fn can_retry(&self) -> bool {
        match self.config.max_retries {
            Some(max) => self.current_attempt < max,
            None => true,
        }
    }

    /// Get the configuration
    pub fn config(&self) -> &BackoffConfig {
        &self.config
    }
}

impl Default for ExponentialBackoff {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backoff_basic() {
        let mut backoff = ExponentialBackoff::new();

        assert_eq!(backoff.attempt_number(), 0);
        assert!(backoff.can_retry());

        // First delay
        let delay1 = backoff.next_delay().unwrap();
        assert_eq!(backoff.attempt_number(), 1);

        // Second delay should be larger
        let delay2 = backoff.next_delay().unwrap();
        assert_eq!(backoff.attempt_number(), 2);
        assert!(delay2 > delay1);
    }

    #[test]
    fn test_backoff_max_retries() {
        let config = BackoffConfig {
            max_retries: Some(3),
            ..Default::default()
        };
        let mut backoff = ExponentialBackoff::with_config(config);

        // Should succeed for first 3 attempts
        assert!(backoff.next_delay().is_some());
        assert!(backoff.next_delay().is_some());
        assert!(backoff.next_delay().is_some());

        // Should fail on 4th attempt
        assert!(backoff.next_delay().is_none());
        assert!(!backoff.can_retry());
    }

    #[test]
    fn test_backoff_max_delay() {
        let config = BackoffConfig {
            initial_delay: Duration::from_millis(10),
            max_delay: Duration::from_millis(100),
            multiplier: 2.0,
            max_retries: Some(20),
            jitter_factor: 0.0, // No jitter for predictable test
        };
        let mut backoff = ExponentialBackoff::with_config(config);

        // Keep increasing until max_delay
        let mut last_delay = Duration::from_secs(0);
        for _ in 0..10 {
            if let Some(delay) = backoff.next_delay() {
                // Delay should not exceed max_delay
                assert!(delay <= Duration::from_millis(100));
                last_delay = delay;
            }
        }

        // Eventually should reach max_delay
        assert!(last_delay >= Duration::from_millis(80));
    }

    #[test]
    fn test_backoff_reset() {
        let mut backoff = ExponentialBackoff::new();

        backoff.next_delay();
        backoff.next_delay();
        assert_eq!(backoff.attempt_number(), 2);

        backoff.reset();
        assert_eq!(backoff.attempt_number(), 0);
        assert!(backoff.can_retry());
    }

    #[test]
    fn test_backoff_jitter() {
        let config = BackoffConfig {
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(10),
            multiplier: 2.0,
            max_retries: Some(5),
            jitter_factor: 0.2, // 20% jitter
        };
        let mut backoff = ExponentialBackoff::with_config(config);

        // Get multiple delays and verify they have variation (jitter)
        let delay1 = backoff.next_delay().unwrap();
        backoff.reset();
        let delay2 = backoff.next_delay().unwrap();
        backoff.reset();
        let delay3 = backoff.next_delay().unwrap();

        // With jitter, these should not all be exactly equal
        // (small chance of false positive, but very unlikely)
        let all_equal = delay1 == delay2 && delay2 == delay3;
        assert!(!all_equal, "Jitter should produce varying delays");
    }

    #[test]
    fn test_backoff_unlimited() {
        let config = BackoffConfig::unlimited();
        let mut backoff = ExponentialBackoff::with_config(config);

        // Should be able to retry many times
        for _ in 0..100 {
            assert!(backoff.next_delay().is_some());
            assert!(backoff.can_retry());
        }
    }

    #[test]
    fn test_backoff_aggressive() {
        let backoff = ExponentialBackoff::with_config(BackoffConfig::aggressive());

        assert_eq!(backoff.config().initial_delay, Duration::from_millis(10));
        assert_eq!(backoff.config().max_delay, Duration::from_secs(1));
        assert_eq!(backoff.config().max_retries, Some(5));
    }

    #[test]
    fn test_backoff_conservative() {
        let backoff = ExponentialBackoff::with_config(BackoffConfig::conservative());

        assert_eq!(backoff.config().initial_delay, Duration::from_secs(1));
        assert_eq!(backoff.config().max_delay, Duration::from_secs(60));
        assert_eq!(backoff.config().max_retries, Some(20));
    }

    #[test]
    fn test_backoff_exponential_growth() {
        let config = BackoffConfig {
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(100),
            multiplier: 2.0,
            max_retries: Some(5),
            jitter_factor: 0.0, // No jitter for predictable test
        };
        let mut backoff = ExponentialBackoff::with_config(config);

        let delay1 = backoff.next_delay().unwrap();
        let delay2 = backoff.next_delay().unwrap();
        let delay3 = backoff.next_delay().unwrap();

        // Verify exponential growth
        assert!(delay2.as_millis() >= delay1.as_millis() * 2);
        assert!(delay3.as_millis() >= delay2.as_millis() * 2);
    }
}
