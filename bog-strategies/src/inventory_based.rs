//! Inventory-Based Market Making Strategy - Zero-Sized Type
//!
//! Based on Avellaneda-Stoikov model with inventory risk management.
//! This is a zero-overhead implementation using:
//! - Zero-sized type (no memory overhead)
//! - Const parameters from Cargo features
//! - u64 fixed-point arithmetic
//!
//! Target: <100ns signal generation
//!
//! TODO: Full implementation in Phase 4
//! For now, this is a stub that demonstrates the ZST pattern.

use bog_core::core::{Position, Signal};
use bog_core::engine::Strategy;
use bog_core::orderbook::L2OrderBook;
use bog_core::config::constants::TICK_SIZE;

// ===== CONFIGURATION FROM CARGO FEATURES =====

/// Target inventory (fixed-point, 9 decimals)
/// Default: 0 (neutral)
#[cfg(not(any(feature = "target-inventory-0", feature = "target-inventory-positive")))]
pub const TARGET_INVENTORY: i64 = 0;

#[cfg(feature = "target-inventory-0")]
pub const TARGET_INVENTORY: i64 = 0;

#[cfg(feature = "target-inventory-positive")]
pub const TARGET_INVENTORY: i64 = 500_000_000; // 0.5 BTC

/// Risk aversion parameter (higher = more conservative)
/// Scaled by 1000 for fixed-point
#[cfg(not(any(feature = "risk-low", feature = "risk-medium", feature = "risk-high")))]
pub const RISK_AVERSION: u32 = 100; // Medium

#[cfg(feature = "risk-low")]
pub const RISK_AVERSION: u32 = 50;

#[cfg(feature = "risk-medium")]
pub const RISK_AVERSION: u32 = 100;

#[cfg(feature = "risk-high")]
pub const RISK_AVERSION: u32 = 200;

/// Order size (fixed-point, 9 decimals)
pub const ORDER_SIZE: u64 = 100_000_000; // 0.1 BTC

/// Volatility estimate (basis points per second)
/// Used for spread calculation
pub const VOLATILITY_BPS: u32 = 5; // 0.05%/s

/// Time horizon in seconds
pub const TIME_HORIZON_SECS: u32 = 60;

/// Inventory-Based Strategy - Zero-Sized Type
///
/// Adjusts quotes based on current inventory to manage risk.
/// Wider spread when inventory is away from target.
pub struct InventoryBased;

impl InventoryBased {
    /// Calculate inventory skew
    ///
    /// Returns adjustment to mid price based on inventory position.
    /// Positive inventory -> lower quotes (encourage selling)
    /// Negative inventory -> higher quotes (encourage buying)
    #[inline(always)]
    fn calculate_inventory_skew(_current_inventory: i64) -> i64 {
        // TODO: Implement Avellaneda-Stoikov formula
        // skew = risk_aversion * (current - target) * volatility^2 * time_horizon

        // Stub: No skew for now
        0
    }

    /// Calculate optimal spread
    ///
    /// Returns spread in fixed-point (half spread from mid)
    #[inline(always)]
    fn calculate_spread(mid_price: u64) -> u64 {
        // TODO: Implement optimal spread formula
        // spread = volatility * sqrt(time_horizon) + inventory_penalty

        // Stub: Use fixed 10bps spread
        (mid_price * 10) / 20_000
    }
}

