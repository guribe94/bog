use crate::data::MarketSnapshot;
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use std::collections::HashMap;

/// Stub OrderBook implementation for Phase 1
/// This provides a simple interface that will be replaced with real OrderBook-rs integration
#[derive(Debug, Clone)]
pub struct StubOrderBook {
    /// Market ID - reserved for future multi-market support and OrderBook-rs integration
    #[allow(dead_code)]
    market_id: u64,
    bid_price: Decimal,
    ask_price: Decimal,
    bid_size: Decimal,
    ask_size: Decimal,
    last_sequence: u64,
}

impl StubOrderBook {
    pub fn new(market_id: u64) -> Self {
        Self {
            market_id,
            bid_price: Decimal::ZERO,
            ask_price: Decimal::ZERO,
            bid_size: Decimal::ZERO,
            ask_size: Decimal::ZERO,
            last_sequence: 0,
        }
    }

    /// Update from Huginn market snapshot
    pub fn sync_from_snapshot(&mut self, snapshot: &MarketSnapshot) {
        use crate::data::conversions::u64_to_decimal;

        self.bid_price = u64_to_decimal(snapshot.best_bid_price);
        self.ask_price = u64_to_decimal(snapshot.best_ask_price);
        self.bid_size = u64_to_decimal(snapshot.best_bid_size);
        self.ask_size = u64_to_decimal(snapshot.best_ask_size);
        self.last_sequence = snapshot.sequence;
    }

    /// Get best bid price
    pub fn best_bid(&self) -> Decimal {
        self.bid_price
    }

    /// Get best ask price
    pub fn best_ask(&self) -> Decimal {
        self.ask_price
    }

    /// Get best bid size
    pub fn best_bid_size(&self) -> Decimal {
        self.bid_size
    }

    /// Get best ask size
    pub fn best_ask_size(&self) -> Decimal {
        self.ask_size
    }

    /// Calculate mid price
    pub fn mid_price(&self) -> Decimal {
        if self.bid_price > Decimal::ZERO && self.ask_price > Decimal::ZERO {
            (self.bid_price + self.ask_price) / Decimal::from(2)
        } else {
            Decimal::ZERO
        }
    }

    /// Calculate spread in basis points
    pub fn spread_bps(&self) -> f64 {
        if self.bid_price > Decimal::ZERO {
            let spread = self.ask_price - self.bid_price;
            ((spread / self.bid_price) * Decimal::from(10000))
                .to_f64()
                .unwrap_or(0.0)
        } else {
            0.0
        }
    }

    /// Calculate order book imbalance (-1.0 to 1.0)
    /// Negative = more sell pressure, Positive = more buy pressure
    /// STUB: Returns neutral (0.5) for now
    pub fn imbalance(&self) -> f64 {
        // TODO Phase 7: Calculate real imbalance from full orderbook
        // For now, use simple bid/ask size ratio
        if self.bid_size > Decimal::ZERO || self.ask_size > Decimal::ZERO {
            let total = self.bid_size + self.ask_size;
            if total > Decimal::ZERO {
                (self.bid_size / total).to_f64().unwrap_or(0.5)
            } else {
                0.5
            }
        } else {
            0.5 // Neutral
        }
    }

    /// Calculate VWAP for a given depth
    /// STUB: Returns mid price for now
    pub fn vwap(&self, _depth: Decimal) -> Option<Decimal> {
        // TODO Phase 7: Calculate real VWAP from orderbook levels
        if self.mid_price() > Decimal::ZERO {
            Some(self.mid_price())
        } else {
            None
        }
    }

    /// Get queue position for an order
    /// STUB: Always returns Some(5) for now
    pub fn queue_position(&self, _order_id: &str) -> Option<usize> {
        // TODO Phase 7: Track real queue positions
        Some(5)
    }

    /// Get total depth at a given distance from mid (in bps)
    /// STUB: Returns simple estimate for now
    pub fn depth_at_distance(&self, _distance_bps: f64) -> (Decimal, Decimal) {
        // TODO Phase 7: Calculate from real orderbook
        (self.bid_size, self.ask_size)
    }

    /// Check if orderbook is crossed (bid >= ask)
    pub fn is_crossed(&self) -> bool {
        self.bid_price >= self.ask_price && self.bid_price > Decimal::ZERO
    }

    /// Check if orderbook is locked (bid == ask)
    pub fn is_locked(&self) -> bool {
        self.bid_price == self.ask_price && self.bid_price > Decimal::ZERO
    }

    /// Check if orderbook is valid (has both sides)
    pub fn is_valid(&self) -> bool {
        self.bid_price > Decimal::ZERO
            && self.ask_price > Decimal::ZERO
            && self.bid_size > Decimal::ZERO
            && self.ask_size > Decimal::ZERO
            && !self.is_crossed()
    }
}

/// Track our own orders in the orderbook
/// STUB: Simple HashMap for now
#[derive(Debug, Clone)]
pub struct OurOrders {
    orders: HashMap<String, OrderInfo>,
}

#[derive(Debug, Clone)]
pub struct OrderInfo {
    pub side: OrderSide,
    pub price: Decimal,
    pub size: Decimal,
    pub timestamp: std::time::SystemTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderSide {
    Bid,
    Ask,
}

impl OurOrders {
    pub fn new() -> Self {
        Self {
            orders: HashMap::new(),
        }
    }

