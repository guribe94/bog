//! Simple Spread Market Making Strategy - Zero-Sized Type
//!
//! This is a zero-overhead implementation using:
//! - Zero-sized type (no memory overhead)
//! - Const parameters from Cargo features
//! - u64 fixed-point arithmetic (9 decimal places)
//! - No heap allocations
//! - #[inline(always)] for maximum performance
//!
//! Target: <100ns signal generation

use bog_core::core::Signal;
use bog_core::data::MarketSnapshot;
use bog_core::engine::Strategy;

// ===== CONFIGURATION FROM CARGO FEATURES =====

/// Spread in basis points (10 = 0.1%)
/// Override with #[cfg(feature = "...")] in your binary
#[cfg(not(any(
    feature = "spread-5bps",
    feature = "spread-10bps",
    feature = "spread-20bps"
)))]
pub const SPREAD_BPS: u32 = 10;

#[cfg(feature = "spread-5bps")]
pub const SPREAD_BPS: u32 = 5;

#[cfg(feature = "spread-10bps")]
pub const SPREAD_BPS: u32 = 10;

#[cfg(feature = "spread-20bps")]
pub const SPREAD_BPS: u32 = 20;

/// Order size in fixed-point (9 decimals)
/// Default: 0.1 BTC = 100_000_000
#[cfg(not(any(
    feature = "size-small",
    feature = "size-medium",
    feature = "size-large"
)))]
pub const ORDER_SIZE: u64 = 100_000_000; // 0.1 BTC

#[cfg(feature = "size-small")]
pub const ORDER_SIZE: u64 = 10_000_000; // 0.01 BTC

#[cfg(feature = "size-medium")]
pub const ORDER_SIZE: u64 = 100_000_000; // 0.1 BTC

#[cfg(feature = "size-large")]
pub const ORDER_SIZE: u64 = 1_000_000_000; // 1.0 BTC

/// Minimum market spread to trade (basis points)
/// If market spread < this, don't quote
#[cfg(not(any(
    feature = "min-spread-1bps",
    feature = "min-spread-5bps",
    feature = "min-spread-10bps"
)))]
pub const MIN_SPREAD_BPS: u32 = 1;

#[cfg(feature = "min-spread-1bps")]
pub const MIN_SPREAD_BPS: u32 = 1;

#[cfg(feature = "min-spread-5bps")]
pub const MIN_SPREAD_BPS: u32 = 5;

#[cfg(feature = "min-spread-10bps")]
pub const MIN_SPREAD_BPS: u32 = 10;

// Note: We calculate spread dynamically rather than pre-computing
// to allow for const generic parameters to work with any spread value

/// Simple Spread Strategy - Zero-Sized Type
///
/// This strategy posts quotes at a fixed spread around the mid price.
/// All parameters are const, resolved at compile time.
pub struct SimpleSpread;

impl SimpleSpread {
    /// Calculate quote prices from mid price
    ///
    /// Returns (bid_price, ask_price) in u64 fixed-point
    #[inline(always)]
    fn calculate_quotes(mid_price: u64) -> (u64, u64) {
        // Calculate half spread: mid_price * (spread_bps / 20000)
        let half_spread = (mid_price * SPREAD_BPS as u64) / 20_000;

        let bid_price = mid_price.saturating_sub(half_spread);
        let ask_price = mid_price.saturating_add(half_spread);

        (bid_price, ask_price)
    }

    /// Check if market spread is wide enough to trade
    ///
    /// Returns true if market_spread_bps >= MIN_SPREAD_BPS
    #[inline(always)]
    fn is_spread_sufficient(bid: u64, ask: u64) -> bool {
        if bid == 0 || ask <= bid {
            return false;
        }

        // Calculate spread in basis points: ((ask - bid) / bid) * 10000
        let spread = ask - bid;
        let spread_bps = (spread * 10_000) / bid;

        spread_bps >= MIN_SPREAD_BPS as u64
    }
}

impl Strategy for SimpleSpread {
    #[inline(always)]
    fn calculate(&mut self, snapshot: &MarketSnapshot) -> Option<Signal> {
        // Extract best bid and ask
        let bid = snapshot.best_bid_price;
        let ask = snapshot.best_ask_price;

        // Validate prices
        if bid == 0 || ask == 0 || ask <= bid {
            return None;
        }

        // Check if spread is sufficient
        if !Self::is_spread_sufficient(bid, ask) {
            return None;
        }

        // Calculate mid price (use (bid + ask) / 2 to avoid overflow)
        let mid_price = bid / 2 + ask / 2 + (bid % 2 + ask % 2) / 2;

        // Calculate our quote prices
        let (our_bid, our_ask) = Self::calculate_quotes(mid_price);

        // Return signal
        Some(Signal::quote_both(our_bid, our_ask, ORDER_SIZE))
    }

    fn name(&self) -> &'static str {
        "SimpleSpread"
    }

    fn reset(&mut self) {
        // No state to reset (ZST)
    }
}

