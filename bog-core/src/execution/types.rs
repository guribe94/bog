use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::fmt;

// Re-export OrderStatus from core (single source of truth)
pub use crate::core::OrderStatus;

/// Unique identifier for an order
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OrderId(String);

impl OrderId {
    pub fn new(id: String) -> Self {
        Self(id)
    }

    pub fn new_random() -> Self {
        // Legacy method - use crate::core::OrderId instead
        use rand::Rng;
        let id = rand::thread_rng().gen::<u128>();
        Self(format!("{:032x}", id))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for OrderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for OrderId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for OrderId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// Order side (Buy or Sell)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Side {
    Buy,
    Sell,
}

impl fmt::Display for Side {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Side::Buy => write!(f, "BUY"),
            Side::Sell => write!(f, "SELL"),
        }
    }
}

/// Order type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderType {
    /// Limit order with specified price
    Limit,
    /// Market order (execute at best available price)
    Market,
    /// Post-only limit order (reject if would take liquidity)
    PostOnly,
}

/// Time-in-force
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimeInForce {
    /// Good-til-cancelled (default)
    GTC,
    /// Immediate-or-cancel
    IOC,
    /// Fill-or-kill
    FOK,
}

// Note: OrderStatus is re-exported from crate::core::OrderStatus (line 6)
// This ensures a single source of truth for order state representation.

/// An order to be submitted to the exchange
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    pub id: OrderId,
    pub side: Side,
    pub order_type: OrderType,
    pub price: Decimal,
    pub size: Decimal,
    pub time_in_force: TimeInForce,
    pub status: OrderStatus,
    pub filled_size: Decimal,
    pub avg_fill_price: Option<Decimal>,
    pub created_at: std::time::SystemTime,
    pub updated_at: std::time::SystemTime,
}

impl Order {
    /// Create a new limit order
    pub fn limit(side: Side, price: Decimal, size: Decimal) -> Self {
        let now = std::time::SystemTime::now();
        Self {
            id: OrderId::new_random(),
            side,
            order_type: OrderType::Limit,
            price,
            size,
            time_in_force: TimeInForce::GTC,
            status: OrderStatus::Pending,
            filled_size: Decimal::ZERO,
            avg_fill_price: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Create a new post-only limit order
    pub fn post_only(side: Side, price: Decimal, size: Decimal) -> Self {
        let mut order = Self::limit(side, price, size);
        order.order_type = OrderType::PostOnly;
        order
    }

    /// Create a new market order
    pub fn market(side: Side, size: Decimal) -> Self {
        let now = std::time::SystemTime::now();
        Self {
            id: OrderId::new_random(),
            side,
            order_type: OrderType::Market,
            price: Decimal::ZERO, // Market orders don't have a price
            size,
            time_in_force: TimeInForce::IOC,
            status: OrderStatus::Pending,
            filled_size: Decimal::ZERO,
            avg_fill_price: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Get remaining unfilled size
    pub fn remaining_size(&self) -> Decimal {
        self.size - self.filled_size
    }

    /// Check if order is fully filled
    pub fn is_filled(&self) -> bool {
        self.filled_size >= self.size
    }

    /// Check if order is active (can still be filled)
    pub fn is_active(&self) -> bool {
        matches!(
            self.status,
            OrderStatus::Pending | OrderStatus::Open | OrderStatus::PartiallyFilled
        )
    }
}

/// A fill (trade execution)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fill {
    pub order_id: OrderId,
    pub side: Side,
    pub price: Decimal,
    pub size: Decimal,
    pub timestamp: std::time::SystemTime,
    pub fee: Option<Decimal>,
    pub fee_currency: Option<String>,
}

impl Fill {
    /// Create a fill without fee information (backwards compatibility)
    pub fn new(order_id: OrderId, side: Side, price: Decimal, size: Decimal) -> Self {
        Self::new_with_fee(order_id, side, price, size, None)
    }

    /// Create a fill with fee information (for realistic paper trading)
    ///
    /// # Arguments
    /// - `fee`: Fee amount in same currency as price (USD for BTC/USD)
    ///
    /// # Example
    /// ```ignore
    /// let fee = Decimal::from(10); // $10 fee
    /// let fill = Fill::new_with_fee(id, Side::Buy, price, size, Some(fee));
    /// ```
    pub fn new_with_fee(
        order_id: OrderId,
        side: Side,
        price: Decimal,
        size: Decimal,
        fee: Option<Decimal>,
    ) -> Self {
        Self {
            order_id,
            side,
            price,
            size,
            timestamp: std::time::SystemTime::now(),
            fee,
            fee_currency: if fee.is_some() {
                Some("USD".to_string())
            } else {
                None
            },
        }
    }

    /// Calculate notional value (price * size)
    pub fn notional(&self) -> Decimal {
        self.price * self.size
    }

    /// Get position change (positive for buys, negative for sells)
    pub fn position_change(&self) -> Decimal {
        match self.side {
            Side::Buy => self.size,
            Side::Sell => -self.size,
        }
    }

    /// Get cash flow (negative for buys, positive for sells)
    ///
    /// Includes fees:
    /// - Buy: -(price * size + fee) - You pay price plus fee
    /// - Sell: +(price * size - fee) - You receive price minus fee
    pub fn cash_flow(&self) -> Decimal {
        let notional = self.notional();
        let fee_amount = self.fee.unwrap_or(Decimal::ZERO);

        match self.side {
            Side::Buy => -(notional + fee_amount),
            Side::Sell => notional - fee_amount,
        }
    }
}

/// Execution mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionMode {
    /// Live trading with real exchange
    Live,
    /// Simulated execution (paper trading or backtest)
    Simulated,
}

impl fmt::Display for ExecutionMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExecutionMode::Live => write!(f, "LIVE"),
            ExecutionMode::Simulated => write!(f, "SIMULATED"),
        }
    }
}

