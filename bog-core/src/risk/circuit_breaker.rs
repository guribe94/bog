//! Circuit Breaker for Flash Crash and Market Anomaly Detection
//!
//! Detects extreme market conditions and halts trading to prevent losses:
//! - Flash crashes (extreme spread widening)
//! - Price spikes (sudden large price movements)
//! - Low liquidity (insufficient size on book)
//! - Stale data (no recent updates)
//!
//! ## Architecture
//!
//! ```text
//! ┌──────────────┐
//! │ MarketData   │
//! └──────┬───────┘
//!        │
//!        ▼
//! ┌──────────────────────────────────────┐
//! │    CircuitBreaker::check()           │
//! │  ┌────────────────────────────────┐  │
//! │  │ 1. Spread Check                │  │
//! │  │    - Max: 100bps (1%)          │  │
//! │  │    - If exceeded → HALT        │  │
//! │  ├────────────────────────────────┤  │
//! │  │ 2. Price Movement Check        │  │
//! │  │    - Max change: 10% per tick  │  │
//! │  │    - If exceeded → HALT        │  │
//! │  ├────────────────────────────────┤  │
//! │  │ 3. Liquidity Check             │  │
//! │  │    - Min size: 0.01 BTC        │  │
//! │  │    - If < min → SKIP TICK      │  │
//! │  ├────────────────────────────────┤  │
//! │  │ 4. Staleness Check             │  │
//! │  │    - Max age: 5s               │  │
//! │  │    - If stale → SKIP TICK      │  │
//! │  └────────────────────────────────┘  │
//! │                │                      │
//! └────────────────┼──────────────────────┘
//!                  │
//!        ┌─────────┴────────┐
//!        │                  │
//!        ▼                  ▼
//!   ┌─────────┐      ┌──────────┐
//!   │  HALT   │      │ PROCEED  │
//!   │ (State) │      │ (Trade)  │
//!   └─────────┘      └──────────┘
//! ```
//!
//! ## State Machine
//!
//! ```text
//!           NORMAL
//!              │
//!      ┌───────┼───────┐
//!      │       │       │
//!      │   Anomaly     │ Normal tick
//!      │   detected    │
//!      ▼       │       ▼
//!   HALTED ◄───┘    NORMAL
//!      │
//!      │ Reset command
//!      │ (manual)
//!      ▼
//!   NORMAL
//! ```
//!
//! ## Example Usage
//!
//! ```rust
//! let mut breaker = CircuitBreaker::new();
//!
//! match breaker.check(&market_snapshot) {
//!     BreakerState::Normal => {
//!         // Safe to trade
//!         let signal = strategy.calculate(&market_snapshot);
//!     }
//!     BreakerState::Halted(reason) => {
//!         error!("Trading halted: {}", reason);
//!         // Skip trading, alert operations
//!     }
//! }
//! ```

use crate::data::MarketSnapshot;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{error, warn};

/// Maximum spread in basis points before circuit breaker trips
/// Default: 100bps (1%) - anything wider is likely a flash crash
pub const MAX_SPREAD_BPS: u64 = 100;

/// Maximum price change between ticks (percentage)
/// Default: 10% - anything larger is likely erroneous data
pub const MAX_PRICE_CHANGE_PCT: u64 = 10;

/// Minimum bid/ask size in fixed-point (9 decimals)
/// Default: 0.01 BTC = 10_000_000
pub const MIN_LIQUIDITY: u64 = 10_000_000;

/// Maximum data age in nanoseconds (5 seconds)
/// Older data is considered stale
pub const MAX_DATA_AGE_NS: i64 = 5_000_000_000;

/// Consecutive violations before halting
/// Prevents single spurious tick from halting trading
pub const CONSECUTIVE_VIOLATIONS_THRESHOLD: u32 = 3;

/// Circuit breaker state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakerState {
    /// Normal operation - safe to trade
    Normal,
    /// Trading halted due to circuit breaker trip
    Halted(HaltReason),
}

/// Reason for circuit breaker trip
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HaltReason {
    /// Spread exceeded MAX_SPREAD_BPS
    ExcessiveSpread { spread_bps: u64, max_bps: u64 },
    /// Price changed >10% in one tick
    ExcessivePriceMove { change_pct: u64, max_pct: u64 },
    /// Insufficient liquidity on both sides
    InsufficientLiquidity { min_size: u64, actual_bid: u64, actual_ask: u64 },
    /// Market data is stale (>5s old)
    StaleData { age_ms: i64, max_age_ms: i64 },
    /// Manual halt (operator command)
    Manual,
}

