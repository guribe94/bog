//! Rate Limiting - Token Bucket Algorithm
//!
//! Prevents the bot from overwhelming the exchange with too many orders.
//! Uses the token bucket algorithm for smooth rate limiting with burst allowance.
//!
//! ## Algorithm
//!
//! ```text
//! Bucket Capacity: 100 tokens
//! Refill Rate: 10 tokens/second
//!
//! Time=0s:  [██████████] 100 tokens → place order (99 left)
//! Time=1s:  [█████████░] 109 tokens → refilled 10, place order (108 left)
//! Time=2s:  [█████████░] 118 tokens → etc.
//!
//! Burst: Can place 100 orders immediately, then throttled to 10/sec
//! ```
//!
//! ## Usage
//!
//! ```
//! use bog_core::risk::RateLimiter;
//!
//! let limiter = RateLimiter::new(100, 10.0); // 100 orders/sec max, 10 burst
//!
//! if limiter.allow() {
//!     // Place order
//! } else {
//!     // Rate limited - wait or skip
//! }
//! ```

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use parking_lot::Mutex;
use tracing::{debug, warn};

/// Rate limiter configuration
#[derive(Debug, Clone, Copy)]
pub struct RateLimiterConfig {
    /// Maximum orders per second (sustained rate)
    pub max_orders_per_second: u64,
    /// Burst capacity (tokens in bucket)
    pub burst_capacity: u64,
    /// Tokens per refill (fractional)
    pub refill_rate: f64,
    /// Refill interval
    pub refill_interval: Duration,
}

impl RateLimiterConfig {
    /// Conservative limits for production
    pub fn conservative() -> Self {
        Self {
            max_orders_per_second: 10,   // 10 orders/sec max
            burst_capacity: 20,           // Can burst to 20
            refill_rate: 10.0,
            refill_interval: Duration::from_secs(1),
        }
    }

    /// Standard limits
    pub fn standard() -> Self {
        Self {
            max_orders_per_second: 100,
            burst_capacity: 100,
            refill_rate: 100.0,
            refill_interval: Duration::from_secs(1),
        }
    }

    /// Aggressive limits (for HFT)
    pub fn aggressive() -> Self {
        Self {
            max_orders_per_second: 1000,
            burst_capacity: 100,
            refill_rate: 1000.0,
            refill_interval: Duration::from_secs(1),
        }
    }
}

impl Default for RateLimiterConfig {
    fn default() -> Self {
        Self::standard()
    }
}

/// Token bucket rate limiter
///
/// Thread-safe with atomic operations for lock-free access in most cases.
#[derive(Clone)]
pub struct RateLimiter {
    config: RateLimiterConfig,
    /// Current tokens available (fixed-point: tokens * 1000)
    tokens: Arc<AtomicU64>,
    /// Last refill time
    last_refill: Arc<Mutex<Instant>>,
    /// Total requests attempted
    total_requests: Arc<AtomicU64>,
    /// Total requests allowed
    total_allowed: Arc<AtomicU64>,
    /// Total requests rejected
    total_rejected: Arc<AtomicU64>,
}

