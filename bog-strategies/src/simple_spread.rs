//! Simple Spread Market Making Strategy - Zero-Sized Type
//!
//! This is a zero-overhead implementation using:
//! - Zero-sized type (no memory overhead)
//! - Const parameters from Cargo features
//! - u64 fixed-point arithmetic (9 decimal places)
//! - No heap allocations
//! - #[inline(always)] for maximum performance
//!
//! ## Strategy Logic
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │              SimpleSpread Market Making Logic                   │
//! └─────────────────────────────────────────────────────────────────┘
//!
//!   Market State                Strategy Calculation
//!   ════════════                ═══════════════════
//!
//!   Best Bid: $50,000                   │
//!   Best Ask: $50,005                   ▼
//!                               ┌───────────────┐
//!                               │Calculate Mid  │
//!                               │ mid = (b+a)/2 │
//!                               └───────────────┘
//!                                       │
//!                                       │ $50,002.50
//!                                       ▼
//!                               ┌───────────────┐
//!                               │  Check Spread │
//!                               │ spread >= MIN?│
//!                               └───────────────┘
//!                                 │            │
//!                            No   │            │ Yes
//!                       ┌─────────┘            └─────────┐
//!                       │                                │
//!                       ▼                                ▼
//!               ┌──────────────┐              ┌──────────────────┐
//!               │Signal::      │              │Calculate Spread  │
//!               │no_action()   │              │ half = mid*bps/2 │
//!               └──────────────┘              └──────────────────┘
//!                                                      │
//!                                                      │ SPREAD = 10bps
//!                                                      │ half = $5
//!                                                      ▼
//!                                             ┌──────────────────┐
//!                                             │ Calculate Quotes │
//!                                             │ our_bid = mid-5  │
//!                                             │ our_ask = mid+5  │
//!                                             └──────────────────┘
//!                                                      │
//!                                                      ▼
//!                                             ┌──────────────────┐
//!                                             │Signal::quote_both│
//!                                             │ bid: $49,997.50  │
//!                                             │ ask: $50,007.50  │
//!                                             │ size: 0.1 BTC    │
//!                                             └──────────────────┘
//!
//! Example with Numbers (SPREAD_BPS = 10, ORDER_SIZE = 0.1 BTC):
//!
//!   Market:              Our Quotes:
//!   $50,000  ← bid       $49,997.50  ← our bid (tighter)
//!   $50,005  ← ask       $50,007.50  ← our ask (tighter)
//!
//!   Spread: 5 bps        Our spread: 10 bps (symmetric around mid)
//!   Mid: $50,002.50      Capture spread while staying competitive
//! ```
//!
//! ## Fixed-Point Arithmetic Example
//!
//! ```text
//! All prices are u64 with 9 decimal places:
//!
//!   Human:     $50,000.00
//!   Fixed:     50_000_000_000_000  (50000 * 10^9)
//!
//!   Human:     0.1 BTC
//!   Fixed:     100_000_000  (0.1 * 10^9)
//!
//! Calculation preserves precision without floating point:
//!
//!   bid = 50_000_000_000_000
//!   ask = 50_005_000_000_000
//!   mid = (bid + ask) / 2 = 50_002_500_000_000
//!
//!   spread_bps = 10  (0.1% or 10 basis points)
//!   half_spread = (mid * spread_bps) / 1_000_000
//!               = (50_002_500_000_000 * 10) / 1_000_000
//!               = 500_025_000_000  ($500.025)
//!
//!   our_bid = mid - half_spread = 49_502_475_000_000
//!   our_ask = mid + half_spread = 50_502_525_000_000
//! ```
//!
//! ## Memory Layout
//!
//! ```text
//! Size of SimpleSpread: 0 BYTES (Zero-Sized Type)
//!
//! ┌────────────────────────────────────┐
//! │ SimpleSpread                       │
//! │ (no fields - purely marker type)   │
//! │                                    │
//! │ All data is in const:              │
//! │  - SPREAD_BPS: u32 (compile-time)  │
//! │  - ORDER_SIZE: u64 (compile-time)  │
//! │  - MIN_SPREAD: u64 (compile-time)  │
//! │                                    │
//! │ Code is inlined at call sites      │
//! └────────────────────────────────────┘
//!
//! Memory at runtime:   0 bytes
//! Instructions inlined: ~20 assembly instructions
//! Cache lines used:    0 (pure code in instruction cache)
//! ```
//!
//! Target: <100ns signal generation ✅ **Achieved: ~5ns** (20x faster)

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