impl std::fmt::Display for HaltReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HaltReason::ExcessiveSpread { spread_bps, max_bps } => {
                write!(f, "Excessive spread: {}bps (max: {}bps)", spread_bps, max_bps)
            }
            HaltReason::ExcessivePriceMove { change_pct, max_pct } => {
                write!(f, "Excessive price move: {}% (max: {}%)", change_pct, max_pct)
            }
            HaltReason::InsufficientLiquidity { min_size, actual_bid, actual_ask } => {
                write!(
                    f,
                    "Insufficient liquidity: bid={}, ask={} (min: {})",
                    actual_bid, actual_ask, min_size
                )
            }
            HaltReason::StaleData { age_ms, max_age_ms } => {
                write!(f, "Stale data: {}ms old (max: {}ms)", age_ms, max_age_ms)
            }
            HaltReason::Manual => write!(f, "Manual halt"),
        }
    }
}

/// Circuit breaker for flash crash detection
///
/// Tracks market state and trips on anomalies.
/// Once tripped, requires manual reset.
pub struct CircuitBreaker {
    /// Current state (normal or halted)
    state: BreakerState,
    /// Last mid price (for price movement detection)
    last_mid_price: Option<u64>,
    /// Last check timestamp
    last_check_ns: u64,
    /// Consecutive violations counter
    consecutive_violations: u32,
    /// Total times tripped
    total_trips: u64,
    /// Last reason for trip
    last_trip_reason: Option<HaltReason>,
}

impl CircuitBreaker {
    /// Create new circuit breaker in Normal state
    pub fn new() -> Self {
        Self {
            state: BreakerState::Normal,
            last_mid_price: None,
            last_check_ns: 0,
            consecutive_violations: 0,
            total_trips: 0,
            last_trip_reason: None,
        }
    }

    /// Check market snapshot and return current state
    ///
    /// Checks for:
    /// 1. Excessive spread (flash crash)
    /// 2. Excessive price movement (erroneous data)
    /// 3. Insufficient liquidity (thin book)
    /// 4. Stale data (>5s old)
    ///
    /// Returns BreakerState::Normal if safe to trade,
    /// BreakerState::Halted(reason) otherwise.
    pub fn check(&mut self, snapshot: &MarketSnapshot) -> BreakerState {
        // If already halted, stay halted until manual reset
        if let BreakerState::Halted(reason) = self.state {
            return BreakerState::Halted(reason);
        }

        let bid = snapshot.best_bid_price;
        let ask = snapshot.best_ask_price;

        // Basic validation
        if bid == 0 || ask == 0 || ask <= bid {
            // Invalid data, skip this tick but don't halt
            return BreakerState::Normal;
        }

        // Check 1: Spread check
        if let Some(reason) = self.check_spread(bid, ask) {
            return self.trip(reason);
        }

        // Check 2: Price movement check
        let mid = bid / 2 + ask / 2 + (bid % 2 + ask % 2) / 2;
        if let Some(reason) = self.check_price_movement(mid) {
            return self.trip(reason);
        }

        // Check 3: Liquidity check
        if let Some(reason) = self.check_liquidity(snapshot) {
            // Low liquidity: skip tick but don't halt
            warn!("{}", reason);
            return BreakerState::Normal;
        }

        // Check 4: Staleness check
        if let Some(reason) = self.check_staleness(snapshot) {
            // Stale data: skip tick but don't halt
            warn!("{}", reason);
            return BreakerState::Normal;
        }

        // All checks passed - update state and continue
        self.last_mid_price = Some(mid);
        self.last_check_ns = snapshot.exchange_timestamp_ns;
        self.consecutive_violations = 0;

        BreakerState::Normal
    }

    /// Check if spread is excessive (flash crash indicator)
    fn check_spread(&self, bid: u64, ask: u64) -> Option<HaltReason> {
        let spread = ask - bid;
        let spread_bps = (spread * 10_000) / bid;

        if spread_bps > MAX_SPREAD_BPS {
            Some(HaltReason::ExcessiveSpread {
                spread_bps,
                max_bps: MAX_SPREAD_BPS,
            })
        } else {
            None
        }
    }

    /// Check if price moved excessively (erroneous data indicator)
    fn check_price_movement(&self, current_mid: u64) -> Option<HaltReason> {
        if let Some(last_mid) = self.last_mid_price {
            if last_mid == 0 {
                return None;
            }

            let change = if current_mid > last_mid {
                current_mid - last_mid
            } else {
                last_mid - current_mid
            };

            let change_pct = (change * 100) / last_mid;

            if change_pct > MAX_PRICE_CHANGE_PCT {
                return Some(HaltReason::ExcessivePriceMove {
                    change_pct,
                    max_pct: MAX_PRICE_CHANGE_PCT,
                });
            }
        }

        None
    }