impl RateLimiter {
    /// Create a new rate limiter
    pub fn new(config: RateLimiterConfig) -> Self {
        let initial_tokens = (config.burst_capacity * 1000) as u64;

        Self {
            config,
            tokens: Arc::new(AtomicU64::new(initial_tokens)),
            last_refill: Arc::new(Mutex::new(Instant::now())),
            total_requests: Arc::new(AtomicU64::new(0)),
            total_allowed: Arc::new(AtomicU64::new(0)),
            total_rejected: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Create with default configuration
    pub fn new_default() -> Self {
        Self::new(RateLimiterConfig::default())
    }

    /// Create with conservative limits
    pub fn new_conservative() -> Self {
        Self::new(RateLimiterConfig::conservative())
    }

    /// Check if request is allowed (consumes 1 token if allowed)
    ///
    /// Returns true if request is allowed, false if rate limited.
    pub fn allow(&self) -> bool {
        self.allow_n(1)
    }

    /// Check if N requests are allowed (consumes N tokens if allowed)
    ///
    /// Returns true if all N requests are allowed, false if rate limited.
    pub fn allow_n(&self, n: u64) -> bool {
        self.total_requests.fetch_add(1, Ordering::Relaxed);

        // Refill tokens based on elapsed time
        self.refill();

        // Try to consume N tokens (fixed-point: n * 1000)
        let needed_tokens = n * 1000;
        let mut current_tokens = self.tokens.load(Ordering::Acquire);

        loop {
            if current_tokens < needed_tokens {
                // Not enough tokens
                self.total_rejected.fetch_add(1, Ordering::Relaxed);

                if self.total_rejected.load(Ordering::Relaxed) % 100 == 1 {
                    warn!(
                        "Rate limit exceeded: {}/{} requests allowed",
                        self.total_allowed.load(Ordering::Relaxed),
                        self.total_requests.load(Ordering::Relaxed)
                    );
                }

                return false;
            }

            // Try to consume tokens atomically
            match self.tokens.compare_exchange_weak(
                current_tokens,
                current_tokens - needed_tokens,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    // Success - tokens consumed
                    self.total_allowed.fetch_add(1, Ordering::Relaxed);
                    return true;
                }
                Err(actual) => {
                    // CAS failed, retry with updated value
                    current_tokens = actual;
                }
            }
        }
    }

    /// Refill tokens based on elapsed time
    fn refill(&self) {
        let mut last_refill = self.last_refill.lock();
        let now = Instant::now();
        let elapsed = now.duration_since(*last_refill);

        if elapsed < self.config.refill_interval {
            return; // Not time to refill yet
        }

        // Calculate tokens to add
        let intervals = elapsed.as_secs_f64() / self.config.refill_interval.as_secs_f64();
        let tokens_to_add = (self.config.refill_rate * intervals * 1000.0) as u64;

        if tokens_to_add > 0 {
            // Add tokens (cap at burst capacity)
            let max_tokens = (self.config.burst_capacity * 1000) as u64;
            let current = self.tokens.load(Ordering::Acquire);
            let new_tokens = (current + tokens_to_add).min(max_tokens);

            self.tokens.store(new_tokens, Ordering::Release);
            *last_refill = now;

            debug!(
                "Rate limiter refilled: +{} tokens (now: {}/{})",
                tokens_to_add / 1000,
                new_tokens / 1000,
                max_tokens / 1000
            );
        }
    }

    /// Get current token count (for monitoring)
    pub fn available_tokens(&self) -> u64 {
        self.tokens.load(Ordering::Acquire) / 1000
    }

    /// Get total requests attempted
    pub fn total_requests(&self) -> u64 {
        self.total_requests.load(Ordering::Acquire)
    }

    /// Get total requests allowed
    pub fn total_allowed(&self) -> u64 {
        self.total_allowed.load(Ordering::Acquire)
    }

    /// Get total requests rejected
    pub fn total_rejected(&self) -> u64 {
        self.total_rejected.load(Ordering::Acquire)
    }

    /// Get acceptance rate (0.0 to 1.0)
    pub fn acceptance_rate(&self) -> f64 {
        let total = self.total_requests.load(Ordering::Acquire);
        if total == 0 {
            return 1.0;
        }

        let allowed = self.total_allowed.load(Ordering::Acquire);
        allowed as f64 / total as f64
    }

    /// Reset statistics (not token count)
    pub fn reset_stats(&self) {
        self.total_requests.store(0, Ordering::Release);
        self.total_allowed.store(0, Ordering::Release);
        self.total_rejected.store(0, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_rate_limiter_creation() {
        let limiter = RateLimiter::new_default();
        assert!(limiter.available_tokens() > 0);
    }

    #[test]
    fn test_rate_limiter_allows_within_limit() {
        let config = RateLimiterConfig {
            max_orders_per_second: 100,
            burst_capacity: 10,
            refill_rate: 100.0,
            refill_interval: Duration::from_secs(1),
        };
        let limiter = RateLimiter::new(config);

        // Should allow first 10 requests (burst capacity)
        for i in 0..10 {
            assert!(limiter.allow(), "Request {} should be allowed", i);
        }

        assert_eq!(limiter.total_allowed(), 10);
    }

    #[test]
    fn test_rate_limiter_rejects_over_burst() {
        let config = RateLimiterConfig {
            max_orders_per_second: 100,
            burst_capacity: 5,
            refill_rate: 100.0,
            refill_interval: Duration::from_secs(1),
        };
        let limiter = RateLimiter::new(config);

        // Consume all tokens
        for _ in 0..5 {
            assert!(limiter.allow());
        }

        // Next request should be rejected (no tokens left, no refill yet)
        assert!(!limiter.allow());
        assert_eq!(limiter.total_rejected(), 1);
    }

    #[test]
    fn test_rate_limiter_refills() {
        let config = RateLimiterConfig {
            max_orders_per_second: 100,
            burst_capacity: 5,
            refill_rate: 10.0, // 10 tokens per interval
            refill_interval: Duration::from_millis(100),
        };
        let limiter = RateLimiter::new(config);

        // Consume all tokens
        for _ in 0..5 {
            limiter.allow();
        }

        // No tokens left
        assert!(!limiter.allow());

        // Wait for refill
        thread::sleep(Duration::from_millis(150));

        // Should have tokens again
        assert!(limiter.allow(), "Should have tokens after refill");
    }

    #[test]
    fn test_allow_n() {
        let config = RateLimiterConfig {
            burst_capacity: 10,
            ..Default::default()
        };
        let limiter = RateLimiter::new(config);

        // Try to consume 5 tokens
        assert!(limiter.allow_n(5));
        assert_eq!(limiter.available_tokens(), 5);

        // Try to consume 10 tokens (should fail, only 5 left)
        assert!(!limiter.allow_n(10));
    }

    #[test]
    fn test_acceptance_rate() {
        let config = RateLimiterConfig {
            burst_capacity: 5,
            ..Default::default()
        };
        let limiter = RateLimiter::new(config);

        // 5 allowed, 5 rejected
        for _ in 0..5 {
            limiter.allow();
        }
        for _ in 0..5 {
            limiter.allow();
        }

        // Acceptance rate should be 50%
        let rate = limiter.acceptance_rate();
        assert!((rate - 0.5).abs() < 0.01, "Rate should be ~0.5, got {}", rate);
    }

    #[test]
    fn test_concurrent_access() {
        let limiter = RateLimiter::new_default();
        let limiter_clone1 = limiter.clone();
        let limiter_clone2 = limiter.clone();

        let handle1 = thread::spawn(move || {
            for _ in 0..50 {
                limiter_clone1.allow();
            }
        });

        let handle2 = thread::spawn(move || {
            for _ in 0..50 {
                limiter_clone2.allow();
            }
        });

        handle1.join().unwrap();
        handle2.join().unwrap();

        // Should have processed 100 total requests
        assert_eq!(limiter.total_requests(), 100);
    }

    #[test]
    fn test_conservative_config() {
        let limiter = RateLimiter::new_conservative();
        assert_eq!(limiter.available_tokens(), 20); // Burst capacity
    }
}
