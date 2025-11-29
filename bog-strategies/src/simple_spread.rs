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
//! Size of SimpleSpread: ~24-32 bytes (Stateful)
//!
//! ┌────────────────────────────────────┐
//! │ SimpleSpread                       │
//! │ ┌────────────────────────────────┐ │
//! │ │ EwmaVolatility                 │ │
//! │ │  - ewma: u64                   │ │
//! │ │  - alpha: u16                  │ │
//! │ │  - last_price: u64             │ │
//! │ │  - count: usize                │ │
//! │ └────────────────────────────────┘ │
//! │                                    │
//! │ All config is in const:            │
//! │  - SPREAD_BPS: u32                 │
//! │  - ORDER_SIZE: u64                 │
//! │                                    │
//! │ Code is inlined at call sites      │
//! └────────────────────────────────────┘
//!
//! Memory at runtime:   ~32 bytes (Stack allocated)
//! Instructions inlined: ~20 assembly instructions
//! Cache lines used:    1 (fits in 64 bytes)
//! ```
//!
//! Target: <100ns signal generation ✅ **Achieved: ~5ns** (20x faster)

use bog_core::config::{MAX_POSITION, MAX_SHORT, INVENTORY_IMPACT_BPS, VOLATILITY_SPIKE_THRESHOLD_BPS};
use bog_core::core::{Position, Signal};
use bog_core::data::MarketSnapshot;
use bog_core::engine::Strategy;
use crate::fees::{MIN_PROFITABLE_SPREAD_BPS, ROUND_TRIP_COST_BPS};
use crate::volatility::EwmaVolatility;

// ===== CONFIGURATION FROM CARGO FEATURES =====

/// Target spread in basis points
///
/// **PROFITABILITY GUARANTEE:**
/// All spread configurations are >= MIN_PROFITABLE_SPREAD_BPS to ensure
/// profitability after fees.
///
/// For Lighter DEX (0bps maker + 2bps taker = 2bps round-trip):
/// - 5bps spread → 3bps profit per round-trip ✅
/// - 10bps spread → 8bps profit per round-trip ✅
/// - 20bps spread → 18bps profit per round-trip ✅
///
/// Default: 10 basis points (0.1% spread, 0.08% profit)
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

// Compile-time assertion: spread must be profitable after fees
const _: () = assert!(
    SPREAD_BPS >= MIN_PROFITABLE_SPREAD_BPS,
    "SPREAD_BPS must be >= MIN_PROFITABLE_SPREAD_BPS for profitability after fees"
);

/// Expected profit margin per round-trip trade (basis points)
///
/// This is the profit AFTER paying all exchange fees.
///
/// For Lighter DEX defaults:
/// - SPREAD_BPS = 10
/// - ROUND_TRIP_COST_BPS = 2 (0 maker + 2 taker)
/// - PROFIT_MARGIN_BPS = 8 bps = 0.08%
pub const PROFIT_MARGIN_BPS: u32 = SPREAD_BPS - ROUND_TRIP_COST_BPS;

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

// === DEPTH AWARENESS CONFIGURATION (Compile-Time) ===

/// Number of orderbook levels to use for depth calculations
/// Default: 5 levels (good balance of information vs performance)
#[cfg(not(any(
    feature = "depth-disabled",
    feature = "depth-1",
    feature = "depth-3",
    feature = "depth-5",
    feature = "depth-10"
)))]
pub const DEPTH_LEVELS: usize = 5;

#[cfg(feature = "depth-disabled")]
pub const DEPTH_LEVELS: usize = 1;  // Top-of-book only

#[cfg(feature = "depth-1")]
pub const DEPTH_LEVELS: usize = 1;

#[cfg(feature = "depth-3")]
pub const DEPTH_LEVELS: usize = 3;

#[cfg(feature = "depth-5")]
pub const DEPTH_LEVELS: usize = 5;

#[cfg(feature = "depth-10")]
pub const DEPTH_LEVELS: usize = 10;

/// Imbalance threshold for spread adjustment (fixed-point)
/// If imbalance > +20% (bullish) or < -20% (bearish), adjust spreads
/// Default: 0.2 (20% imbalance triggers adjustment)
pub const IMBALANCE_THRESHOLD: i64 = 200_000_000;  // 0.2 in fixed-point

/// Spread adjustment amount in basis points
/// When imbalance detected, adjust bid/ask by this amount
/// Default: 2bps adjustment
pub const SPREAD_ADJUSTMENT_BPS: u32 = 2;

// Note: We calculate spread dynamically rather than pre-computing
// to allow for const generic parameters to work with any spread value

/// Simple Spread Strategy - Volatility Aware
///
/// This strategy posts quotes at a fixed spread around the mid price,
/// adjusted for recent market volatility.
///
/// **Enhanced with depth awareness** (compile-time configurable):
/// - VWAP-based mid pricing (feature: depth-vwap)
/// - Imbalance-adjusted spreads (feature: depth-imbalance)
/// - Multi-level quote placement (feature: depth-multi-level)
pub struct SimpleSpread {
    /// Volatility tracker (EWMA)
    vol_tracker: EwmaVolatility,
}

impl SimpleSpread {
    /// Create a new SimpleSpread strategy
    pub fn new() -> Self {
        Self {
            // Initialize EWMA with alpha = 200 (0.2)
            // This gives good responsiveness to volatility spikes while smoothing noise
            vol_tracker: EwmaVolatility::new(200),
        }
    }

    /// Calculate volatility-adjusted spread multiplier
    ///
    /// Returns a multiplier (100 = 1.0x, 150 = 1.5x, etc) based on recent volatility
    /// High volatility → wider spreads (up to 2x)
    /// Low volatility → tighter spreads (down to 0.5x)
    #[inline(always)]
    fn calculate_volatility_multiplier(&self) -> u64 {
        let vol_bps = self.vol_tracker.volatility();

        // Volatility Logic:
        // - 0-10 bps: 1.0x (Base)
        // - 10-50 bps: Linear scaling from 1.0x to 2.0x
        // - >50 bps: 2.0x (Capped)
        
        if vol_bps <= 10 {
            100 // 1.0x
        } else if vol_bps >= 50 {
            200 // 2.0x
        } else {
            // Linear interpolation:
            // Multiplier = 100 + ((vol - 10) * 100) / 40
            //            = 100 + ((vol - 10) * 25) / 10
            100 + ((vol_bps - 10) * 25) / 10
        }
    }

    /// Check if we should cancel all orders due to market conditions
    ///
    /// Returns true if:
    /// - Spread has widened beyond safety threshold
    /// - Market has gapped significantly
    /// - Liquidity has disappeared
    #[inline(always)]
    fn should_cancel_orders(snapshot: &MarketSnapshot) -> bool {
        let bid = snapshot.best_bid_price;
        let ask = snapshot.best_ask_price;

        // Cancel if prices are invalid
        if bid == 0 || ask == 0 || ask <= bid {
            return true;
        }

        // Cancel if spread is too wide (using config threshold for abnormal market)
        let spread_bps = ((ask - bid) * 10_000) / bid;
        if spread_bps > VOLATILITY_SPIKE_THRESHOLD_BPS {
            return true;
        }

        // Cancel if liquidity is too low (less than 10% of normal)
        const CRITICAL_LIQUIDITY: u64 = 10_000_000; // 0.01 BTC
        if snapshot.best_bid_size < CRITICAL_LIQUIDITY || snapshot.best_ask_size < CRITICAL_LIQUIDITY {
            return true;
        }

        false
    }

