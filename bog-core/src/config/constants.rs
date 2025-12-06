//! Centralized configuration for HFT trading system
//!
//! All configuration values are compile-time constants for zero overhead.
//! These can be controlled via Cargo features or environment variables at build time.

// ===== ENGINE RISK LIMITS =====

/// Maximum long position limit (in fixed-point with 9 decimals)
/// Default: 1 BTC
#[cfg(not(feature = "max-position-2btc"))]
pub const MAX_POSITION: i64 = 1_000_000_000;
#[cfg(feature = "max-position-2btc")]
pub const MAX_POSITION: i64 = 2_000_000_000;

/// Maximum short position limit (in fixed-point with 9 decimals)
/// Default: 1 BTC
#[cfg(not(feature = "max-short-2btc"))]
pub const MAX_SHORT: i64 = 1_000_000_000;
#[cfg(feature = "max-short-2btc")]
pub const MAX_SHORT: i64 = 2_000_000_000;

/// Maximum drawdown allowed before halting (fraction, 9-decimal fixed-point)
/// Default: 5% drawdown from peak realized PnL
#[cfg(not(feature = "max-drawdown-10pct"))]
pub const MAX_DRAWDOWN: i64 = 50_000_000;
#[cfg(feature = "max-drawdown-10pct")]
pub const MAX_DRAWDOWN: i64 = 100_000_000;

/// Maximum daily loss allowed (quote currency, 9 decimals)
/// Default: $5,000 loss
#[cfg(not(feature = "max-daily-loss-20pct"))]
pub const MAX_DAILY_LOSS: i64 = 5_000 * 1_000_000_000;
#[cfg(feature = "max-daily-loss-20pct")]
pub const MAX_DAILY_LOSS: i64 = 10_000 * 1_000_000_000;

// ===== RATE LIMITING =====

/// Minimum time between quotes in nanoseconds
/// Default: 100ms = 100_000_000 ns (10 quotes per second max)
#[cfg(not(any(feature = "rate-limit-50ms", feature = "rate-limit-200ms")))]
pub const MIN_QUOTE_INTERVAL_NS: u64 = 100_000_000;
#[cfg(feature = "rate-limit-50ms")]
pub const MIN_QUOTE_INTERVAL_NS: u64 = 50_000_000;
#[cfg(feature = "rate-limit-200ms")]
pub const MIN_QUOTE_INTERVAL_NS: u64 = 200_000_000;

// ===== QUEUE MANAGEMENT =====

/// Queue depth warning threshold
/// Default: 100 messages
#[cfg(not(feature = "queue-warning-200"))]
pub const QUEUE_DEPTH_WARNING_THRESHOLD: usize = 100;
#[cfg(feature = "queue-warning-200")]
pub const QUEUE_DEPTH_WARNING_THRESHOLD: usize = 200;

// ===== MARKET DATA VALIDATION =====

/// Maximum price change after stale data recovery (basis points)
/// Default: 200 bps (2%)
#[cfg(not(feature = "stale-recovery-5pct"))]
pub const MAX_POST_STALE_CHANGE_BPS: u64 = 200;
#[cfg(feature = "stale-recovery-5pct")]
pub const MAX_POST_STALE_CHANGE_BPS: u64 = 500;

// ===== CIRCUIT BREAKER THRESHOLDS =====

/// Sequence gap threshold for circuit breaker
/// Default: 1000 gaps
#[cfg(not(feature = "breaker-gap-5000"))]
pub const CIRCUIT_BREAKER_SEQUENCE_GAP: u64 = 1000;
#[cfg(feature = "breaker-gap-5000")]
pub const CIRCUIT_BREAKER_SEQUENCE_GAP: u64 = 5000;

/// Price spike threshold for circuit breaker (basis points)
/// Default: 500 bps (5%)
#[cfg(not(feature = "breaker-spike-10pct"))]
pub const CIRCUIT_BREAKER_PRICE_SPIKE_BPS: u64 = 500;
#[cfg(feature = "breaker-spike-10pct")]
pub const CIRCUIT_BREAKER_PRICE_SPIKE_BPS: u64 = 1000;