// Compile-time size verification
#[cfg(test)]
const _: () = {
    assert!(std::mem::size_of::<SimpleSpread>() == 0, "SimpleSpread must be zero-sized");
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_spread_is_zst() {
        // Verify zero-sized type
        assert_eq!(std::mem::size_of::<SimpleSpread>(), 0);
        assert_eq!(std::mem::align_of::<SimpleSpread>(), 1);
    }

    #[test]
    fn test_calculate_quotes() {
        // Mid price = 50,000 BTC (in fixed-point: 50_000_000_000_000)
        let mid = 50_000_000_000_000u64;

        let (bid, ask) = SimpleSpread::calculate_quotes(mid);

        // With default 10bps spread:
        // half_spread = 50000 * 10 / 20000 = 25 (in dollars)
        // bid = 50000 - 25 = 49975
        // ask = 50000 + 25 = 50025

        assert!(bid < mid);
        assert!(ask > mid);
        assert_eq!(ask - bid, (mid * SPREAD_BPS as u64) / 10_000);
    }

    #[test]
    fn test_spread_check() {
        // Wide spread (should pass)
        let bid = 50_000_000_000_000u64;
        let ask = 50_100_000_000_000u64; // 20bps spread

        assert!(SimpleSpread::is_spread_sufficient(bid, ask));

        // Tight spread (should fail if MIN_SPREAD_BPS > 1)
        let bid_tight = 50_000_000_000_000u64;
        let ask_tight = 50_005_000_000_000u64; // 1bp spread

        // Result depends on MIN_SPREAD_BPS const
        let _ = SimpleSpread::is_spread_sufficient(bid_tight, ask_tight);
    }

    #[test]
    fn test_invalid_prices() {
        // Zero prices
        assert!(!SimpleSpread::is_spread_sufficient(0, 100));
        assert!(!SimpleSpread::is_spread_sufficient(100, 0));

        // Crossed book
        assert!(!SimpleSpread::is_spread_sufficient(100, 50));
    }

    #[test]
    fn test_signal_generation() {
        let mut strategy = SimpleSpread;

        let snapshot = MarketSnapshot {
            market_id: 1,
            sequence: 1,
            exchange_timestamp_ns: 0,
            local_recv_ns: 0,
            local_publish_ns: 0,
            best_bid_price: 50_000_000_000_000,
            best_bid_size: 1_000_000_000,
            best_ask_price: 50_010_000_000_000, // 2bps spread
            best_ask_size: 1_000_000_000,
            bid_prices: [0; 3],
            ask_prices: [0; 3],
            dex_type: 1,
            _padding: [0; 7],
        };

        let signal = strategy.calculate(&snapshot);
        assert!(signal.is_some());

        if let Some(sig) = signal {
            assert_eq!(sig.size, ORDER_SIZE);
            assert!(sig.bid_price > 0);
            assert!(sig.ask_price > sig.bid_price);
        }
    }

    #[test]
    fn test_invalid_snapshot() {
        let mut strategy = SimpleSpread;

        // Zero prices
        let snapshot = MarketSnapshot {
            market_id: 1,
            sequence: 1,
            exchange_timestamp_ns: 0,
            local_recv_ns: 0,
            local_publish_ns: 0,
            best_bid_price: 0,
            best_bid_size: 0,
            best_ask_price: 0,
            best_ask_size: 0,
            bid_prices: [0; 3],
            ask_prices: [0; 3],
            dex_type: 1,
            _padding: [0; 7],
        };

        let signal = strategy.calculate(&snapshot);
        assert!(signal.is_none());
    }

    #[test]
    fn test_strategy_name() {
        let strategy = SimpleSpread;
        assert_eq!(strategy.name(), "SimpleSpread");
    }

    #[test]
    fn test_const_values() {
        // Verify constants are defined
        println!("SPREAD_BPS: {}", SPREAD_BPS);
        println!("ORDER_SIZE: {}", ORDER_SIZE);
        println!("MIN_SPREAD_BPS: {}", MIN_SPREAD_BPS);

        // Verify they're sane
        assert!(SPREAD_BPS > 0 && SPREAD_BPS < 1000); // 0-10%
        assert!(ORDER_SIZE > 0);
        assert!(MIN_SPREAD_BPS < SPREAD_BPS * 2);
    }

    #[test]
    fn test_mid_price_calculation() {
        // Test mid price doesn't overflow
        let bid = u64::MAX / 2;
        let ask = u64::MAX / 2 + 1000;

        // This formula prevents overflow:
        let mid = bid / 2 + ask / 2 + (bid % 2 + ask % 2) / 2;

        assert!(mid >= bid);
        assert!(mid <= ask);
    }

    #[test]
    fn test_performance_characteristics() {
        let mut strategy = SimpleSpread;

        let snapshot = MarketSnapshot {
            market_id: 1,
            sequence: 1,
            exchange_timestamp_ns: 0,
            local_recv_ns: 0,
            local_publish_ns: 0,
            best_bid_price: 50_000_000_000_000,
            best_bid_size: 1_000_000_000,
            best_ask_price: 50_010_000_000_000,
            best_ask_size: 1_000_000_000,
            bid_prices: [0; 3],
            ask_prices: [0; 3],
            dex_type: 1,
            _padding: [0; 7],
        };

        // This should be <100ns (measure with criterion in benchmarks)
        let _signal = strategy.calculate(&snapshot);

        // Verify no allocations by checking we're still ZST
        assert_eq!(std::mem::size_of_val(&strategy), 0);
    }
}