impl ExecutionMode {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "live" => Some(Self::Live),
            "simulated" | "paper" | "backtest" => Some(Self::Simulated),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_order_creation() {
        let order = Order::limit(Side::Buy, dec!(50000), dec!(0.1));

        assert_eq!(order.side, Side::Buy);
        assert_eq!(order.order_type, OrderType::Limit);
        assert_eq!(order.price, dec!(50000));
        assert_eq!(order.size, dec!(0.1));
        assert_eq!(order.status, OrderStatus::Pending);
        assert_eq!(order.filled_size, Decimal::ZERO);
        assert!(!order.is_filled());
        assert!(order.is_active());
    }

    #[test]
    fn test_order_remaining_size() {
        let mut order = Order::limit(Side::Buy, dec!(50000), dec!(1.0));

        assert_eq!(order.remaining_size(), dec!(1.0));

        order.filled_size = dec!(0.3);
        assert_eq!(order.remaining_size(), dec!(0.7));

        order.filled_size = dec!(1.0);
        assert_eq!(order.remaining_size(), Decimal::ZERO);
        assert!(order.is_filled());
    }

    #[test]
    fn test_market_order() {
        let order = Order::market(Side::Sell, dec!(0.5));

        assert_eq!(order.order_type, OrderType::Market);
        assert_eq!(order.price, Decimal::ZERO);
        assert_eq!(order.time_in_force, TimeInForce::IOC);
    }

    #[test]
    fn test_post_only_order() {
        let order = Order::post_only(Side::Buy, dec!(50000), dec!(0.1));

        assert_eq!(order.order_type, OrderType::PostOnly);
    }

    #[test]
    fn test_fill_calculations() {
        let fill = Fill::new(
            OrderId::new_random(),
            Side::Buy,
            dec!(50000),
            dec!(0.1),
        );

        assert_eq!(fill.notional(), dec!(5000)); // 50000 * 0.1
        assert_eq!(fill.position_change(), dec!(0.1)); // Buy increases position
        assert_eq!(fill.cash_flow(), dec!(-5000)); // Buy decreases cash
    }

    #[test]
    fn test_fill_sell() {
        let fill = Fill::new(
            OrderId::new_random(),
            Side::Sell,
            dec!(50000),
            dec!(0.1),
        );

        assert_eq!(fill.position_change(), dec!(-0.1)); // Sell decreases position
        assert_eq!(fill.cash_flow(), dec!(5000)); // Sell increases cash
    }

    #[test]
    fn test_execution_mode_from_str() {
        assert_eq!(ExecutionMode::from_str("live"), Some(ExecutionMode::Live));
        assert_eq!(
            ExecutionMode::from_str("simulated"),
            Some(ExecutionMode::Simulated)
        );
        assert_eq!(
            ExecutionMode::from_str("paper"),
            Some(ExecutionMode::Simulated)
        );
        assert_eq!(ExecutionMode::from_str("invalid"), None);
    }

    #[test]
    fn test_order_status_active() {
        let mut order = Order::limit(Side::Buy, dec!(50000), dec!(0.1));

        order.status = OrderStatus::Pending;
        assert!(order.is_active());

        order.status = OrderStatus::Open;
        assert!(order.is_active());

        order.status = OrderStatus::PartiallyFilled;
        assert!(order.is_active());

        order.status = OrderStatus::Filled;
        assert!(!order.is_active());

        order.status = OrderStatus::Cancelled;
        assert!(!order.is_active());
    }

    #[test]
    fn test_order_id() {
        let id1 = OrderId::new("test123".to_string());
        let id2 = OrderId::from("test123");

        assert_eq!(id1.as_str(), "test123");
        assert_eq!(id1, id2);
        assert_eq!(format!("{}", id1), "test123");
    }
}