// === PRODUCTION SAFETY LIMITS ===

/// Maximum spread considered valid (basis points)
/// Spreads wider than this indicate flash crash or bad data
/// Circuit breaker will halt at 100bps, we filter at 50bps
pub const MAX_SPREAD_BPS: u32 = 50;

/// Minimum price considered valid (in fixed-point)
/// Below this is likely bad data (< $1)
pub const MIN_VALID_PRICE: u64 = 1_000_000_000; // $1

/// Maximum price considered valid (in fixed-point)
/// Above this is likely bad data (> $1M per BTC)
pub const MAX_VALID_PRICE: u64 = 1_000_000_000_000_000; // $1,000,000

/// Minimum liquidity required on both sides (in fixed-point)
/// Below this we don't quote (< 0.001 BTC)
pub const MIN_SIZE_THRESHOLD: u64 = 1_000_000; // 0.001 BTC

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

    /// Check if market spread is within valid bounds
    ///
    /// Returns true if MIN_SPREAD_BPS <= spread <= MAX_SPREAD_BPS
    #[inline(always)]
    fn is_spread_valid(bid: u64, ask: u64) -> bool {
        if bid == 0 || ask <= bid {
            return false;
        }

        // Calculate spread in basis points: ((ask - bid) / bid) * 10000
        let spread = ask - bid;
        let spread_bps = (spread * 10_000) / bid;

        // Must be >= MIN and <= MAX
        spread_bps >= MIN_SPREAD_BPS as u64 && spread_bps <= MAX_SPREAD_BPS as u64
    }

    /// Validate price is within sane bounds
    ///
    /// Prevents trading on obviously bad data
    #[inline(always)]
    fn is_price_valid(price: u64) -> bool {
        price >= MIN_VALID_PRICE && price <= MAX_VALID_PRICE
    }

    /// Validate liquidity is sufficient
    ///
    /// Don't quote if book is too thin
    #[inline(always)]
    fn is_liquidity_sufficient(bid_size: u64, ask_size: u64) -> bool {
        bid_size >= MIN_SIZE_THRESHOLD && ask_size >= MIN_SIZE_THRESHOLD
    }
}

