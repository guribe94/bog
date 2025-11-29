//! Test Helper Utilities
//!
//! Provides convenient builders and utilities for testing the trading engine.
//!
//! ## Categories
//!
//! ### Market Data Creation
//!
//! - [`create_test_snapshot`] - Custom snapshot with specified prices/sizes
//! - [`create_simple_snapshot`] - Default BTC-USD snapshot ($50,000 bid, $50,005 ask)
//!
//! Use SnapshotBuilder internally to ensure proper struct initialization.
//!
//! ### Position Builders
//!
//! - [`create_test_position`] - Position with specified quantity
//! - [`create_test_position_with_pnl`] - Position with quantity and PnL values
//!
//! Returns Arc<Position> for use with Engine.
//!
//! ### Signal Builders
//!
//! - [`create_quote_both_signal`] - Two-sided market making signal
//! - [`create_quote_signal`] - Single-side quote (bid or ask)
//! - [`create_cancel_signal`] - Cancel all orders
//! - [`create_no_action_signal`] - No-op signal
//!
//! ### Performance Measurement
//!
//! - [`assert_within_latency`] - Assert operation completes within time limit
//! - [`measure_latency`] - Measure single operation latency
//! - [`measure_average_latency`] - Measure average over multiple iterations
//!
//! Use these to verify sub-microsecond performance targets.
//!
//! ### Fixed-Point Conversion
//!
//! [`fixed_point`] module provides:
//! - [`fixed_point::from_f64`] - Convert f64 to u64 (9 decimals)
//! - [`fixed_point::to_f64`] - Convert u64 to f64
//! - [`fixed_point::to_f64_signed`] - Convert i64 to f64
//!
//! ### Test Constants
//!
//! [`constants`] module provides common test values:
//! - `BTC_BID` - $50,000 in fixed-point
//! - `BTC_ASK` - $50,025 in fixed-point (5 bp spread)
//! - `BTC_SIZE` - 1.0 BTC
//! - `BTC_SMALL_SIZE` - 0.1 BTC
//! - `ONE_BP`, `TEN_BP` - Basis point values
//!
//! ## Example Usage
//!
//! ```ignore
//! use bog_core::testing::helpers::*;
//! use std::time::Duration;
//!
//! // Create test data
//! let snapshot = create_simple_snapshot(1);
//! let position = create_test_position(1_000_000_000); // 1.0 BTC long
//!
//! // Measure performance
//! assert_within_latency(Duration::from_nanos(1000), || {
//!     // Operation that should complete in <1μs
//!     let signal = create_quote_both_signal(
//!         constants::BTC_BID,
//!         constants::BTC_ASK,
//!         constants::BTC_SIZE,
//!     );
//! }, "signal creation");
//!
//! // Fixed-point conversion
//! let price_f64 = 50000.50;
//! let price_fp = fixed_point::from_f64(price_f64);
//! assert_eq!(price_fp, 50_000_500_000_000);
//! ```

use crate::core::{OrderId, Position, Side, Signal, SignalAction};
use crate::data::SnapshotBuilder;
use crate::monitoring::MetricsRegistry;
use huginn::MarketSnapshot;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Create a test market snapshot with specified prices
///
/// Uses SnapshotBuilder to ensure proper array sizing (no hardcoded values).
pub fn create_test_snapshot(
    market_id: u64,
    sequence: u64,
    bid_price: u64,
    ask_price: u64,
    bid_size: u64,
    ask_size: u64,
) -> MarketSnapshot {
    SnapshotBuilder::new()
        .market_id(market_id)
        .sequence(sequence)
        .timestamp(0)
        .best_bid(bid_price, bid_size)
        .best_ask(ask_price, ask_size)
        .incremental_snapshot()
        .build()
}

/// Create a simple test snapshot with default BTC-USD prices
pub fn create_simple_snapshot(sequence: u64) -> MarketSnapshot {
    create_test_snapshot(
        1,               // market_id
        sequence,        // sequence
        50000_000000000, // bid: $50,000
        50005_000000000, // ask: $50,005
        1_000000000,     // bid size: 1.0 BTC
        1_000000000,     // ask size: 1.0 BTC
    )
}

/// Create a test position with specified quantity
pub fn create_test_position(quantity: i64) -> Arc<Position> {
    let position = Arc::new(Position::new());
    position
        .quantity
        .store(quantity, std::sync::atomic::Ordering::Relaxed);
    position
}

/// Create a test position with quantity and PnL
pub fn create_test_position_with_pnl(
    quantity: i64,
    realized_pnl: i64,
    daily_pnl: i64,
) -> Arc<Position> {
    let position = Arc::new(Position::new());
    position
        .quantity
        .store(quantity, std::sync::atomic::Ordering::Relaxed);
    position
        .realized_pnl
        .store(realized_pnl, std::sync::atomic::Ordering::Relaxed);
    position
        .daily_pnl
        .store(daily_pnl, std::sync::atomic::Ordering::Relaxed);
    position
}

/// Create a test signal for quote-both action
pub fn create_quote_both_signal(bid_price: u64, ask_price: u64, size: u64) -> Signal {
    Signal::quote_both(bid_price, ask_price, size)
}

/// Create a test signal for single-side quote
pub fn create_quote_signal(side: Side, price: u64, size: u64) -> Signal {
    match side {
        Side::Buy => Signal::quote_bid(price, size),
        Side::Sell => Signal::quote_ask(price, size),
    }
}

/// Create a cancel signal
pub fn create_cancel_signal(_order_id: OrderId) -> Signal {
    Signal::cancel_all()
}

/// Create a no-action signal
pub fn create_no_action_signal() -> Signal {
    Signal::no_action()
}

