pub mod stub;
pub mod depth;
pub mod l2_book;

// Use L2OrderBook as the primary implementation
pub use l2_book::{L2OrderBook as OrderBook, QueuePosition, DEPTH_LEVELS};

// Re-export stub utility types (still needed for our order tracking)
pub use stub::{OrderInfo, OrderSide, OurOrders};

// Re-export depth calculation functions
pub use depth::{
    calculate_vwap, calculate_imbalance, calculate_liquidity,
    mid_price, spread_bps, spread_bps_from_prices,
    calculate_vwap_u64, calculate_imbalance_i64,
};

use crate::data::MarketSnapshot;
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use tracing::{debug, warn};

/// OrderBook Manager - wraps orderbook with additional tracking
pub struct OrderBookManager {
    orderbook: OrderBook,
    our_orders: OurOrders,
    /// Market ID for this orderbook - reserved for future multi-market support
    #[allow(dead_code)]
    market_id: u64,
    update_count: u64,
}

impl OrderBookManager {
    /// Create a new OrderBook manager
    pub fn new(market_id: u64) -> Self {
        Self {
            orderbook: OrderBook::new(market_id),
            our_orders: OurOrders::new(),
            market_id,
            update_count: 0,
        }
    }

    /// Sync orderbook from Huginn snapshot
    pub fn sync_from_snapshot(&mut self, snapshot: &MarketSnapshot) {
        // Check for sequence gaps (potential data loss)
        if let Some(gap) = self.orderbook.check_sequence_gap(snapshot.sequence) {
            warn!(
                "Sequence gap detected: {} messages lost (last: {}, current: {})",
                gap, self.orderbook.last_sequence, snapshot.sequence
            );
        }

        self.orderbook.sync_from_snapshot(snapshot);
        self.update_count += 1;

        if self.update_count % 1000 == 0 {
            debug!(
                "OrderBook update #{}: bid={}, ask={}, spread={}bps, depth={}x{}",
                self.update_count,
                self.orderbook.best_bid(),
                self.orderbook.best_ask(),
                self.orderbook.spread_bps(),
                self.orderbook.bid_depth(),
                self.orderbook.ask_depth(),
            );
        }

        // Warn if orderbook is invalid
        if !self.orderbook.is_valid() {
            if self.orderbook.is_crossed() {
                warn!("OrderBook is crossed! bid={}, ask={}",
                    self.orderbook.best_bid(),
                    self.orderbook.best_ask()
                );
            } else {
                warn!("OrderBook is invalid (missing data)");
            }
        }
    }

    /// Get reference to the orderbook
    pub fn orderbook(&self) -> &OrderBook {
        &self.orderbook
    }

    /// Get reference to our orders
    pub fn our_orders(&self) -> &OurOrders {
        &self.our_orders
    }

    /// Add one of our orders
    pub fn add_our_order(
        &mut self,
        order_id: String,
        side: OrderSide,
        price: Decimal,
        size: Decimal,
    ) {
        self.our_orders.add(order_id, side, price, size);
    }

    /// Remove one of our orders
    pub fn remove_our_order(&mut self, order_id: &str) -> Option<OrderInfo> {
        self.our_orders.remove(order_id)
    }

    /// Get queue position for our order at a given price
    ///
    /// Note: L2OrderBook estimates queue position based on visible depth.
    /// Returns None if price is not in visible depth.
    pub fn queue_position(&self, is_bid: bool, price: Decimal) -> Option<QueuePosition> {
        let price_u64 = (price * Decimal::from(1_000_000_000))
            .to_u64()
            .unwrap_or(0);
        self.orderbook.estimate_queue_position(is_bid, price_u64)
    }

    /// Get orderbook imbalance (-100 to +100)
    ///
    /// Negative = more sell pressure, Positive = more buy pressure
    pub fn imbalance(&self) -> i64 {
        self.orderbook.imbalance()
    }

    /// Get VWAP for bid or ask side
    ///
    /// - `is_bid`: true for bid VWAP, false for ask VWAP
    /// - `max_levels`: number of levels to include (default: 5)
    pub fn vwap(&self, is_bid: bool, max_levels: usize) -> Option<Decimal> {
        self.orderbook.vwap_decimal(is_bid, max_levels)
    }