    pub fn add(&mut self, order_id: String, side: OrderSide, price: Decimal, size: Decimal) {
        self.orders.insert(
            order_id,
            OrderInfo {
                side,
                price,
                size,
                timestamp: std::time::SystemTime::now(),
            },
        );
    }

    pub fn remove(&mut self, order_id: &str) -> Option<OrderInfo> {
        self.orders.remove(order_id)
    }

    pub fn get(&self, order_id: &str) -> Option<&OrderInfo> {
        self.orders.get(order_id)
    }

    pub fn count(&self) -> usize {
        self.orders.len()
    }

    pub fn total_bid_size(&self) -> Decimal {
        self.orders
            .values()
            .filter(|o| o.side == OrderSide::Bid)
            .map(|o| o.size)
            .sum()
    }

    pub fn total_ask_size(&self) -> Decimal {
        self.orders
            .values()
            .filter(|o| o.side == OrderSide::Ask)
            .map(|o| o.size)
            .sum()
    }
}

impl Default for OurOrders {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::conversions::f64_to_u64;
    use rust_decimal_macros::dec;

    fn create_test_snapshot(bid: f64, ask: f64, bid_size: f64, ask_size: f64) -> MarketSnapshot {
        MarketSnapshot {
            market_id: 1,
            sequence: 1,
            exchange_timestamp_ns: 0,
            local_recv_ns: 0,
            local_publish_ns: 0,
            best_bid_price: f64_to_u64(bid),
            best_bid_size: f64_to_u64(bid_size),
            best_ask_price: f64_to_u64(ask),
            best_ask_size: f64_to_u64(ask_size),
            dex_type: 1,
            _padding: [0; 7],
        }
    }

    #[test]
    fn test_orderbook_sync() {
        let mut ob = StubOrderBook::new(1);
        let snapshot = create_test_snapshot(50000.0, 50005.0, 1.0, 1.5);

        ob.sync_from_snapshot(&snapshot);

        assert!(ob.best_bid() > dec!(49999.0) && ob.best_bid() < dec!(50001.0));
        assert!(ob.best_ask() > dec!(50004.0) && ob.best_ask() < dec!(50006.0));
    }

    #[test]
    fn test_mid_price() {
        let mut ob = StubOrderBook::new(1);
        let snapshot = create_test_snapshot(50000.0, 50010.0, 1.0, 1.0);

        ob.sync_from_snapshot(&snapshot);

        let mid = ob.mid_price();
        assert!(mid > dec!(50004.0) && mid < dec!(50006.0));
    }

    #[test]
    fn test_spread_bps() {
        let mut ob = StubOrderBook::new(1);
        let snapshot = create_test_snapshot(50000.0, 50005.0, 1.0, 1.0);

        ob.sync_from_snapshot(&snapshot);

        let spread = ob.spread_bps();
        // 5 / 50000 * 10000 = 1 bp
        assert!((spread - 1.0).abs() < 0.1);
    }

    #[test]
    fn test_imbalance() {
        let mut ob = StubOrderBook::new(1);
        let snapshot = create_test_snapshot(50000.0, 50005.0, 2.0, 1.0);

        ob.sync_from_snapshot(&snapshot);

        let imb = ob.imbalance();
        // Bid size = 2, Ask size = 1, so imbalance = 2/3 = 0.666...
        assert!(imb > 0.6 && imb < 0.7);
    }

    #[test]
    fn test_crossed_orderbook() {
        let mut ob = StubOrderBook::new(1);

        // Normal orderbook
        let snapshot = create_test_snapshot(50000.0, 50005.0, 1.0, 1.0);
        ob.sync_from_snapshot(&snapshot);
        assert!(!ob.is_crossed());
        assert!(!ob.is_locked());
        assert!(ob.is_valid());

        // Crossed orderbook (bid > ask) - should not happen in real markets
        let crossed = create_test_snapshot(50005.0, 50000.0, 1.0, 1.0);
        ob.sync_from_snapshot(&crossed);
        assert!(ob.is_crossed());
        assert!(!ob.is_valid());
    }

    #[test]
    fn test_our_orders() {
        let mut orders = OurOrders::new();

        orders.add("order1".to_string(), OrderSide::Bid, dec!(50000), dec!(0.5));
        orders.add("order2".to_string(), OrderSide::Ask, dec!(50005), dec!(0.3));

        assert_eq!(orders.count(), 2);
        assert_eq!(orders.total_bid_size(), dec!(0.5));
        assert_eq!(orders.total_ask_size(), dec!(0.3));

        let removed = orders.remove("order1");
        assert!(removed.is_some());
        assert_eq!(orders.count(), 1);
    }

    #[test]
    fn test_vwap_stub() {
        let mut ob = StubOrderBook::new(1);
        let snapshot = create_test_snapshot(50000.0, 50010.0, 1.0, 1.0);

        ob.sync_from_snapshot(&snapshot);

        // STUB returns mid price
        let vwap = ob.vwap(dec!(1.0));
        assert!(vwap.is_some());
        assert_eq!(vwap.unwrap(), ob.mid_price());
    }

    #[test]
    fn test_queue_position_stub() {
        let ob = StubOrderBook::new(1);

        // STUB always returns 5
        let pos = ob.queue_position("dummy_order");
        assert_eq!(pos, Some(5));
    }
}
