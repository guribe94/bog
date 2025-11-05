pub mod stub;

// Re-export stub types for now (Phase 1)
// In Phase 7, we'll integrate real OrderBook-rs here
pub use stub::{OrderInfo, OrderSide, OurOrders, StubOrderBook as OrderBook};

use crate::data::MarketSnapshot;
use rust_decimal::Decimal;
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
        self.orderbook.sync_from_snapshot(snapshot);
        self.update_count += 1;

        if self.update_count % 1000 == 0 {
            debug!(
                "OrderBook update #{}: bid={}, ask={}, spread={:.2}bps",
                self.update_count,
                self.orderbook.best_bid(),
                self.orderbook.best_ask(),
                self.orderbook.spread_bps()
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

    /// Get queue position for our order
    pub fn queue_position(&self, order_id: &str) -> Option<usize> {
        self.orderbook.queue_position(order_id)
    }

    /// Get orderbook imbalance
    pub fn imbalance(&self) -> f64 {
        self.orderbook.imbalance()
    }

    /// Get VWAP for a given depth
    pub fn vwap(&self, depth: Decimal) -> Option<Decimal> {
        self.orderbook.vwap(depth)
    }

    /// Get mid price
    pub fn mid_price(&self) -> Decimal {
        self.orderbook.mid_price()
    }

    /// Get spread in basis points
    pub fn spread_bps(&self) -> f64 {
        self.orderbook.spread_bps()
    }

    /// Get best bid
    pub fn best_bid(&self) -> Decimal {
        self.orderbook.best_bid()
    }

    /// Get best ask
    pub fn best_ask(&self) -> Decimal {
        self.orderbook.best_ask()
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
            _padding: [0; 7],
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