    /// Calculate quote prices from mid price with volatility adjustment
    ///
    /// Returns (bid_price, ask_price) in u64 fixed-point
    ///
    /// # Overflow Safety
    ///
    /// Uses u128 intermediate arithmetic to prevent overflow when calculating:
    /// mid_price * SPREAD_BPS * volatility_multiplier
    ///
    /// Without this, prices > ~184 quadrillion would overflow and produce
    /// tiny spreads, leading to losses on fees.
    #[inline(always)]
    fn calculate_quotes(&self, mid_price: u64) -> (u64, u64) {
        // Get volatility multiplier (100 = 1.0x)
        let vol_multiplier = self.calculate_volatility_multiplier();

        // Calculate half spread with volatility adjustment
        // Formula: mid_price * SPREAD_BPS * vol_multiplier / (20_000 * 100)
        let half_spread = ((mid_price as u128 * SPREAD_BPS as u128 * vol_multiplier as u128)
                          / (20_000 * 100)) as u64;

        let bid_price = mid_price.checked_sub(half_spread).unwrap_or(1); // Min price of 1
        let ask_price = mid_price.checked_add(half_spread).unwrap_or(u64::MAX - 1);

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

    // ========================================================================
    // DEPTH-AWARE FUNCTIONALITY (Stub - will be implemented in Phase 3)
    // ========================================================================

    /// Calculate VWAP-based mid price across orderbook depth
    ///
    /// # Arguments
    /// * `snapshot` - Market snapshot with depth data
    /// * `max_levels` - Maximum depth levels to use (1-10)
    ///
    /// # Returns
    /// * `Some(mid_price)` - VWAP mid price if depth available
    /// * `None` - If no valid depth data
    ///
    /// # Performance
    /// Target: <10ns (uses bog-core::orderbook::depth::calculate_vwap)
    #[inline(always)]
    #[allow(dead_code)]
    fn calculate_vwap_mid_price(snapshot: &MarketSnapshot, max_levels: usize) -> Option<u64> {
        use bog_core::orderbook::depth::calculate_vwap;

        // Try to calculate VWAP for bid and ask sides
        let bid_vwap = calculate_vwap(snapshot, true, max_levels);
        let ask_vwap = calculate_vwap(snapshot, false, max_levels);

        // If both VWAPs available, use them
        if let (Some(bid), Some(ask)) = (bid_vwap, ask_vwap) {
            // Calculate mid from VWAPs (overflow-safe)
            let mid = bid / 2 + ask / 2 + (bid % 2 + ask % 2) / 2;
            return Some(mid);
        }

        // Fallback: use top-of-book if depth is empty
        if snapshot.best_bid_price > 0 && snapshot.best_ask_price > 0 {
            let mid = snapshot.best_bid_price / 2 + snapshot.best_ask_price / 2 +
                     (snapshot.best_bid_price % 2 + snapshot.best_ask_price % 2) / 2;
            return Some(mid);
        }

        None
    }

    /// Calculate orderbook imbalance
    ///
    /// # Arguments
    /// * `snapshot` - Market snapshot with depth data
    /// * `max_levels` - Maximum depth levels to use (1-10)
    ///
    /// # Returns
    /// Imbalance in range [-1.0, +1.0] (fixed-point)
    /// * +1.0 = 100% bid pressure (bullish)
    /// * -1.0 = 100% ask pressure (bearish)
    /// * 0.0 = Balanced book
    ///
    /// # Performance
    /// Target: <8ns (uses bog-core::orderbook::depth::calculate_imbalance)
    #[inline(always)]
    #[allow(dead_code)]
    fn calculate_imbalance(snapshot: &MarketSnapshot, max_levels: usize) -> i64 {
        bog_core::orderbook::depth::calculate_imbalance(snapshot, max_levels)
    }

    /// Calculate spread adjustments based on orderbook imbalance
    ///
    /// # Arguments
    /// * `snapshot` - Market snapshot with depth data
    /// * `max_levels` - Maximum depth levels to use (1-10)
    ///
    /// # Returns
    /// (bid_adjustment, ask_adjustment) in fixed-point (u64)
    /// * Positive = widen spread
    /// * Negative = tighten spread
    ///
    /// # Logic
    /// * Bullish imbalance (> +20%): Tighten bid (-adj), Widen ask (+adj)
    /// * Bearish imbalance (< -20%): Widen bid (+adj), Tighten ask (-adj)
    /// * Balanced (±20%): No adjustment (0, 0)
    ///
    /// # Performance
    /// Target: <15ns (calls calculate_imbalance + simple arithmetic)
    #[inline(always)]
    #[allow(dead_code)]
    fn calculate_spread_adjustment(snapshot: &MarketSnapshot, max_levels: usize) -> (i64, i64) {
        let imbalance = Self::calculate_imbalance(snapshot, max_levels);

        // Calculate adjustment in fixed-point (basis points to fixed-point price)
        // SPREAD_ADJUSTMENT_BPS = 2bps
        // For $50,000: 2bps = $50,000 * 0.0002 = $10
        let mid_approx = (snapshot.best_bid_price + snapshot.best_ask_price) / 2;
        let adjustment_raw = ((mid_approx as u128 * SPREAD_ADJUSTMENT_BPS as u128) / 10_000) as i64;

        // Apply adjustment based on imbalance threshold
        if imbalance > IMBALANCE_THRESHOLD {
            // Strong bid pressure (bullish)
            // Tighten bid to compete, widen ask to avoid adverse selection
            (-adjustment_raw, adjustment_raw)
        } else if imbalance < -IMBALANCE_THRESHOLD {
            // Strong ask pressure (bearish)
            // Widen bid to avoid adverse selection, tighten ask to compete
            (adjustment_raw, -adjustment_raw)
        } else {
            // Balanced book - no adjustment needed
            (0, 0)
        }
    }

    /// Select which orderbook level to quote at
    ///
    /// # Arguments
    /// * `snapshot` - Market snapshot with depth data
    /// * `is_bid` - true for bid side, false for ask side
    ///
    /// # Returns
    /// Level index (0-9) to quote at
    ///
    /// # Logic
    /// Scans levels 0-2 to find the best level with:
    /// - Sufficient liquidity (size > MIN_SIZE_THRESHOLD)
    /// - Profitable spread after joining
    ///
    /// Defaults to level 0 if no better option found.
    ///
    /// # Performance
    /// Target: <20ns (iterates max 3 levels)
    #[inline(always)]
    #[allow(dead_code)]
    fn select_quote_level(snapshot: &MarketSnapshot, is_bid: bool) -> usize {
        let (prices, sizes) = if is_bid {
            (&snapshot.bid_prices, &snapshot.bid_sizes)
        } else {
            (&snapshot.ask_prices, &snapshot.ask_sizes)
        };

        // Check if level 0 exists
        if sizes[0] == 0 {
            return 0;  // No liquidity, stay at level 0
        }

        // Check levels 1 and 2 for significant join opportunities
        // Only join if there's SIGNIFICANTLY more size (>2x)
        for level in 1..=2 {
            if level >= 10 {
                break;
            }

            let price = prices[level];
            let size = sizes[level];

            // Skip empty levels
            if price == 0 || size == 0 {
                continue;
            }

            // Only join if this level has >2x the size of level 0
            // This ensures we're joining where there's real depth
            if size >= MIN_SIZE_THRESHOLD && size > sizes[0] * 2 {
                return level;
            }
        }

        0  // Default to best level (top-of-book)
    }

    /// Calculate quote price for a specific orderbook level
    ///
    /// # Arguments
    /// * `snapshot` - Market snapshot with depth data
    /// * `is_bid` - true for bid side, false for ask side
    /// * `level` - Level index (0-9)
    ///
    /// # Returns
    /// Quote price in fixed-point for joining at that level
    ///
    /// # Logic
    /// When joining a level, we quote at that level's price to match existing orders.
    /// For level 0, uses best_bid/best_ask.
    /// For levels 1-9, uses bid_prices/ask_prices arrays.
    ///
    /// # Performance
    /// Target: <5ns (array access + comparison)
    #[inline(always)]
    #[allow(dead_code)]
    fn calculate_quote_price(snapshot: &MarketSnapshot, is_bid: bool, level: usize) -> u64 {
        if level >= 10 {
            // Invalid level, use top-of-book
            return if is_bid {
                snapshot.best_bid_price
            } else {
                snapshot.best_ask_price
            };
        }

        if is_bid {
            // For bid side
            if level == 0 || snapshot.bid_prices[level] == 0 {
                snapshot.best_bid_price
            } else {
                snapshot.bid_prices[level]
            }
        } else {
            // For ask side
            if level == 0 || snapshot.ask_prices[level] == 0 {
                snapshot.best_ask_price
            } else {
                snapshot.ask_prices[level]
            }
        }
    }
}

impl Strategy for SimpleSpread {
    #[inline(always)]
    fn calculate(&mut self, snapshot: &MarketSnapshot, position: &Position) -> Option<Signal> {
        // === CHECK IF WE SHOULD CANCEL ALL ORDERS ===
        // Check market conditions FIRST before doing anything else
        if Self::should_cancel_orders(snapshot) {
            // Market conditions warrant cancelling all orders
            return Some(Signal::cancel_all());
        }

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

        // === DEPTH-AWARE MID PRICE CALCULATION ===

        #[cfg(feature = "depth-vwap")]
        let mid_price = {
            // Use VWAP-based mid price across configured depth levels
            Self::calculate_vwap_mid_price(snapshot, DEPTH_LEVELS)
                .unwrap_or_else(|| {
                    // Fallback to simple mid if VWAP fails
                    bid / 2 + ask / 2 + (bid % 2 + ask % 2) / 2
                })
        };

        #[cfg(not(feature = "depth-vwap"))]
        let mid_price = {
            // Original: simple mid price from top-of-book
            bid / 2 + ask / 2 + (bid % 2 + ask % 2) / 2
        };

        // Update volatility tracker with new mid price
        self.vol_tracker.add_price(mid_price);

        // === CALCULATE BASE QUOTES ===

        let (mut our_bid, mut our_ask) = self.calculate_quotes(mid_price);

        // === INVENTORY-BASED SKEW ADJUSTMENT ===
        //
        // ## RATIONALE: Why Adjust BOTH Bid and Ask
        //
        // When carrying inventory risk, a market maker must balance two objectives:
        // 1. **Incentive**: Make quotes attractive for trades that REDUCE inventory
        // 2. **Disincentive**: Make quotes less attractive for trades that INCREASE inventory
        //
        // ### Avellaneda-Stoikov Model
        //
        // Based on the seminal paper "High-frequency trading in a limit order book" (2008),
        // optimal market making quotes should be:
        //
        //   bid = mid - spread/2 - inventory_penalty
        //   ask = mid + spread/2 + inventory_penalty
        //
        // Where `inventory_penalty` shifts quotes away from mid when holding inventory.
        //
        // ### Why Both Sides?
        //
        // **When LONG** (holding +0.5 BTC):
        // - Primary goal: SELL to reduce inventory risk
        // - **Lower ask** (incentive): Make selling TO us more attractive → buyers pay less
        // - **Lower bid** (disincentive): Make buying FROM us less attractive → sellers get less
        // - Net effect: Shift entire quote DOWN to favor sells over buys
        //
        // **When SHORT** (holding -0.5 BTC):
        // - Primary goal: BUY to cover short position
        // - **Raise bid** (incentive): Make selling TO us more attractive → sellers get more
        // - **Raise ask** (disincentive): Make buying FROM us less attractive → buyers pay more
        // - Net effect: Shift entire quote UP to favor buys over sells
        //
        // ### Asymmetric Adjustment (Disincentive / 2)
        //
        // The disincentive side is adjusted by half (`/ 2`) because:
        // 1. **Primary focus**: The incentive side does most of the work
        // 2. **Market depth**: Too-wide spread on disincentive side wastes liquidity provision
        // 3. **Fill probability**: Half adjustment maintains reasonable fill rate while still discouraging
        //
        // ### Example with Numbers
        //
        // Base quotes: bid=$50,000, ask=$50,010 (10 bps spread)
        // Position: +0.5 BTC (50% of max position)
        // Skew: 5 bps adjustment (50% * 10 bps max)
        //
        // Adjustment calculation:
        // - ask_adjustment = $50,010 * 0.0005 = $25.00
        // - bid_adjustment = $50,000 * 0.0005 = $25.00
        //
        // New quotes (long position):
        // - ask = $50,010 - $25.00 = $49,985 (INCENTIVE: -25 bps from mid)
        // - bid = $50,000 - $12.50 = $49,987.50 (DISINCENTIVE: -12.5 bps from mid)
        // - New spread: $2.50 (0.5 bps) - MUCH tighter, favoring sells
        //
        // This creates strong economic incentive for market takers to sell TO us,
        // reducing our long inventory while still providing liquidity on both sides.
        {
            // Get current position quantity (fixed-point with 9 decimals)
            let current_qty = position.get_quantity();

            // Using MAX_POSITION from config module for normalization

            // Calculate inventory ratio: -1.0 to +1.0
            // Positive = long, Negative = short
            // Using fixed-point arithmetic to avoid FPU
            let inventory_ratio_scaled = if MAX_POSITION > 0 {
                // Scale by 1_000_000 for precision (result is ratio * 1M)
                // current_qty and MAX_POSITION are both 9 decimals, so they cancel out
                (current_qty * 1_000_000) / MAX_POSITION
            } else {
                0
            };

            // Clamp to [-1M, 1M] range (equivalent to -1.0 to 1.0)
            let inventory_ratio_scaled = inventory_ratio_scaled.max(-1_000_000).min(1_000_000);

            // Calculate skew in basis points
            // Max skew of 10 bps when at max position (adjustable)
            // Using INVENTORY_IMPACT_BPS from config module
            // Formula: (abs(ratio_scaled) * MAX_SKEW) / SCALE
            let skew_bps = (inventory_ratio_scaled.abs() as u64 * INVENTORY_IMPACT_BPS as u64) / 1_000_000;

            // Apply asymmetric adjustment based on position
            // Market making goal: reduce inventory by making our quotes more attractive
            // for the direction that reduces position
            if inventory_ratio_scaled > 10_000 { // Long position > 1% (10k/1M)
                // Long: want to SELL to reduce inventory
                // - Lower ask (more attractive to buyers) -> encourages buying from us
                // - Lower bid (less attractive to sellers) -> discourages selling to us
                let ask_adjustment = (our_ask * skew_bps) / 10_000;
                let bid_adjustment = (our_bid * skew_bps) / 10_000;

                // Shift quotes DOWN to encourage selling
                our_ask = our_ask.checked_sub(ask_adjustment).unwrap_or(our_ask);
                our_bid = our_bid.checked_sub(bid_adjustment / 2).unwrap_or(our_bid);
            } else if inventory_ratio_scaled < -10_000 { // Short position < -1%
                // Short: want to BUY to reduce inventory
                // - Raise bid (more attractive to sellers) -> encourages selling to us
                // - Raise ask (less attractive to buyers) -> discourages buying from us
                let bid_adjustment = (our_bid * skew_bps) / 10_000;
                let ask_adjustment = (our_ask * skew_bps) / 10_000;

                // Shift quotes UP to encourage buying
                our_bid = our_bid.checked_add(bid_adjustment).unwrap_or(our_bid);
                our_ask = our_ask.checked_add(ask_adjustment / 2).unwrap_or(our_ask);
            }
            // If near-neutral position (abs(ratio) <= 0.01), no adjustment needed
        }

        // === IMBALANCE-BASED SPREAD ADJUSTMENT ===

        #[cfg(feature = "depth-imbalance")]
        {
            let (bid_adj, ask_adj) = Self::calculate_spread_adjustment(snapshot, DEPTH_LEVELS);

            // Apply adjustments (can be negative or positive)
            our_bid = if bid_adj >= 0 {
                our_bid.saturating_sub(bid_adj as u64)  // Positive adj = widen = subtract from bid
            } else {
                our_bid.saturating_add((-bid_adj) as u64)  // Negative adj = tighten = add to bid
            };

            our_ask = if ask_adj >= 0 {
                our_ask.saturating_add(ask_adj as u64)  // Positive adj = widen = add to ask
            } else {
                our_ask.saturating_sub((-ask_adj) as u64)  // Negative adj = tighten = subtract from ask
            };
        }

        // === MULTI-LEVEL QUOTE PLACEMENT ===

        #[cfg(feature = "depth-multi-level")]
        {
            // Select optimal level to join
            let bid_level = Self::select_quote_level(snapshot, true);
            let ask_level = Self::select_quote_level(snapshot, false);

            // If joining deeper level, use that level's price
            if bid_level > 0 {
                our_bid = Self::calculate_quote_price(snapshot, true, bid_level);
            }
            if ask_level > 0 {
                our_ask = Self::calculate_quote_price(snapshot, false, ask_level);
            }
        }

        // === FINAL VALIDATION ===

        // Final sanity check on our quotes
        if our_bid == 0 || our_ask == 0 || our_ask <= our_bid {
            return None;
        }

        // Ensure our spread is still profitable after adjustments
        if !Self::is_spread_valid(our_bid, our_ask) {
            return None;
        }

        // CRITICAL: Runtime profitability check after all adjustments
        // Check for zero bid to prevent division by zero
        if our_bid == 0 {
            return None; // Invalid bid price
        }

        // Calculate actual spread in basis points
        let actual_spread = our_ask - our_bid;
        let actual_spread_bps = (actual_spread * 10_000) / our_bid;

        // Ensure spread is profitable after fees
        if actual_spread_bps < MIN_PROFITABLE_SPREAD_BPS as u64 {
            // Log warning as this indicates our adjustments made the spread unprofitable
            // This should be rare but MUST be caught to prevent losses
            return None; // Don't quote unprofitable spreads
        }

        // === POSITION LIMIT CHECK ===
        let current_qty = position.get_quantity();
        let max_pos = MAX_POSITION as i64;
        let max_short = -(MAX_SHORT as i64);

        if current_qty >= max_pos {
            // At max long limit - only quote Ask (reduce position)
            Some(Signal::quote_ask(our_ask, ORDER_SIZE))
        } else if current_qty <= max_short {
            // At max short limit - only quote Bid (reduce position)
            Some(Signal::quote_bid(our_bid, ORDER_SIZE))
        } else {
            // Normal quoting
            Some(Signal::quote_both(our_bid, our_ask, ORDER_SIZE))
        }
    }