/// Zero market threshold for circuit breaker
/// Default: 3 consecutive zero markets
#[cfg(not(feature = "breaker-zero-5"))]
pub const CIRCUIT_BREAKER_ZERO_MARKETS: u32 = 3;
#[cfg(feature = "breaker-zero-5")]
pub const CIRCUIT_BREAKER_ZERO_MARKETS: u32 = 5;

// ===== STRATEGY PARAMETERS =====

/// Inventory impact on quotes (basis points)
/// How much to adjust quotes based on inventory
/// Default: 5 bps per 10% of position limit
#[cfg(not(feature = "inventory-impact-10bps"))]
pub const INVENTORY_IMPACT_BPS: u64 = 5;
#[cfg(feature = "inventory-impact-10bps")]
pub const INVENTORY_IMPACT_BPS: u64 = 10;

/// Volatility spike threshold (basis points)
/// When to widen spreads due to volatility
/// Default: 100 bps (1%)
#[cfg(not(feature = "vol-threshold-200bps"))]
pub const VOLATILITY_SPIKE_THRESHOLD_BPS: u64 = 100;
#[cfg(feature = "vol-threshold-200bps")]
pub const VOLATILITY_SPIKE_THRESHOLD_BPS: u64 = 200;

// ===== FEE CONFIGURATION =====

/// Default exchange fee rate (basis points)
/// Default: 2 bps (0.02%)
#[cfg(not(feature = "fee-5bps"))]
pub const DEFAULT_FEE_BPS: u32 = 2;
#[cfg(feature = "fee-5bps")]
pub const DEFAULT_FEE_BPS: u32 = 5;

/// Exchange tick size (price increment) in fixed-point (9 decimals)
/// Default: $0.00001 (smallest tick on Lighter DEX for meme coins)
/// For POPCAT at $0.098, each tick is 0.01 bps
pub const TICK_SIZE: u64 = 10_000;

// ===== PERFORMANCE TUNING =====

/// Maximum fill batch size to process per tick
/// Default: 100 fills
#[cfg(not(feature = "fill-batch-200"))]
pub const MAX_FILL_BATCH_SIZE: usize = 100;
#[cfg(feature = "fill-batch-200")]
pub const MAX_FILL_BATCH_SIZE: usize = 200;

/// Object pool sizes for allocation-free operation
/// Default: 1024 objects
#[cfg(not(feature = "pool-size-2048"))]
pub const OBJECT_POOL_SIZE: usize = 1024;
#[cfg(feature = "pool-size-2048")]
pub const OBJECT_POOL_SIZE: usize = 2048;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_values_reasonable() {
        // Ensure position limits are positive
        assert!(MAX_POSITION > 0);
        assert!(MAX_SHORT > 0);

        // Ensure drawdown limits are positive
        assert!(MAX_DRAWDOWN > 0);
        assert!(MAX_DAILY_LOSS > 0);
        #[cfg(not(feature = "max-daily-loss-20pct"))]
        assert_eq!(MAX_DAILY_LOSS, 5_000 * 1_000_000_000);
        #[cfg(feature = "max-daily-loss-20pct")]
        assert_eq!(MAX_DAILY_LOSS, 10_000 * 1_000_000_000);

        // Daily loss should be >= drawdown
        assert!(MAX_DAILY_LOSS >= MAX_DRAWDOWN);

        // Rate limiting should be reasonable (not too fast)
        assert!(MIN_QUOTE_INTERVAL_NS >= 10_000_000); // At least 10ms

        // Queue warning should be reasonable
        assert!(QUEUE_DEPTH_WARNING_THRESHOLD >= 10);

        // Circuit breaker thresholds should be positive
        assert!(CIRCUIT_BREAKER_SEQUENCE_GAP > 0);
        assert!(CIRCUIT_BREAKER_PRICE_SPIKE_BPS > 0);
        assert!(CIRCUIT_BREAKER_ZERO_MARKETS > 0);
    }

    #[test]
    fn test_config_consistency() {
        // Position limits should be symmetric by default
        // (Can be asymmetric with features)
        #[cfg(not(any(feature = "max-position-2btc", feature = "max-short-2btc")))]
        assert_eq!(MAX_POSITION, MAX_SHORT);
    }
}