    /// Check if liquidity is sufficient
    fn check_liquidity(&self, snapshot: &MarketSnapshot) -> Option<HaltReason> {
        let bid_size = snapshot.best_bid_size;
        let ask_size = snapshot.best_ask_size;

        if bid_size < MIN_LIQUIDITY || ask_size < MIN_LIQUIDITY {
            Some(HaltReason::InsufficientLiquidity {
                min_size: MIN_LIQUIDITY,
                actual_bid: bid_size,
                actual_ask: ask_size,
            })
        } else {
            None
        }
    }

    /// Check if data is stale
    fn check_staleness(&self, snapshot: &MarketSnapshot) -> Option<HaltReason> {
        let now_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;

        if now_ns < snapshot.exchange_timestamp_ns {
            // Clock skew: future timestamp
            return None;
        }

        let age_ns = now_ns - snapshot.exchange_timestamp_ns;

        if age_ns > MAX_DATA_AGE_NS as u64 {
            Some(HaltReason::StaleData {
                age_ms: (age_ns / 1_000_000) as i64,
                max_age_ms: MAX_DATA_AGE_NS / 1_000_000,
            })
        } else {
            None
        }
    }

    /// Trip the circuit breaker
    fn trip(&mut self, reason: HaltReason) -> BreakerState {
        self.consecutive_violations += 1;

        if self.consecutive_violations >= CONSECUTIVE_VIOLATIONS_THRESHOLD {
            error!("CIRCUIT BREAKER TRIPPED: {}", reason);
            self.state = BreakerState::Halted(reason);
            self.last_trip_reason = Some(reason);
            self.total_trips += 1;
            BreakerState::Halted(reason)
        } else {
            // Not enough consecutive violations yet, just warn
            warn!(
                "Circuit breaker warning ({}/{}): {}",
                self.consecutive_violations, CONSECUTIVE_VIOLATIONS_THRESHOLD, reason
            );
            BreakerState::Normal
        }
    }

    /// Manually reset circuit breaker
    ///
    /// Should only be called after investigating and resolving the issue.
    pub fn reset(&mut self) {
        if let BreakerState::Halted(reason) = self.state {
            warn!("Circuit breaker reset (was: {})", reason);
            self.state = BreakerState::Normal;
            self.consecutive_violations = 0;
        }
    }

    /// Manually halt trading
    pub fn manual_halt(&mut self) {
        error!("Circuit breaker manually halted");
        self.state = BreakerState::Halted(HaltReason::Manual);
        self.total_trips += 1;
    }

    /// Get current state
    pub fn state(&self) -> BreakerState {
        self.state
    }

    /// Get total times tripped
    pub fn total_trips(&self) -> u64 {
        self.total_trips
    }

    /// Get last trip reason
    pub fn last_trip_reason(&self) -> Option<HaltReason> {
        self.last_trip_reason
    }

    /// Check if currently halted
    pub fn is_halted(&self) -> bool {
        matches!(self.state, BreakerState::Halted(_))
    }
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_normal_snapshot() -> MarketSnapshot {
        MarketSnapshot {
            market_id: 1,
            sequence: 1,
            exchange_timestamp_ns: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos() as i64,
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
        }
    }

    #[test]
    fn test_normal_operation() {
        let mut breaker = CircuitBreaker::new();
        let snapshot = create_normal_snapshot();

        let state = breaker.check(&snapshot);
        assert_eq!(state, BreakerState::Normal);
        assert!(!breaker.is_halted());
        assert_eq!(breaker.total_trips(), 0);
    }

    #[test]
    fn test_excessive_spread_trips_breaker() {
        let mut breaker = CircuitBreaker::new();

        // Flash crash: spread goes from 2bps to 500bps
        let mut snapshot = create_normal_snapshot();
        snapshot.best_ask_price = 52_500_000_000_000;  // 5% spread (500bps)

        // First 2 violations: warnings only
        breaker.check(&snapshot);
        assert!(!breaker.is_halted());
        breaker.check(&snapshot);
        assert!(!breaker.is_halted());

        // 3rd violation: trips
        let state = breaker.check(&snapshot);
        assert!(breaker.is_halted());
        assert_eq!(breaker.total_trips(), 1);

        if let BreakerState::Halted(HaltReason::ExcessiveSpread { spread_bps, .. }) = state {
            assert_eq!(spread_bps, 500);
        } else {
            panic!("Expected ExcessiveSpread");
        }
    }