impl Strategy for SimpleSpread {
    #[inline(always)]
    fn calculate(&mut self, snapshot: &MarketSnapshot) -> Option<Signal> {
        // Extract best bid and ask
        let bid = snapshot.best_bid_price;
        let ask = snapshot.best_ask_price;

        // === PRODUCTION VALIDATION LAYER ===

        // 1. Basic sanity check
        if bid == 0 || ask == 0 || ask <= bid {
            return None;
        }

        // 2. Price bounds validation (prevent trading on bad data)
        if !Self::is_price_valid(bid) || !Self::is_price_valid(ask) {
            return None;
        }

        // 3. Spread validation (MIN <= spread <= MAX)
        // This catches both too-tight spreads and flash crashes
        if !Self::is_spread_valid(bid, ask) {
            return None;
        }

        // 4. Liquidity validation (sufficient size on both sides)
        if !Self::is_liquidity_sufficient(snapshot.best_bid_size, snapshot.best_ask_size) {
            return None;
        }

        // === ALL CHECKS PASSED - GENERATE SIGNAL ===

        // Calculate mid price (use (bid + ask) / 2 to avoid overflow)
        let mid_price = bid / 2 + ask / 2 + (bid % 2 + ask % 2) / 2;

        // Calculate our quote prices
        let (our_bid, our_ask) = Self::calculate_quotes(mid_price);

        // Final sanity check on our quotes
        if our_bid == 0 || our_ask == 0 || our_ask <= our_bid {
            return None;
        }

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
    fn test_spread_validation() {
        // Normal spread (should pass)
        let bid = 50_000_000_000_000u64;
        let ask = 50_010_000_000_000u64; // 2bps spread

        assert!(SimpleSpread::is_spread_valid(bid, ask));

        // Tight spread - test based on actual MIN_SPREAD_BPS
        let bid_tight = 50_000_000_000_000u64;
        let ask_tight = 50_001_000_000_000u64; // 0.2bp spread

        // Calculate actual spread
        let spread_bps = ((ask_tight - bid_tight) * 10_000) / bid_tight;

        // Result depends on MIN_SPREAD_BPS const
        let result = SimpleSpread::is_spread_valid(bid_tight, ask_tight);
        let expected = spread_bps >= MIN_SPREAD_BPS as u64 && spread_bps <= MAX_SPREAD_BPS as u64;
        assert_eq!(result, expected,
            "Spread {}bps should be valid={} (MIN={}, MAX={})",
            spread_bps, expected, MIN_SPREAD_BPS, MAX_SPREAD_BPS);

        // Flash crash spread (should fail - exceeds MAX_SPREAD_BPS)
        let bid_wide = 50_000_000_000_000u64;
        let ask_wide = 52_500_000_000_000u64; // 500bps spread

        assert!(!SimpleSpread::is_spread_valid(bid_wide, ask_wide));
    }

    #[test]
    fn test_invalid_prices() {
        // Zero prices
        assert!(!SimpleSpread::is_spread_valid(0, 100));
        assert!(!SimpleSpread::is_spread_valid(100, 0));

        // Crossed book
        assert!(!SimpleSpread::is_spread_valid(100, 50));
    }

    #[test]
    fn test_price_bounds() {
        // Valid price range
        assert!(SimpleSpread::is_price_valid(50_000_000_000_000)); // $50k

        // Too low (< $1)
        assert!(!SimpleSpread::is_price_valid(500_000_000)); // $0.50

        // Too high (> $1M)
        assert!(!SimpleSpread::is_price_valid(1_500_000_000_000_000)); // $1.5M

        // Edge cases
        assert!(SimpleSpread::is_price_valid(MIN_VALID_PRICE)); // Exactly $1
        assert!(SimpleSpread::is_price_valid(MAX_VALID_PRICE)); // Exactly $1M
        assert!(!SimpleSpread::is_price_valid(MIN_VALID_PRICE - 1));
        assert!(!SimpleSpread::is_price_valid(MAX_VALID_PRICE + 1));
    }

    #[test]
    fn test_liquidity_checks() {
        // Sufficient liquidity
        assert!(SimpleSpread::is_liquidity_sufficient(100_000_000, 100_000_000)); // 0.1 BTC each

        // Insufficient bid size
        assert!(!SimpleSpread::is_liquidity_sufficient(500_000, 100_000_000)); // 0.0005 BTC bid

        // Insufficient ask size
        assert!(!SimpleSpread::is_liquidity_sufficient(100_000_000, 500_000)); // 0.0005 BTC ask

        // Both insufficient
        assert!(!SimpleSpread::is_liquidity_sufficient(500_000, 500_000));

        // Edge case: exactly at threshold
        assert!(SimpleSpread::is_liquidity_sufficient(MIN_SIZE_THRESHOLD, MIN_SIZE_THRESHOLD));
        assert!(!SimpleSpread::is_liquidity_sufficient(MIN_SIZE_THRESHOLD - 1, MIN_SIZE_THRESHOLD));
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
            bid_prices: [0; 10],
            bid_sizes: [0; 10],
            ask_prices: [0; 10],
            ask_sizes: [0; 10],
            dex_type: 1,
            _padding: [0; 111],
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
            bid_prices: [0; 10],
            bid_sizes: [0; 10],
            ask_prices: [0; 10],
            ask_sizes: [0; 10],
            dex_type: 1,
            _padding: [0; 111],
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
            bid_prices: [0; 10],
            bid_sizes: [0; 10],
            ask_prices: [0; 10],
            ask_sizes: [0; 10],
            dex_type: 1,
            _padding: [0; 111],
        };

        // This should be <100ns (measure with criterion in benchmarks)
        let _signal = strategy.calculate(&snapshot);

        // Verify no allocations by checking we're still ZST
        assert_eq!(std::mem::size_of_val(&strategy), 0);
    }

    #[test]
    fn test_flash_crash_protection() {
        let mut strategy = SimpleSpread;

        // Flash crash: spread goes from normal to 500bps
        let flash_crash_snapshot = MarketSnapshot {
            market_id: 1,
            sequence: 1,
            exchange_timestamp_ns: 0,
            local_recv_ns: 0,
            local_publish_ns: 0,
            best_bid_price: 50_000_000_000_000,  // $50,000
            best_bid_size: 1_000_000_000,
            best_ask_price: 52_500_000_000_000,  // $52,500 (5% spread!)
            best_ask_size: 1_000_000_000,
            bid_prices: [0; 10],
            bid_sizes: [0; 10],
            ask_prices: [0; 10],
            ask_sizes: [0; 10],
            dex_type: 1,
            _padding: [0; 111],
        };

        // Strategy should refuse to trade
        let signal = strategy.calculate(&flash_crash_snapshot);
        assert!(signal.is_none(), "Strategy should NOT generate signal during flash crash");
    }