impl Strategy for InventoryBased {
    #[inline(always)]
    fn calculate(&mut self, book: &L2OrderBook, position: &Position) -> Option<Signal> {
        // Extract best bid and ask
        let bid = book.best_bid_price();
        let ask = book.best_ask_price();

        // Validate prices
        if bid == 0 || ask == 0 || ask <= bid {
            return None;
        }

        // Calculate mid price
        let mid_price = bid / 2 + ask / 2 + (bid % 2 + ask % 2) / 2;

        // Get actual inventory from Position
        let current_inventory = position.get_quantity();

        // Calculate inventory skew
        let skew = Self::calculate_inventory_skew(current_inventory);

        // Adjust mid price for inventory
        let adjusted_mid = if skew >= 0 {
            mid_price.saturating_add(skew as u64)
        } else {
            mid_price.saturating_sub((-skew) as u64)
        };

        // Calculate spread
        let half_spread = Self::calculate_spread(adjusted_mid);

        // Calculate quote prices
        let raw_bid = adjusted_mid.saturating_sub(half_spread);
        let raw_ask = adjusted_mid.saturating_add(half_spread);

        // Round to tick size
        let mut our_bid = (raw_bid / TICK_SIZE) * TICK_SIZE;
        let mut our_ask = (raw_ask / TICK_SIZE) * TICK_SIZE;
        
        // Ensure we don't cross the book or quote zero spread
        if our_ask <= our_bid {
             // Force at least 1 tick spread
             our_ask = our_bid + TICK_SIZE;
        }
        
        // Ensure symmetry around mid if possible, or widen if needed
        // If mid is not on tick, rounding might shift it.
        
        // Final safety check: ask > bid
        if our_ask <= our_bid {
            return None;
        }

        // Return signal
        Some(Signal::quote_both(our_bid, our_ask, ORDER_SIZE))
    }

    fn name(&self) -> &'static str {
        "InventoryBased"
    }

    fn reset(&mut self) {
        // No state to reset (ZST)
    }
}

// Compile-time size verification
#[cfg(test)]
const _: () = {
    assert!(
        std::mem::size_of::<InventoryBased>() == 0,
        "InventoryBased must be zero-sized"
    );
};

#[cfg(test)]
mod tests {
    use super::*;
    use bog_core::data::MarketSnapshot;

    #[test]
    fn test_inventory_based_is_zst() {
        assert_eq!(std::mem::size_of::<InventoryBased>(), 0);
    }

    #[test]
    fn test_stub_signal_generation() {
        let mut strategy = InventoryBased;

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

        let mut book = L2OrderBook::new(1);
        book.sync_from_snapshot(&snapshot);

        let position = Position::new();
        let signal = strategy.calculate(&book, &position);
        assert!(signal.is_some());
    }

    #[test]
    fn test_const_values() {
        // Verify constants are sane
        assert!(RISK_AVERSION > 0);
        assert!(ORDER_SIZE > 0);
        assert!(VOLATILITY_BPS > 0);
        assert!(TIME_HORIZON_SECS > 0);
    }

    #[test]
    fn test_zero_spread_prevention() {
        let mut strategy = InventoryBased;
        let mut book = L2OrderBook::new(1);
        
        // Set up a book with a very tight/small price
        // Mid = 1000 (very small). 10bps of 1000 = 1. Half spread = 0.5 -> 0.
        // This should produce bid=1000, ask=1000 if not handled.
        let snapshot = MarketSnapshot {
            market_id: 1,
            sequence: 1,
            exchange_timestamp_ns: 0,
            local_recv_ns: 0,
            local_publish_ns: 0,
            best_bid_price: 999,
            best_bid_size: 1_000_000,
            best_ask_price: 1001,
            best_ask_size: 1_000_000,
            bid_prices: [0; 10],
            bid_sizes: [0; 10],
            ask_prices: [0; 10],
            ask_sizes: [0; 10],
            snapshot_flags: 0,
            dex_type: 1,
            _padding: [0; 54],
        };
        book.sync_from_snapshot(&snapshot);
        
        let position = Position::new();
        // We need to unwrap because we want to assert properties of the signal
        let signal = strategy.calculate(&book, &position).unwrap();
        
        println!("Bid: {}, Ask: {}", signal.bid_price, signal.ask_price);
        
        // We expect bid < ask strictly
        assert!(signal.bid_price < signal.ask_price, "Bid {} must be less than Ask {}", signal.bid_price, signal.ask_price);
    }
}
