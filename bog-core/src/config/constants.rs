//! Centralized configuration for HFT trading system
//!
//! All configuration values are compile-time constants for zero overhead.
//! These can be controlled via Cargo features or environment variables at build time.

// ===== ENGINE RISK LIMITS =====

/// Maximum long position limit (in fixed-point with 9 decimals)
/// Default: 10 units (for paper trading with market-crossing fills)
#[cfg(not(feature = "max-position-2btc"))]
pub const MAX_POSITION: i64 = 10_000_000_000;
#[cfg(feature = "max-position-2btc")]
pub const MAX_POSITION: i64 = 20_000_000_000;

/// Maximum short position limit (in fixed-point with 9 decimals)
/// Default: 10 units (for paper trading with market-crossing fills)
#[cfg(not(feature = "max-short-2btc"))]
pub const MAX_SHORT: i64 = 10_000_000_000;
#[cfg(feature = "max-short-2btc")]
pub const MAX_SHORT: i64 = 20_000_000_000;

/// Maximum drawdown allowed before halting (fraction, 9-decimal fixed-point)
/// Default: 5% drawdown from peak realized PnL
#[cfg(not(feature = "max-drawdown-10pct"))]
pub const MAX_DRAWDOWN: i64 = 50_000_000;
#[cfg(feature = "max-drawdown-10pct")]
pub const MAX_DRAWDOWN: i64 = 100_000_000;

/// Minimum peak PnL required before drawdown check applies (quote currency, 9 decimals)
/// Below this threshold, drawdown protection is skipped to avoid false triggers
/// on small simulated PnL values in paper trading.
/// Default: $10
pub const MIN_DRAWDOWN_PEAK_THRESHOLD: i64 = 10_000_000_000;

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

/// Maximum spread in basis points before circuit breaker trips
/// Default: 1000bps (10%) - suitable for volatile low-liquidity altcoins
#[cfg(not(feature = "breaker-spread-100bps"))]
pub const CIRCUIT_BREAKER_MAX_SPREAD_BPS: u64 = 1000;
#[cfg(feature = "breaker-spread-100bps")]
pub const CIRCUIT_BREAKER_MAX_SPREAD_BPS: u64 = 100;

/// Maximum price change between ticks (percentage) before circuit breaker trips
/// Default: 10% - anything larger is likely erroneous data
pub const CIRCUIT_BREAKER_MAX_PRICE_CHANGE_PCT: u64 = 10;

/// Minimum bid/ask size in fixed-point (9 decimals) for circuit breaker
/// Default: 0 - disabled for altcoin trading where one side is often empty
pub const CIRCUIT_BREAKER_MIN_LIQUIDITY: u64 = 0;

/// Maximum data age in nanoseconds before considered stale
/// Default: 5 seconds
pub const CIRCUIT_BREAKER_MAX_DATA_AGE_NS: i64 = 5_000_000_000;

/// Consecutive violations before circuit breaker trips
/// Prevents single spurious tick from halting trading
pub const CIRCUIT_BREAKER_VIOLATIONS_THRESHOLD: u32 = 3;

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

/// Orderbook imbalance threshold for spread adjustment (fixed-point fraction)
/// If imbalance > +20% (bullish) or < -20% (bearish), adjust spreads
/// Default: 0.2 (20% imbalance triggers adjustment)
pub const IMBALANCE_THRESHOLD: i64 = 200_000_000;

/// Spread adjustment amount in basis points
/// When imbalance detected, adjust bid/ask by this amount
/// Default: 2bps adjustment
pub const SPREAD_ADJUSTMENT_BPS: u32 = 2;

// ===== FEE CONFIGURATION =====

/// Default exchange fee rate in sub-basis points (1/100th of a basis point)
/// This allows fractional bps precision: 1 sub-bps = 0.0001% = 0.01 bps
///
/// Lighter DEX fees:
/// - Maker: 0.2 bps (0.002%) = 20 sub-bps
/// - Taker: 2 bps (0.02%) = 200 sub-bps
///
/// We use maker fee since we're placing limit orders as a market maker.
#[cfg(not(feature = "fee-5bps"))]
pub const DEFAULT_FEE_SUB_BPS: u32 = 20; // Maker fee: 0.2 bps = 20 sub-bps
#[cfg(feature = "fee-5bps")]
pub const DEFAULT_FEE_SUB_BPS: u32 = 500; // 5 bps = 500 sub-bps

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

// ===== BASIS POINT CALCULATIONS =====

/// Scale factor for basis point calculations (1 bp = 0.01% = 0.0001)
/// 10_000 bps = 100%
pub const BPS_SCALE: u64 = 10_000;

// ===== ORDER SIZE LIMITS =====

/// Minimum order size (in fixed-point with 9 decimals)
/// Default: 0.01 units
pub const MIN_ORDER_SIZE: u64 = 10_000_000;

/// Maximum order size (in fixed-point with 9 decimals)
/// Default: 0.5 units
pub const MAX_ORDER_SIZE: u64 = 500_000_000;

// ===== PRICE VALIDATION =====

/// Maximum price distance from mid allowed for quotes (basis points)
/// Default: 50 bps (0.5%)
pub const MAX_PRICE_DISTANCE_BPS: u32 = 50;

// ===== EXCHANGE LATENCY SIMULATION =====
//
// Lighter DEX applies latency to orders based on account type.
// We simulate these latencies in paper trading to accurately model execution.
//
// Standard Account (default, more conservative):
// - Maker orders: 200ms delay before they appear in orderbook
// - Taker orders: 300ms delay before fill
// - Cancel orders: 100ms delay before order is removed
//
// Premium Account:
// - Maker/Cancel: 0ms (instant)
// - Taker: 150ms delay

/// Maker order latency in nanoseconds (time before order appears in book)
/// Default: 200ms (Standard account - more conservative)
#[cfg(not(feature = "latency-premium"))]
pub const MAKER_LATENCY_NS: u64 = 200_000_000; // 200ms
#[cfg(feature = "latency-premium")]
pub const MAKER_LATENCY_NS: u64 = 0; // 0ms for premium

/// Taker order latency in nanoseconds (time before fill)
/// Default: 300ms (Standard account - more conservative)
#[cfg(not(feature = "latency-premium"))]
pub const TAKER_LATENCY_NS: u64 = 300_000_000; // 300ms
#[cfg(feature = "latency-premium")]
pub const TAKER_LATENCY_NS: u64 = 150_000_000; // 150ms for premium

/// Cancel order latency in nanoseconds (time before order is removed)
/// Default: 100ms (Standard account)
#[cfg(not(feature = "latency-premium"))]
pub const CANCEL_LATENCY_NS: u64 = 100_000_000; // 100ms
#[cfg(feature = "latency-premium")]
pub const CANCEL_LATENCY_NS: u64 = 0; // 0ms for premium

// ===== TIME CONSTANTS =====

/// Seconds per day for daily reset calculations
pub const SECONDS_PER_DAY: u64 = 86400;

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