    /// Get mid price (as Decimal for backwards compatibility)
    pub fn mid_price(&self) -> Decimal {
        self.orderbook.mid_price_decimal()
    }

    /// Get spread in basis points
    pub fn spread_bps(&self) -> u32 {
        self.orderbook.spread_bps()
    }

    /// Get best bid (as Decimal for backwards compatibility)
    pub fn best_bid(&self) -> Decimal {
        self.orderbook.best_bid()
    }

    /// Get best ask (as Decimal for backwards compatibility)
    pub fn best_ask(&self) -> Decimal {
        self.orderbook.best_ask()
    }

    /// Get total liquidity on a side up to N levels
    pub fn total_liquidity(&self, is_bid: bool, max_levels: usize) -> Decimal {
        let liq_u64 = self.orderbook.total_liquidity(is_bid, max_levels);
        Decimal::from(liq_u64) / Decimal::from(1_000_000_000)
    }

    /// Get liquidity within N basis points of mid
    pub fn liquidity_within_bps(&self, bps: u32) -> (Decimal, Decimal) {
        let (bid, ask) = self.orderbook.liquidity_within_bps(bps);
        (
            Decimal::from(bid) / Decimal::from(1_000_000_000),
            Decimal::from(ask) / Decimal::from(1_000_000_000),
        )
    }

    /// Get all bid levels
    pub fn bid_levels(&self) -> Vec<(Decimal, Decimal)> {
        self.orderbook
            .bid_levels()
            .into_iter()
            .map(|(p, s)| {
                (
                    Decimal::from(p) / Decimal::from(1_000_000_000),
                    Decimal::from(s) / Decimal::from(1_000_000_000),
                )
            })
            .collect()
    }

    /// Get all ask levels
    pub fn ask_levels(&self) -> Vec<(Decimal, Decimal)> {
        self.orderbook
            .ask_levels()
            .into_iter()
            .map(|(p, s)| {
                (
                    Decimal::from(p) / Decimal::from(1_000_000_000),
                    Decimal::from(s) / Decimal::from(1_000_000_000),
                )
            })
            .collect()
    }

    /// Check if orderbook is valid
    pub fn is_valid(&self) -> bool {
        self.orderbook.is_valid()
    }

    /// Get update count
    pub fn update_count(&self) -> u64 {
        self.update_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::conversions::f64_to_u64;

    fn create_test_snapshot(bid: f64, ask: f64) -> MarketSnapshot {
        MarketSnapshot {
            market_id: 1,
            sequence: 1,
            exchange_timestamp_ns: 0,
            local_recv_ns: 0,
            local_publish_ns: 0,
            best_bid_price: f64_to_u64(bid),
            best_bid_size: f64_to_u64(1.0),
            best_ask_price: f64_to_u64(ask),
            best_ask_size: f64_to_u64(1.0),
            dex_type: 1,
            ..Default::default()
        }
    }

    #[test]
    fn test_orderbook_manager_creation() {
        let manager = OrderBookManager::new(1);
        assert_eq!(manager.market_id, 1);
        assert_eq!(manager.update_count(), 0);
    }

    #[test]
    fn test_orderbook_manager_sync() {
        let mut manager = OrderBookManager::new(1);
        let snapshot = create_test_snapshot(50000.0, 50005.0);

        manager.sync_from_snapshot(&snapshot);

        assert_eq!(manager.update_count(), 1);
        assert!(manager.is_valid());
        assert!(manager.mid_price() > Decimal::ZERO);
    }

    #[test]
    fn test_our_orders_tracking() {
        use rust_decimal_macros::dec;

        let mut manager = OrderBookManager::new(1);

        manager.add_our_order(
            "order1".to_string(),
            OrderSide::Bid,
            dec!(50000),
            dec!(0.5),
        );

        assert_eq!(manager.our_orders().count(), 1);

        let removed = manager.remove_our_order("order1");
        assert!(removed.is_some());
        assert_eq!(manager.our_orders().count(), 0);
    }
}