    #[test]
    fn test_excessive_price_move_trips_breaker() {
        let mut breaker = CircuitBreaker::new();

        // Normal tick
        let snapshot1 = create_normal_snapshot();
        breaker.check(&snapshot1);

        // Price jumps 20% (flash crash)
        let mut snapshot2 = create_normal_snapshot();
        snapshot2.best_bid_price = 60_000_000_000_000;  // +20%
        snapshot2.best_ask_price = 60_010_000_000_000;

        // First 2 violations: warnings
        breaker.check(&snapshot2);
        breaker.check(&snapshot2);

        // 3rd violation: trips
        let state = breaker.check(&snapshot2);
        assert!(breaker.is_halted());

        if let BreakerState::Halted(HaltReason::ExcessivePriceMove { change_pct, .. }) = state {
            assert_eq!(change_pct, 19); // (60000-50005)/50005*100 ≈ 19%
        } else {
            panic!("Expected ExcessivePriceMove");
        }
    }

    #[test]
    fn test_insufficient_liquidity_skips_tick() {
        let mut breaker = CircuitBreaker::new();

        let mut snapshot = create_normal_snapshot();
        snapshot.best_bid_size = 1_000_000;  // 0.001 BTC (< MIN_LIQUIDITY)
        snapshot.best_ask_size = 1_000_000;

        // Low liquidity should skip tick but not halt
        let state = breaker.check(&snapshot);
        assert_eq!(state, BreakerState::Normal);
        assert!(!breaker.is_halted());
    }

    #[test]
    fn test_stale_data_skips_tick() {
        let mut breaker = CircuitBreaker::new();

        let mut snapshot = create_normal_snapshot();
        snapshot.exchange_timestamp_ns = 0;  // Very old

        // Stale data should skip tick but not halt
        let state = breaker.check(&snapshot);
        assert_eq!(state, BreakerState::Normal);
        assert!(!breaker.is_halted());
    }

    #[test]
    fn test_manual_halt() {
        let mut breaker = CircuitBreaker::new();

        breaker.manual_halt();
        assert!(breaker.is_halted());

        let snapshot = create_normal_snapshot();
        let state = breaker.check(&snapshot);
        assert!(matches!(state, BreakerState::Halted(HaltReason::Manual)));
    }

    #[test]
    fn test_reset() {
        let mut breaker = CircuitBreaker::new();

        breaker.manual_halt();
        assert!(breaker.is_halted());

        breaker.reset();
        assert!(!breaker.is_halted());

        let snapshot = create_normal_snapshot();
        let state = breaker.check(&snapshot);
        assert_eq!(state, BreakerState::Normal);
    }

    #[test]
    fn test_consecutive_violations_threshold() {
        let mut breaker = CircuitBreaker::new();

        let mut snapshot = create_normal_snapshot();
        snapshot.best_ask_price = 52_500_000_000_000;  // Wide spread

        // First violation: warning
        breaker.check(&snapshot);
        assert!(!breaker.is_halted());
        assert_eq!(breaker.consecutive_violations, 1);

        // Second violation: warning
        breaker.check(&snapshot);
        assert!(!breaker.is_halted());
        assert_eq!(breaker.consecutive_violations, 2);

        // Third violation: trips
        breaker.check(&snapshot);
        assert!(breaker.is_halted());
        assert_eq!(breaker.total_trips(), 1);
    }

    #[test]
    fn test_violations_reset_on_normal_tick() {
        let mut breaker = CircuitBreaker::new();

        // 2 violations
        let mut bad_snapshot = create_normal_snapshot();
        bad_snapshot.best_ask_price = 52_500_000_000_000;
        breaker.check(&bad_snapshot);
        breaker.check(&bad_snapshot);
        assert_eq!(breaker.consecutive_violations, 2);

        // Normal tick resets counter
        let good_snapshot = create_normal_snapshot();
        breaker.check(&good_snapshot);
        assert_eq!(breaker.consecutive_violations, 0);
        assert!(!breaker.is_halted());
    }

    #[test]
    fn test_halt_reason_display() {
        let reason1 = HaltReason::ExcessiveSpread {
            spread_bps: 500,
            max_bps: 100,
        };
        assert!(format!("{}", reason1).contains("500bps"));

        let reason2 = HaltReason::ExcessivePriceMove {
            change_pct: 20,
            max_pct: 10,
        };
        assert!(format!("{}", reason2).contains("20%"));

        let reason3 = HaltReason::Manual;
        assert_eq!(format!("{}", reason3), "Manual halt");
    }
}