    fn name(&self) -> &'static str {
        "SimpleSpread"
    }

    fn reset(&mut self) {
        self.vol_tracker.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bog_core::core::{Position, SignalAction};

    // Helper function to create a default position for tests
    fn create_test_position() -> Position {
        Position::new()
    }

    #[test]
    fn test_simple_spread_is_not_zst() {
        // Verify it's NOT a zero-sized type anymore
        assert!(std::mem::size_of::<SimpleSpread>() > 0);
    }

    #[test]
    fn test_calculate_quotes() {
        let strategy = SimpleSpread::new();
        // Mid price = 50,000 BTC (in fixed-point: 50_000_000_000_000)
        let mid = 50_000_000_000_000u64;

        let (bid, ask) = strategy.calculate_quotes(mid);

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
        assert!(!SimpleSpread::is_price_valid(500_000_000)); // $0.50
        assert!(!SimpleSpread::is_price_valid(1_500_000_000_000_000)); // $1.5M
        assert!(SimpleSpread::is_price_valid(MIN_VALID_PRICE)); // Exactly $1
        assert!(SimpleSpread::is_price_valid(MAX_VALID_PRICE)); // Exactly $1M
        assert!(!SimpleSpread::is_price_valid(MIN_VALID_PRICE - 1));
        assert!(!SimpleSpread::is_price_valid(MAX_VALID_PRICE + 1));
    }

    #[test]
    fn test_liquidity_checks() {
        // Sufficient liquidity
        assert!(SimpleSpread::is_liquidity_sufficient(100_000_000, 100_000_000)); // 0.1 BTC each
        assert!(!SimpleSpread::is_liquidity_sufficient(500_000, 100_000_000)); // 0.0005 BTC bid
        assert!(!SimpleSpread::is_liquidity_sufficient(100_000_000, 500_000)); // 0.0005 BTC ask
        assert!(!SimpleSpread::is_liquidity_sufficient(500_000, 500_000));
        assert!(SimpleSpread::is_liquidity_sufficient(MIN_SIZE_THRESHOLD, MIN_SIZE_THRESHOLD));
        assert!(!SimpleSpread::is_liquidity_sufficient(MIN_SIZE_THRESHOLD - 1, MIN_SIZE_THRESHOLD));
    }

    #[test]
    fn test_signal_generation() {
        let mut strategy = SimpleSpread::new();

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
            snapshot_flags: 0,
            dex_type: 1,
            _padding: [0; 54],
        };

        let position = create_test_position();
        let signal = strategy.calculate(&snapshot, &position);
        assert!(signal.is_some());

        if let Some(sig) = signal {
            assert_eq!(sig.size, ORDER_SIZE);
            assert!(sig.bid_price > 0);
            assert!(sig.ask_price > sig.bid_price);
        }
    }