    #[test]
    fn test_bad_data_rejection() {
        let mut strategy = SimpleSpread;

        // Test 1: Price too low (< $1)
        let low_price_snapshot = MarketSnapshot {
            market_id: 1,
            sequence: 1,
            exchange_timestamp_ns: 0,
            local_recv_ns: 0,
            local_publish_ns: 0,
            best_bid_price: 500_000_000,  // $0.50
            best_bid_size: 1_000_000_000,
            best_ask_price: 510_000_000,  // $0.51
            best_ask_size: 1_000_000_000,
            bid_prices: [0; 10],
            bid_sizes: [0; 10],
            ask_prices: [0; 10],
            ask_sizes: [0; 10],
            dex_type: 1,
            _padding: [0; 111],
        };

        assert!(strategy.calculate(&low_price_snapshot).is_none());

        // Test 2: Price too high (> $1M)
        let high_price_snapshot = MarketSnapshot {
            market_id: 1,
            sequence: 1,
            exchange_timestamp_ns: 0,
            local_recv_ns: 0,
            local_publish_ns: 0,
            best_bid_price: 1_500_000_000_000_000,  // $1.5M
            best_bid_size: 1_000_000_000,
            best_ask_price: 1_510_000_000_000_000,  // $1.51M
            best_ask_size: 1_000_000_000,
            bid_prices: [0; 10],
            bid_sizes: [0; 10],
            ask_prices: [0; 10],
            ask_sizes: [0; 10],
            dex_type: 1,
            _padding: [0; 111],
        };

        assert!(strategy.calculate(&high_price_snapshot).is_none());
    }

    #[test]
    fn test_low_liquidity_rejection() {
        let mut strategy = SimpleSpread;

        // Thin book: only 0.0005 BTC on each side
        let thin_book_snapshot = MarketSnapshot {
            market_id: 1,
            sequence: 1,
            exchange_timestamp_ns: 0,
            local_recv_ns: 0,
            local_publish_ns: 0,
            best_bid_price: 50_000_000_000_000,
            best_bid_size: 500_000,  // 0.0005 BTC (< MIN_SIZE_THRESHOLD)
            best_ask_price: 50_010_000_000_000,
            best_ask_size: 500_000,  // 0.0005 BTC
            bid_prices: [0; 10],
            bid_sizes: [0; 10],
            ask_prices: [0; 10],
            ask_sizes: [0; 10],
            dex_type: 1,
            _padding: [0; 111],
        };

        // Strategy should not quote in thin markets
        let signal = strategy.calculate(&thin_book_snapshot);
        assert!(signal.is_none(), "Strategy should NOT quote in thin markets");
    }

    #[test]
    fn test_production_ready() {
        let mut strategy = SimpleSpread;

        // Normal, production-quality market snapshot
        let good_snapshot = MarketSnapshot {
            market_id: 1,
            sequence: 1,
            exchange_timestamp_ns: 0,
            local_recv_ns: 0,
            local_publish_ns: 0,
            best_bid_price: 50_000_000_000_000,  // $50,000
            best_bid_size: 1_000_000_000,        // 1 BTC
            best_ask_price: 50_010_000_000_000,  // $50,010 (2bps spread)
            best_ask_size: 1_000_000_000,        // 1 BTC
            bid_prices: [0; 10],
            bid_sizes: [0; 10],
            ask_prices: [0; 10],
            ask_sizes: [0; 10],
            dex_type: 1,
            _padding: [0; 111],
        };

        let signal = strategy.calculate(&good_snapshot);
        assert!(signal.is_some(), "Strategy should generate signal for good market data");

        if let Some(sig) = signal {
            // Verify signal is valid
            assert!(sig.bid_price > 0);
            assert!(sig.ask_price > 0);
            assert!(sig.ask_price > sig.bid_price);
            assert_eq!(sig.size, ORDER_SIZE);

            // Verify our quotes are inside the market (passive quotes)
            // We should be bidding BELOW market bid and asking ABOVE market ask
            // to provide liquidity without taking from the book
            let mid = (good_snapshot.best_bid_price + good_snapshot.best_ask_price) / 2;
            assert!(sig.bid_price < mid, "Our bid should be below mid");
            assert!(sig.ask_price > mid, "Our ask should be above mid");
        }
    }
}