/// Assert that an operation completes within expected latency
pub fn assert_within_latency<F>(max_latency: Duration, operation: F, operation_name: &str)
where
    F: FnOnce(),
{
    let start = Instant::now();
    operation();
    let elapsed = start.elapsed();

    assert!(
        elapsed <= max_latency,
        "{} took {:?}, expected <= {:?}",
        operation_name,
        elapsed,
        max_latency
    );
}

/// Measure operation latency
pub fn measure_latency<F, R>(operation: F) -> (R, Duration)
where
    F: FnOnce() -> R,
{
    let start = Instant::now();
    let result = operation();
    let elapsed = start.elapsed();
    (result, elapsed)
}

/// Measure average latency over multiple runs
pub fn measure_average_latency<F>(iterations: usize, mut operation: F) -> Duration
where
    F: FnMut(),
{
    let start = Instant::now();
    for _ in 0..iterations {
        operation();
    }
    let total = start.elapsed();
    total / iterations as u32
}

/// Collect metrics snapshot for assertions
pub struct MetricsSnapshot {
    pub orders_submitted: u64,
    pub fills_received: u64,
    pub volume_usd: f64,
    pub position_btc: f64,
    pub ticks_per_second: f64,
}

impl MetricsSnapshot {
    /// Collect current metrics from registry
    pub fn collect(_registry: &MetricsRegistry) -> Self {
        // Note: Prometheus metrics don't have direct getters for current values
        // In production, we'd query the registry, but for tests we can use this
        // as a placeholder structure
        Self {
            orders_submitted: 0,
            fills_received: 0,
            volume_usd: 0.0,
            position_btc: 0.0,
            ticks_per_second: 0.0,
        }
    }
}

/// Create a test metrics registry
pub fn create_test_metrics() -> Arc<MetricsRegistry> {
    Arc::new(MetricsRegistry::new().expect("Failed to create test metrics"))
}

/// Fixed-point conversion helpers (9 decimal places)
pub mod fixed_point {
    /// Convert f64 to u64 fixed-point (9 decimals)
    pub fn from_f64(value: f64) -> u64 {
        (value * 1_000_000_000.0) as u64
    }

    /// Convert u64 fixed-point to f64
    pub fn to_f64(value: u64) -> f64 {
        value as f64 / 1_000_000_000.0
    }

    /// Convert i64 fixed-point to f64
    pub fn to_f64_signed(value: i64) -> f64 {
        value as f64 / 1_000_000_000.0
    }
}

/// Common test constants
pub mod constants {
    /// Default BTC-USD bid price: $50,000
    pub const BTC_BID: u64 = 50000_000000000;

    /// Default BTC-USD ask price: $50,025 (5bp spread)
    /// 5 bp on $50,000 = $50,000 * 0.0005 = $25
    pub const BTC_ASK: u64 = 50025_000000000;

    /// Default size: 1.0 BTC
    pub const BTC_SIZE: u64 = 1_000000000;

    /// Default size: 0.1 BTC
    pub const BTC_SMALL_SIZE: u64 = 100_000000;

    /// 1 basis point in fixed-point
    pub const ONE_BP: u64 = 10_000000;

    /// 10 basis points in fixed-point
    pub const TEN_BP: u64 = 100_000000;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_test_snapshot() {
        let snapshot = create_simple_snapshot(1);
        assert_eq!(snapshot.market_id, 1);
        assert_eq!(snapshot.sequence, 1);
        assert_eq!(snapshot.best_bid_price, 50000_000000000);
        assert_eq!(snapshot.best_ask_price, 50005_000000000);
    }

    #[test]
    fn test_create_test_position() {
        let position = create_test_position(1_000000000);
        assert_eq!(position.get_quantity(), 1_000000000);
    }

    #[test]
    fn test_create_test_position_with_pnl() {
        let position = create_test_position_with_pnl(1_000000000, 100_000000000, 50_000000000);
        assert_eq!(position.get_quantity(), 1_000000000);
        assert_eq!(position.get_realized_pnl(), 100_000000000);
        assert_eq!(position.get_daily_pnl(), 50_000000000);
    }

    #[test]
    fn test_create_quote_both_signal() {
        let signal = create_quote_both_signal(50000_000000000, 50005_000000000, 100_000000);
        assert_eq!(signal.action, SignalAction::QuoteBoth);
        assert_eq!(signal.bid_price, 50000_000000000);
        assert_eq!(signal.ask_price, 50005_000000000);
        assert_eq!(signal.size, 100_000000);
    }

    #[test]
    fn test_assert_within_latency() {
        assert_within_latency(
            Duration::from_millis(10),
            || {
                // Fast operation
                let _x = 1 + 1;
            },
            "fast operation",
        );
    }

    #[test]
    fn test_measure_latency() {
        let (result, latency) = measure_latency(|| {
            std::thread::sleep(Duration::from_millis(1));
            42
        });

        assert_eq!(result, 42);
        assert!(latency >= Duration::from_millis(1));
    }

    #[test]
    fn test_measure_average_latency() {
        let avg = measure_average_latency(10, || {
            // Simulate work
            let _x = (0..100).sum::<i32>();
        });

        // Should be very fast
        assert!(avg < Duration::from_millis(1));
    }

    #[test]
    fn test_fixed_point_conversion() {
        let value = 50000.5;
        let fp = fixed_point::from_f64(value);
        let back = fixed_point::to_f64(fp);

        assert!((back - value).abs() < 0.0001);
    }

    #[test]
    fn test_constants() {
        use constants::*;

        // Verify spread
        let spread = BTC_ASK - BTC_BID;
        assert_eq!(spread, 25_000000000); // $25

        // Verify 5bp spread
        let spread_bps = (spread * 10000) / BTC_BID;
        assert_eq!(spread_bps, 5); // 5 basis points
    }
}