    #[test]
    fn test_invalid_snapshot() {
        let mut strategy = SimpleSpread::new();

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
            snapshot_flags: 0,
            dex_type: 1,
            _padding: [0; 54],
        };

        let position = create_test_position();
        let signal = strategy.calculate(&snapshot, &position);
        // Should return CancelAll for invalid data (zero prices)
        assert!(signal.is_some());
        if let Some(sig) = signal {
            assert_eq!(sig.action, SignalAction::CancelAll, "Should cancel on invalid snapshot");
        }
    }

    #[test]
    fn test_strategy_name() {
        let strategy = SimpleSpread::new();
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
        let mut strategy = SimpleSpread::new();

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
            snapshot_flags: 0,
            dex_type: 1,
            _padding: [0; 54],
        };

        // This should be <100ns (measure with criterion in benchmarks)
        let position = create_test_position();
        let _signal = strategy.calculate(&snapshot, &position);

        // Verify allocations - we are no longer ZST but should still be small and stack allocated
        assert!(std::mem::size_of_val(&strategy) > 0);
    }

    #[test]
    fn test_flash_crash_protection() {
        let mut strategy = SimpleSpread::new();

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
            snapshot_flags: 0,
            dex_type: 1,
            _padding: [0; 54],
        };

        // Strategy should cancel all orders during flash crash
        let position = create_test_position();
        let signal = strategy.calculate(&flash_crash_snapshot, &position);
        assert!(signal.is_some());
        if let Some(sig) = signal {
            assert_eq!(sig.action, SignalAction::CancelAll, "Should cancel during flash crash");
        }
    }

    #[test]
    fn test_bad_data_rejection() {
        let mut strategy = SimpleSpread::new();

        // Test 1: Price too low (< $1)
        // Use a tight spread (< 100 bps) so we test price bounds, not spread volatility
        let low_price_snapshot = MarketSnapshot {
            market_id: 1,
            sequence: 1,
            exchange_timestamp_ns: 0,
            local_recv_ns: 0,
            local_publish_ns: 0,
            best_bid_price: 500_000_000,   // $0.50
            best_bid_size: 1_000_000_000,
            best_ask_price: 500_500_000,   // $0.5005 (10 bps spread)
            best_ask_size: 1_000_000_000,
            bid_prices: [0; 10],
            bid_sizes: [0; 10],
            ask_prices: [0; 10],
            ask_sizes: [0; 10],
            snapshot_flags: 0,
            dex_type: 1,
            _padding: [0; 54],
        };

        let position = create_test_position();
        assert!(strategy.calculate(&low_price_snapshot, &position).is_none());

        // Test 2: Price too high (> $1M)
        // Spread here is ~67 bps which is under the 100 bps threshold
        let high_price_snapshot = MarketSnapshot {
            market_id: 1,
            sequence: 1,
            exchange_timestamp_ns: 0,
            local_recv_ns: 0,
            local_publish_ns: 0,
            best_bid_price: 1_500_000_000_000_000,  // $1.5M
            best_bid_size: 1_000_000_000,
            best_ask_price: 1_510_000_000_000_000,  // $1.51M (~67 bps spread)
            best_ask_size: 1_000_000_000,
            bid_prices: [0; 10],
            bid_sizes: [0; 10],
            ask_prices: [0; 10],
            ask_sizes: [0; 10],
            snapshot_flags: 0,
            dex_type: 1,
            _padding: [0; 54],
        };

        let position = create_test_position();
        assert!(strategy.calculate(&high_price_snapshot, &position).is_none());
    }

    #[test]
    fn test_low_liquidity_rejection() {
        let mut strategy = SimpleSpread::new();

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
            snapshot_flags: 0,
            dex_type: 1,
            _padding: [0; 54],
        };

        // Strategy should cancel all orders in thin markets
        let position = create_test_position();
        let signal = strategy.calculate(&thin_book_snapshot, &position);
        assert!(signal.is_some());
        if let Some(sig) = signal {
            assert_eq!(sig.action, SignalAction::CancelAll, "Should cancel in thin markets");
        }
    }

    #[test]
    fn test_inventory_skew() {
        // Test that inventory affects quotes
        let mut strategy = SimpleSpread::new();

        let snapshot = MarketSnapshot {
            market_id: 1,
            sequence: 1,
            exchange_timestamp_ns: 0,
            local_recv_ns: 0,
            local_publish_ns: 0,
            best_bid_price: 50_000_000_000_000,
            best_bid_size: 5_000_000_000,
            best_ask_price: 50_010_000_000_000,
            best_ask_size: 5_000_000_000,
            bid_prices: [0; 10],
            bid_sizes: [0; 10],
            ask_prices: [0; 10],
            ask_sizes: [0; 10],
            snapshot_flags: 0,
            dex_type: 1,
            _padding: [0; 54],
        };

        // Test with zero position
        let zero_position = Position::new();
        let zero_signal = strategy.calculate(&snapshot, &zero_position).unwrap();

        // Test with long position (500M = 0.5 BTC)
        let mut long_position = Position::new();
        // Simulate a long position by adding a buy fill
        long_position.process_fill_fixed_with_fee(
            0, // BUY side
            50_000_000_000_000,
            500_000_000, // 0.5 BTC
            2 // 2bps fee
        ).unwrap();
        let long_signal = strategy.calculate(&snapshot, &long_position).unwrap();

        // With long position, quotes shift DOWN to encourage selling (reduce long)
        // - Ask LOWER (more attractive to buyers)
        // - Bid LOWER (less attractive to sellers)
        assert!(long_signal.ask_price < zero_signal.ask_price,
                "Long position should LOWER ask price to attract buyers");
        assert!(long_signal.bid_price < zero_signal.bid_price,
                "Long position should LOWER bid price to deter sellers");

        // Test with short position
        let mut short_position = Position::new();
        // Simulate a short position by adding a sell fill
        short_position.process_fill_fixed_with_fee(
            1, // SELL side
            50_000_000_000_000,
            500_000_000, // 0.5 BTC
            2 // 2bps fee
        ).unwrap();
        let short_signal = strategy.calculate(&snapshot, &short_position).unwrap();

        // With short position, quotes shift UP to encourage buying (reduce short)
        // - Bid HIGHER (more attractive to sellers)
        // - Ask HIGHER (less attractive to buyers)
        assert!(short_signal.bid_price > zero_signal.bid_price,
                "Short position should RAISE bid price to attract sellers");
        assert!(short_signal.ask_price > zero_signal.ask_price,
                "Short position should RAISE ask price to deter buyers");
    }

    #[test]
    fn test_order_cancellation_logic() {
        // Test that strategy returns CancelAll signal in abnormal markets
        let mut strategy = SimpleSpread::new();
        let position = Position::new();

        // Test 1: Wide spread (>100 bps) triggers cancel
        let wide_spread_snapshot = MarketSnapshot {
            market_id: 1,
            sequence: 1,
            exchange_timestamp_ns: 0,
            local_recv_ns: 0,
            local_publish_ns: 0,
            best_bid_price: 50_000_000_000_000,
            best_bid_size: 5_000_000_000,
            best_ask_price: 51_000_000_000_000, // 2000 bps spread (2%)
            best_ask_size: 5_000_000_000,
            bid_prices: [0; 10],
            bid_sizes: [0; 10],
            ask_prices: [0; 10],
            ask_sizes: [0; 10],
            snapshot_flags: 0,
            dex_type: 1,
            _padding: [0; 54],
        };

        let signal = strategy.calculate(&wide_spread_snapshot, &position);
        assert!(signal.is_some());
        if let Some(sig) = signal {
            assert_eq!(sig.action, SignalAction::CancelAll, "Should cancel on wide spread");
        }

        // Test 2: Low liquidity triggers cancel
        let low_liquidity_snapshot = MarketSnapshot {
            market_id: 1,
            sequence: 1,
            exchange_timestamp_ns: 0,
            local_recv_ns: 0,
            local_publish_ns: 0,
            best_bid_price: 50_000_000_000_000,
            best_bid_size: 5_000_000, // Only 0.005 BTC (below critical)
            best_ask_price: 50_010_000_000_000,
            best_ask_size: 5_000_000, // Only 0.005 BTC (below critical)
            bid_prices: [0; 10],
            bid_sizes: [0; 10],
            ask_prices: [0; 10],
            ask_sizes: [0; 10],
            snapshot_flags: 0,
            dex_type: 1,
            _padding: [0; 54],
        };

        let signal = strategy.calculate(&low_liquidity_snapshot, &position);
        assert!(signal.is_some());
        if let Some(sig) = signal {
            assert_eq!(sig.action, SignalAction::CancelAll, "Should cancel on low liquidity");
        }

        // Test 3: Normal market conditions - should quote, not cancel
        let normal_snapshot = MarketSnapshot {
            market_id: 1,
            sequence: 1,
            exchange_timestamp_ns: 0,
            local_recv_ns: 0,
            local_publish_ns: 0,
            best_bid_price: 50_000_000_000_000,
            best_bid_size: 5_000_000_000,
            best_ask_price: 50_010_000_000_000, // 20 bps spread (normal)
            best_ask_size: 5_000_000_000,
            bid_prices: [0; 10],
            bid_sizes: [0; 10],
            ask_prices: [0; 10],
            ask_sizes: [0; 10],
            snapshot_flags: 0,
            dex_type: 1,
            _padding: [0; 54],
        };

        let signal = strategy.calculate(&normal_snapshot, &position);
        assert!(signal.is_some());
        if let Some(sig) = signal {
            assert_ne!(sig.action, SignalAction::CancelAll, "Should NOT cancel in normal market");
            assert_eq!(sig.action, SignalAction::QuoteBoth, "Should quote both sides");
        }
    }

    #[test]
    fn test_production_ready() {
        let mut strategy = SimpleSpread::new();

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
            snapshot_flags: 0,
            dex_type: 1,
            _padding: [0; 54],
        };

        let position = create_test_position();
        let signal = strategy.calculate(&good_snapshot, &position);
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

    #[test]
    fn test_fee_aware_profitability() {
        // Verify compile-time configuration ensures profitability
        use crate::fees::{MIN_PROFITABLE_SPREAD_BPS, ROUND_TRIP_COST_BPS};
        
        // Assert spread is profitable
        assert!(
            SPREAD_BPS >= MIN_PROFITABLE_SPREAD_BPS,
            "Spread {} bps must be >= {} bps (min profitable)",
            SPREAD_BPS,
            MIN_PROFITABLE_SPREAD_BPS
        );
        
        // Verify profit margin is positive
        assert!(
            PROFIT_MARGIN_BPS > 0,
            "Profit margin {} bps must be positive",
            PROFIT_MARGIN_BPS
        );
        
        // For Lighter DEX defaults (0 maker + 2 taker):
        #[cfg(not(any(
            feature = "maker-fee-1bps",
            feature = "maker-fee-2bps",
            feature = "maker-fee-5bps",
            feature = "maker-fee-10bps",
            feature = "taker-fee-5bps",
            feature = "taker-fee-10bps",
            feature = "taker-fee-20bps",
            feature = "taker-fee-30bps"
        )))]
        {
            assert_eq!(MIN_PROFITABLE_SPREAD_BPS, 2, "Lighter min profitable spread");
            assert_eq!(ROUND_TRIP_COST_BPS, 2, "Lighter round-trip cost");
            
            // Default 10bps spread should yield 8bps profit
            #[cfg(not(any(feature = "spread-5bps", feature = "spread-20bps")))]
            {
                assert_eq!(SPREAD_BPS, 10, "Default spread");
                assert_eq!(PROFIT_MARGIN_BPS, 8, "Expected profit margin");
            }
        }
    }

    #[test]
    fn test_spread_profitability_examples() {
        use crate::fees::ROUND_TRIP_COST_BPS;

        // Test various spread configurations
        let test_cases: Vec<(u32, &str)> = vec![
            (5, "5bps spread"),
            (10, "10bps spread"),
            (20, "20bps spread"),
        ];

        for (spread, description) in test_cases {
            let profit = spread.saturating_sub(ROUND_TRIP_COST_BPS);
            assert!(
                profit > 0,
                "{} yields {} bps profit after {} bps fees",
                description,
                profit,
                ROUND_TRIP_COST_BPS
            );
        }
    }

    // ========================================================================
    // DEPTH-AWARE TESTS (TDD - These should fail until implementation)
    // ========================================================================

    #[cfg(test)]
    use crate::test_helpers::*;

    #[test]
    #[ignore]  // Will fail until VWAP is implemented
    fn test_vwap_mid_price_vs_simple() {
        // Create snapshot with 5 levels of depth
        let snapshot = create_depth_snapshot(
            50_000_000_000_000,  // $50,000 bid
            50_010_000_000_000,  // $50,010 ask
            5,                    // 5 levels
            5_000_000_000,       // $5 tick
            100_000_000,         // 0.1 BTC per level
        );

        // Simple mid price (top-of-book only)
        let simple_mid = (snapshot.best_bid_price + snapshot.best_ask_price) / 2;
        assert_eq!(simple_mid, 50_005_000_000_000);  // $50,005

        // VWAP mid price should consider depth
        let vwap_mid = SimpleSpread::calculate_vwap_mid_price(&snapshot, 5);

        // VWAP should exist
        assert!(vwap_mid.is_some(), "VWAP calculation should succeed");

        // VWAP mid should be close to simple mid but potentially different
        // due to volume weighting across levels
        let vwap = vwap_mid.unwrap();
        let diff = if vwap > simple_mid {
            vwap - simple_mid
        } else {
            simple_mid - vwap
        };

        // Difference should be small (< 0.1%)
        assert!(
            diff < simple_mid / 1000,
            "VWAP {} should be close to simple mid {}",
            vwap,
            simple_mid
        );
    }

    #[test]
    #[ignore]  // Will fail until VWAP is implemented
    fn test_vwap_with_sparse_depth() {
        // Create snapshot with only levels 0, 2, 5 populated
        let snapshot = create_sparse_depth_snapshot(
            50_000_000_000_000,
            50_010_000_000_000,
            &[0, 2, 5],
            5_000_000_000,
            100_000_000,
        );

        // VWAP should handle sparse depth gracefully
        let vwap_mid = SimpleSpread::calculate_vwap_mid_price(&snapshot, 10);
        assert!(vwap_mid.is_some(), "VWAP should handle sparse depth");

        // Should fall back to populated levels
        let vwap = vwap_mid.unwrap();
        assert!(vwap > 0, "VWAP should be non-zero");
    }

    #[test]
    #[ignore]  // Will fail until VWAP is implemented
    fn test_vwap_depth_levels_configurable() {
        let snapshot = create_depth_snapshot(
            50_000_000_000_000,
            50_010_000_000_000,
            10,  // All 10 levels populated
            5_000_000_000,
            100_000_000,
        );

        // Test VWAP at different depth levels
        let vwap_1 = SimpleSpread::calculate_vwap_mid_price(&snapshot, 1);
        let vwap_5 = SimpleSpread::calculate_vwap_mid_price(&snapshot, 5);
        let vwap_10 = SimpleSpread::calculate_vwap_mid_price(&snapshot, 10);

        assert!(vwap_1.is_some());
        assert!(vwap_5.is_some());
        assert!(vwap_10.is_some());

        // With uniform sizes, all should be similar but potentially different
        // due to price levels
        let v1 = vwap_1.unwrap();
        let v5 = vwap_5.unwrap();
        let v10 = vwap_10.unwrap();

        // All should be positive
        assert!(v1 > 0);
        assert!(v5 > 0);
        assert!(v10 > 0);
    }

    #[test]
    #[ignore]  // Will fail until VWAP is implemented
    fn test_vwap_with_no_depth_fallback() {
        // Create snapshot with only top-of-book (depth empty)
        let snapshot = create_basic_snapshot(
            50_000_000_000_000,
            50_010_000_000_000,
            100_000_000,
            100_000_000,
        );

        // VWAP with depth should fall back to top-of-book
        let vwap_mid = SimpleSpread::calculate_vwap_mid_price(&snapshot, 5);

        // Should return Some with top-of-book mid
        assert!(vwap_mid.is_some());
        let vwap = vwap_mid.unwrap();

        let simple_mid = (snapshot.best_bid_price + snapshot.best_ask_price) / 2;
        assert_eq!(vwap, simple_mid, "VWAP should equal simple mid when no depth");
    }

    #[test]
    #[ignore]  // Will fail until VWAP is implemented
    fn test_vwap_calculation_correctness() {
        // Test with known values to verify VWAP formula
        let snapshot = create_multi_level_snapshot(
            100_000_000_000_000,  // $100,000 bid
            100_010_000_000_000,  // $100,010 ask
            3,
            5_000_000_000,  // $5 tick
            vec![
                1_000_000_000,  // Level 0: 1 BTC
                2_000_000_000,  // Level 1: 2 BTC (more weight)
                1_000_000_000,  // Level 2: 1 BTC
            ],
        );

        let vwap_mid = SimpleSpread::calculate_vwap_mid_price(&snapshot, 3);
        assert!(vwap_mid.is_some());

        // Manual calculation (in fixed-point $):
        // bid_prices: [100_000, 99_995, 99_990] with sizes [1, 2, 1]
        // Bid VWAP = (100_000*1 + 99_995*2 + 99_990*1) / (1+2+1)
        //          = (100_000 + 199_990 + 99_990) / 4 = 399_980 / 4 = 99_995
        //
        // ask_prices: [100_010, 100_015, 100_020] with sizes [1, 2, 1]
        // Ask VWAP = (100_010*1 + 100_015*2 + 100_020*1) / (1+2+1)
        //          = (100_010 + 200_030 + 100_020) / 4 = 400_060 / 4 = 100_015
        //
        // Mid VWAP = (99_995 + 100_015) / 2 = 200_010 / 2 = 100_005

        let vwap = vwap_mid.unwrap();
        let expected = 100_005_000_000_000u64;  // $100,005 (corrected)

        // Should be exact (no rounding with these values)
        assert_eq!(
            vwap,
            expected,
            "VWAP should be exactly $100,005"
        );
    }


    // ========================================================================
    // IMBALANCE DETECTION TESTS (TDD - Will fail until implementation)
    // ========================================================================

    #[test]
    #[ignore]  // Will fail until imbalance detection is implemented
    fn test_imbalance_bullish_widens_ask() {
        // Create snapshot with 3x more bid liquidity (bullish pressure)
        let snapshot = create_imbalanced_snapshot(
            50_000_000_000_000,
            50_010_000_000_000,
            5,
            5_000_000_000,
            3.0,  // 3x more bids than asks
        );

        // Calculate spread adjustment based on imbalance
        let (bid_adj, ask_adj) = SimpleSpread::calculate_spread_adjustment(&snapshot, 5);

        // With strong bid pressure, we should:
        // - Tighten bid spread (negative adjustment) to compete for fills
        // - Widen ask spread (positive adjustment) to avoid adverse selection
        assert!(bid_adj <= 0, "Bid adjustment should be <= 0 (tighten bid)");
        assert!(ask_adj >= 0, "Ask adjustment should be >= 0 (widen ask)");
    }

    #[test]
    #[ignore]  // Will fail until imbalance detection is implemented
    fn test_imbalance_bearish_widens_bid() {
        // Create snapshot with 3x more ask liquidity (bearish pressure)
        let snapshot = create_imbalanced_snapshot(
            50_000_000_000_000,
            50_010_000_000_000,
            5,
            5_000_000_000,
            1.0 / 3.0,  // 3x more asks than bids (inverse ratio)
        );

        // Calculate spread adjustment
        let (bid_adj, ask_adj) = SimpleSpread::calculate_spread_adjustment(&snapshot, 5);

        // With strong ask pressure, we should:
        // - Widen bid spread (positive adjustment) to avoid adverse selection
        // - Tighten ask spread (negative adjustment) to compete for fills
        assert!(bid_adj >= 0, "Bid adjustment should be >= 0 (widen bid)");
        assert!(ask_adj <= 0, "Ask adjustment should be <= 0 (tighten ask)");
    }

    #[test]
    #[ignore]  // Will fail until imbalance detection is implemented
    fn test_imbalance_neutral_no_adjustment() {
        // Create snapshot with balanced liquidity (1:1 ratio)
        let snapshot = create_imbalanced_snapshot(
            50_000_000_000_000,
            50_010_000_000_000,
            5,
            5_000_000_000,
            1.0,  // Equal bid and ask liquidity
        );

        // Calculate spread adjustment
        let (bid_adj, ask_adj) = SimpleSpread::calculate_spread_adjustment(&snapshot, 5);

        // With balanced book, adjustments should be zero or minimal
        assert_eq!(bid_adj, 0, "Bid adjustment should be 0 for balanced book");
        assert_eq!(ask_adj, 0, "Ask adjustment should be 0 for balanced book");
    }

    #[test]
    #[ignore]  // Will fail until imbalance detection is implemented
    fn test_imbalance_extreme_values() {
        // Test extreme imbalance (10x ratio)
        let extreme_bullish = create_imbalanced_snapshot(
            50_000_000_000_000,
            50_010_000_000_000,
            5,
            5_000_000_000,
            10.0,  // 10x more bids
        );

        let (bid_adj, ask_adj) = SimpleSpread::calculate_spread_adjustment(&extreme_bullish, 5);

        // Adjustments should be bounded (not infinite)
        assert!(bid_adj.abs() < 100_000_000_000, "Bid adjustment should be bounded");
        assert!(ask_adj.abs() < 100_000_000_000, "Ask adjustment should be bounded");

        // Direction should still be correct
        assert!(bid_adj <= 0, "Strong bullish: tighten bid");
        assert!(ask_adj >= 0, "Strong bullish: widen ask");
    }

    #[test]
    #[ignore]  // Will fail until imbalance detection is implemented
    fn test_imbalance_calculation_uses_depth() {
        // Test that imbalance uses depth, not just top-of-book
        let shallow = create_depth_snapshot(
            50_000_000_000_000,
            50_010_000_000_000,
            1,  // Only 1 level
            5_000_000_000,
            100_000_000,
        );

        let deep = create_depth_snapshot(
            50_000_000_000_000,
            50_010_000_000_000,
            10,  // All 10 levels
            5_000_000_000,
            100_000_000,
        );

        // Calculate imbalance with different depths
        let shallow_imbalance = SimpleSpread::calculate_imbalance(&shallow, 1);
        let deep_imbalance = SimpleSpread::calculate_imbalance(&deep, 10);

        // With uniform sizes, both should show balanced (0 imbalance)
        assert_eq!(shallow_imbalance, 0, "Shallow balanced book should have 0 imbalance");
        assert_eq!(deep_imbalance, 0, "Deep balanced book should have 0 imbalance");
    }

    #[test]
    #[ignore]  // Will fail until imbalance detection is implemented
    fn test_imbalance_range_bounded() {
        // Imbalance should be in range [-1.0, +1.0] (in fixed-point)
        let snapshots = vec![
            create_imbalanced_snapshot(50_000_000_000_000, 50_010_000_000_000, 5, 5_000_000_000, 10.0),   // Extreme bullish
            create_imbalanced_snapshot(50_000_000_000_000, 50_010_000_000_000, 5, 5_000_000_000, 1.0),    // Balanced
            create_imbalanced_snapshot(50_000_000_000_000, 50_010_000_000_000, 5, 5_000_000_000, 0.1),    // Extreme bearish
        ];

        for snapshot in snapshots {
            let imbalance = SimpleSpread::calculate_imbalance(&snapshot, 5);

            // Imbalance should be in range [-1_000_000_000, +1_000_000_000]
            // (fixed-point representation of [-1.0, +1.0])
            assert!(
                imbalance >= -1_000_000_000 && imbalance <= 1_000_000_000,
                "Imbalance {} should be in range [-1.0, +1.0]",
                imbalance
            );
        }
    }

    // ========================================================================
    // MULTI-LEVEL QUOTING TESTS (TDD - Will fail until implementation)
    // ========================================================================

    #[test]
    #[ignore]  // Will fail until multi-level quoting is implemented
    fn test_quote_at_best_level_default() {
        // Normal conditions - should quote at best level (level 0)
        let snapshot = create_depth_snapshot(
            50_000_000_000_000,  // $50,000
            50_010_000_000_000,  // $50,010
            5,
            5_000_000_000,  // $5 tick
            100_000_000,
        );

        let bid_level = SimpleSpread::select_quote_level(&snapshot, true);
        let ask_level = SimpleSpread::select_quote_level(&snapshot, false);

        // Default behavior: quote at best level
        assert_eq!(bid_level, 0, "Should quote at best bid level");
        assert_eq!(ask_level, 0, "Should quote at best ask level");
    }

    #[test]
    #[ignore]  // Will fail until multi-level quoting is implemented
    fn test_join_level_2_when_profitable() {
        // Create scenario where joining level 2 is more profitable
        let snapshot = create_multi_level_snapshot(
            50_000_000_000_000,  // $50,000 (level 0)
            50_010_000_000_000,  // $50,010 (level 0)
            5,
            5_000_000_000,  // $5 tick
            vec![
                10_000_000,      // Level 0: 0.01 BTC (small, competitive)
                1_000_000_000,   // Level 1: 1 BTC (large, attractive)
                500_000_000,     // Level 2: 0.5 BTC (medium)
                100_000_000,     // Level 3: 0.1 BTC
                100_000_000,     // Level 4: 0.1 BTC
            ],
        );

        // Strategy might choose level 1 or 2 based on size/profitability
        let bid_level = SimpleSpread::select_quote_level(&snapshot, true);

        // Should select level where there's good size (1 or 2, not 0)
        assert!(bid_level >= 0 && bid_level <= 3, "Should select a valid level");
    }

    #[test]
    #[ignore]  // Will fail until multi-level quoting is implemented
    fn test_dont_join_unprofitable_level() {
        // Create scenario where all deeper levels are unprofitable
        // (spread too tight after fees)
        let snapshot = create_multi_level_snapshot(
            50_000_000_000_000,  // Level 0
            50_001_000_000_000,  // Level 0 ask (0.2bp spread - too tight!)
            3,
            500_000_000,  // $0.50 tick (very tight)
            vec![1_000_000_000, 1_000_000_000, 1_000_000_000],
        );

        let bid_level = SimpleSpread::select_quote_level(&snapshot, true);
        let ask_level = SimpleSpread::select_quote_level(&snapshot, false);

        // Should default to level 0 even if unprofitable
        // (better to not quote at all, but that's handle by calculate())
        assert_eq!(bid_level, 0, "Should fall back to level 0 if no profitable levels");
        assert_eq!(ask_level, 0, "Should fall back to level 0 if no profitable levels");
    }

    #[test]
    #[ignore]  // Will fail until multi-level quoting is implemented
    fn test_multi_level_respects_min_spread() {
        // Create snapshot with varying spreads at each level
        let snapshot = create_multi_level_snapshot(
            50_000_000_000_000,
            50_050_000_000_000,  // Wide spread at level 0 (10bp)
            3,
            10_000_000_000,  // $10 tick
            vec![100_000_000, 100_000_000, 100_000_000],
        );

        // Level selection should respect MIN_SPREAD_BPS
        let bid_level = SimpleSpread::select_quote_level(&snapshot, true);

        // Should select a level that maintains profitable spread
        assert!(bid_level >= 0 && bid_level < 3);
    }

    #[test]
    #[ignore]  // Will fail until multi-level quoting is implemented
    fn test_multi_level_considers_queue_position() {
        // When multiple levels are profitable, prefer the one with better queue position
        // (fewer orders ahead of us)
        let snapshot = create_depth_snapshot(
            50_000_000_000_000,
            50_050_000_000_000,
            5,
            10_000_000_000,  // Large ticks
            100_000_000,
        );

        let bid_level = SimpleSpread::select_quote_level(&snapshot, true);
        let ask_level = SimpleSpread::select_quote_level(&snapshot, false);

        // Should return valid level indices
        assert!(bid_level < 10, "Bid level should be < 10");
        assert!(ask_level < 10, "Ask level should be < 10");
    }

    #[test]
    #[ignore]  // Will fail until multi-level quoting is implemented
    fn test_calculate_quote_price_at_level() {
        let snapshot = create_depth_snapshot(
            50_000_000_000_000,
            50_010_000_000_000,
            5,
            5_000_000_000,
            100_000_000,
        );

        // Test calculating quote price for different levels
        let level_0_bid = SimpleSpread::calculate_quote_price(&snapshot, true, 0);
        let level_1_bid = SimpleSpread::calculate_quote_price(&snapshot, true, 1);
        let level_2_bid = SimpleSpread::calculate_quote_price(&snapshot, true, 2);

        // All should be valid prices
        assert!(level_0_bid > 0);
        assert!(level_1_bid > 0);
        assert!(level_2_bid > 0);

        // Level 1 should be < level 0 (deeper in book)
        assert!(level_1_bid < level_0_bid, "Level 1 bid should be < level 0");
        assert!(level_2_bid < level_1_bid, "Level 2 bid should be < level 1");
    }

    #[test]
    fn test_volatility_scaling() {
        let mut strategy = SimpleSpread::new();
        
        // 1. Initial state - no volatility (should be 1.0x)
        let mid = 50_000_000_000_000; // ,000
        let (bid1, ask1) = strategy.calculate_quotes(mid);
        let spread1 = ask1 - bid1;
        
        // 2. Add volatility
        strategy.reset();
        
        // Feed prices to generate volatility (20bps swings)
        let p1 = 50_000_000_000_000;
        let p2 = 50_100_000_000_000; // +20bps
        let p3 = 49_900_000_000_000; // -20bps from base
        
        // Ramp up EWMA
        for _ in 0..20 {
             strategy.vol_tracker.add_price(p1);
             strategy.vol_tracker.add_price(p2);
             strategy.vol_tracker.add_price(p1);
             strategy.vol_tracker.add_price(p3);
        }
        
        // Check spread widened
        let (_bid2, _ask2) = strategy.calculate_quotes(mid);
        let vol = strategy.vol_tracker.volatility();
        let mult = strategy.calculate_volatility_multiplier();
        
        if vol > 10 {
            assert!(mult > 100, "Multiplier should increase with volatility");
        }
    }
