//! Bridge between legacy Order type and order state machine
//!
//! This module provides conversion utilities between:
//! - `execution::types::Order` (legacy, Decimal-based, mutable status)
//! - `core::order_fsm::OrderState` (new, u64 fixed-point, typestate FSM)
//!
//! This allows executors to use the type-safe state machine internally while
//! maintaining backwards compatibility with the existing Executor trait.

use crate::core::order_fsm::{
    FillResult, FillResultOrError, OrderData, OrderPending, OrderState, PartialFillResultOrError,
};
use crate::core::{OrderId as CoreOrderId, OrderStatus, OrderType, Side};
use crate::execution::types::{Order as LegacyOrder, OrderId as LegacyOrderId};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;

/// Convert Decimal to u64 fixed-point (9 decimals)
#[inline]
fn decimal_to_u64(value: Decimal) -> u64 {
    (value * Decimal::from(1_000_000_000)).to_u64().unwrap_or(0)
}

/// Convert u64 fixed-point to Decimal
#[inline]
fn u64_to_decimal(value: u64) -> Decimal {
    Decimal::from(value) / Decimal::from(1_000_000_000)
}

/// Convert legacy OrderId (string) to core OrderId (u128)
///
/// # Errors
///
/// Returns error if:
/// - Hex string is invalid
/// - Parsed ID is zero (reserved/invalid)
fn legacy_to_core_order_id(legacy_id: &LegacyOrderId) -> Result<CoreOrderId, String> {
    // Parse the hex string to u128
    // Legacy OrderId is formatted as hex string
    let hex_str = legacy_id.as_str().trim_start_matches("0x");

    let id_u128 = u128::from_str_radix(hex_str, 16)
        .map_err(|e| format!("Invalid OrderId hex string '{}': {}", hex_str, e))?;

    if id_u128 == 0 {
        return Err("OrderId cannot be zero (reserved value)".to_string());
    }

    Ok(CoreOrderId::new(id_u128))
}

/// Convert core OrderId (u128) to legacy OrderId (string)
fn core_to_legacy_order_id(core_id: &CoreOrderId) -> LegacyOrderId {
    // Format as hex string (matching legacy format)
    LegacyOrderId::from(format!("{:032x}", core_id.0))
}

/// Convert legacy Order to OrderPending (initial state)
///
/// # Errors
///
/// Returns error if OrderId conversion fails.
pub fn legacy_order_to_pending(order: &LegacyOrder) -> Result<OrderPending, String> {
    let core_id = legacy_to_core_order_id(&order.id)?;
    let price_u64 = decimal_to_u64(order.price);
    let quantity_u64 = decimal_to_u64(order.size);

    // Map legacy Side to core Side
    let side = match order.side {
        crate::execution::types::Side::Buy => Side::Buy,
        crate::execution::types::Side::Sell => Side::Sell,
    };

    // Map legacy OrderType to core OrderType
    let order_type = match order.order_type {
        crate::execution::types::OrderType::Limit => OrderType::Limit,
        crate::execution::types::OrderType::Market => OrderType::Market,
        crate::execution::types::OrderType::PostOnly => OrderType::PostOnly,
    };

    Ok(OrderPending::new_with_type(
        core_id,
        side,
        order_type,
        price_u64,
        quantity_u64,
    ))
}

/// Convert OrderState back to legacy Order
pub fn order_state_to_legacy(state: &OrderState) -> LegacyOrder {
    let (data, status) = match state {
        OrderState::Pending(o) => (o.data(), OrderStatus::Pending),
        OrderState::Open(o) => (o.data(), OrderStatus::Open),
        OrderState::PartiallyFilled(o) => (o.data(), OrderStatus::PartiallyFilled),
        OrderState::Filled(o) => (o.data(), OrderStatus::Filled),
        OrderState::Cancelled(o) => (o.data(), OrderStatus::Cancelled),
        OrderState::Rejected(o) => (o.data(), OrderStatus::Rejected),
        OrderState::Expired(o) => (o.data(), OrderStatus::Expired),
    };

    order_data_to_legacy(data, status)
}

/// Convert OrderData to legacy Order
pub fn order_data_to_legacy(data: &OrderData, status: OrderStatus) -> LegacyOrder {
    let legacy_id = core_to_legacy_order_id(&data.id);
    let price = u64_to_decimal(data.price);
    let size = u64_to_decimal(data.quantity);
    let filled_size = u64_to_decimal(data.filled_quantity);

    // Map core Side to legacy Side
    let side = match data.side {
        Side::Buy => crate::execution::types::Side::Buy,
        Side::Sell => crate::execution::types::Side::Sell,
    };

    // Map core OrderType to legacy OrderType
    let order_type = match data.order_type {
        OrderType::Limit => crate::execution::types::OrderType::Limit,
        OrderType::Market => crate::execution::types::OrderType::Market,
        OrderType::PostOnly => crate::execution::types::OrderType::PostOnly,
    };

    // Calculate average fill price (simplified - use order price for now)
    let avg_fill_price = if filled_size > Decimal::ZERO {
        Some(price)
    } else {
        None
    };

    LegacyOrder {
        id: legacy_id,
        side,
        order_type,
        price,
        size,
        time_in_force: crate::execution::types::TimeInForce::GTC, // Default
        status,
        filled_size,
        avg_fill_price,
        created_at: data.created_at,
        updated_at: data.updated_at,
    }
}

/// Wrapper that manages OrderState internally but presents legacy Order externally
pub struct OrderStateWrapper {
    state: OrderState,
}

impl OrderStateWrapper {
    /// Create from legacy order (starts in Pending state)
    ///
    /// # Errors
    ///
    /// Returns error if OrderId conversion fails.
    pub fn from_legacy(order: &LegacyOrder) -> Result<Self, String> {
        let pending = legacy_order_to_pending(order)?;
        Ok(Self {
            state: OrderState::Pending(pending),
        })
    }

    /// Get the current state
    pub fn state(&self) -> &OrderState {
        &self.state
    }

    /// Get as legacy Order
    pub fn to_legacy(&self) -> LegacyOrder {
        order_state_to_legacy(&self.state)
    }

    /// Acknowledge the order (Pending → Open)
    pub fn acknowledge(&mut self) -> Result<(), String> {
        match std::mem::replace(
            &mut self.state,
            OrderState::Pending(OrderPending::new(CoreOrderId::generate(), Side::Buy, 0, 0)),
        ) {
            OrderState::Pending(pending) => {
                self.state = OrderState::Open(pending.acknowledge());
                Ok(())
            }
            other => {
                self.state = other;
                Err("Can only acknowledge pending orders".to_string())
            }
        }
    }

    /// Apply a fill
    ///
    /// Now with strict validation! Returns error if:
    /// - Fill quantity is zero
    /// - Fill price is zero
    /// - Fill quantity exceeds remaining
    pub fn apply_fill(
        &mut self,
        fill_quantity_u64: u64,
        fill_price_u64: u64,
    ) -> Result<(), String> {
        match std::mem::replace(
            &mut self.state,
            OrderState::Pending(OrderPending::new(CoreOrderId::generate(), Side::Buy, 0, 0)),
        ) {
            OrderState::Open(open) => {
                match open.fill(fill_quantity_u64, fill_price_u64) {
                    FillResultOrError::Ok(FillResult::Filled(filled)) => {
                        self.state = OrderState::Filled(filled);
                        Ok(())
                    }
                    FillResultOrError::Ok(FillResult::PartiallyFilled(partial)) => {
                        self.state = OrderState::PartiallyFilled(partial);
                        Ok(())
                    }
                    FillResultOrError::Error(err, order) => {
                        // Validation failed - restore order unchanged
                        self.state = OrderState::Open(order);
                        Err(format!("Fill validation failed: {}", err))
                    }
                }
            }
            OrderState::PartiallyFilled(partial) => {
                match partial.fill(fill_quantity_u64, fill_price_u64) {
                    PartialFillResultOrError::Ok(FillResult::Filled(filled)) => {
                        self.state = OrderState::Filled(filled);
                        Ok(())
                    }
                    PartialFillResultOrError::Ok(FillResult::PartiallyFilled(partial)) => {
                        self.state = OrderState::PartiallyFilled(partial);
                        Ok(())
                    }
                    PartialFillResultOrError::Error(err, order) => {
                        // Validation failed - restore order unchanged
                        self.state = OrderState::PartiallyFilled(order);
                        Err(format!("Fill validation failed: {}", err))
                    }
                }
            }
            other => {
                self.state = other;
                Err("Can only fill Open or PartiallyFilled orders".to_string())
            }
        }
    }

    /// Cancel the order
    pub fn cancel(&mut self) -> Result<(), String> {
        match std::mem::replace(
            &mut self.state,
            OrderState::Pending(OrderPending::new(CoreOrderId::generate(), Side::Buy, 0, 0)),
        ) {
            OrderState::Open(open) => {
                self.state = OrderState::Cancelled(open.cancel());
                Ok(())
            }
            OrderState::PartiallyFilled(partial) => {
                self.state = OrderState::Cancelled(partial.cancel());
                Ok(())
            }
            other => {
                self.state = other;
                Err("Can only cancel Open or PartiallyFilled orders".to_string())
            }
        }
    }

    /// Reject the order (Pending → Rejected)
    pub fn reject(&mut self, reason: String) -> Result<(), String> {
        match std::mem::replace(
            &mut self.state,
            OrderState::Pending(OrderPending::new(CoreOrderId::generate(), Side::Buy, 0, 0)),
        ) {
            OrderState::Pending(pending) => {
                self.state = OrderState::Rejected(pending.reject(reason));
                Ok(())
            }
            other => {
                self.state = other;
                Err("Can only reject pending orders".to_string())
            }
        }
    }

    /// Check if order is active
    pub fn is_active(&self) -> bool {
        self.state.is_active()
    }

    /// Check if order is terminal
    pub fn is_terminal(&self) -> bool {
        self.state.is_terminal()
    }

    /// Get order ID
    pub fn id(&self) -> &CoreOrderId {
        self.state.id()
    }

    /// Get current status
    pub fn status(&self) -> OrderStatus {
        self.state.status()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn create_test_legacy_order() -> LegacyOrder {
        LegacyOrder::limit(
            crate::execution::types::Side::Buy,
            dec!(50000), // $50,000
            dec!(1.0),   // 1.0 BTC
        )
    }

    #[test]
    fn test_legacy_to_pending_conversion() {
        let legacy = create_test_legacy_order();
        let pending = legacy_order_to_pending(&legacy).unwrap();

        assert_eq!(pending.status(), OrderStatus::Pending);
        assert_eq!(pending.data().quantity, 1_000_000_000); // 1.0 in fixed-point
        assert_eq!(pending.data().price, 50_000_000_000_000); // $50,000 in fixed-point
    }

    #[test]
    fn test_order_state_to_legacy_conversion() {
        let legacy = create_test_legacy_order();
        let pending = legacy_order_to_pending(&legacy).unwrap();
        let state = OrderState::Pending(pending);

        let converted = order_state_to_legacy(&state);

        assert_eq!(converted.status, OrderStatus::Pending);
        assert_eq!(converted.size, dec!(1.0));
        assert_eq!(converted.price, dec!(50000));
    }

    #[test]
    fn test_wrapper_acknowledge() {
        let legacy = create_test_legacy_order();
        let mut wrapper = OrderStateWrapper::from_legacy(&legacy).unwrap();

        assert_eq!(wrapper.status(), OrderStatus::Pending);

        wrapper.acknowledge().unwrap();
        assert_eq!(wrapper.status(), OrderStatus::Open);

        // Cannot acknowledge again
        assert!(wrapper.acknowledge().is_err());
    }

    #[test]
    fn test_wrapper_fill_sequence() {
        let legacy = create_test_legacy_order();
        let mut wrapper = OrderStateWrapper::from_legacy(&legacy).unwrap();

        wrapper.acknowledge().unwrap();

        // Partial fill: 0.5 BTC
        wrapper.apply_fill(500_000_000, 50_000_000_000_000).unwrap();
        assert_eq!(wrapper.status(), OrderStatus::PartiallyFilled);

        // Complete fill: 0.5 BTC more
        wrapper.apply_fill(500_000_000, 50_000_000_000_000).unwrap();
        assert_eq!(wrapper.status(), OrderStatus::Filled);

        // Cannot fill again
        assert!(wrapper.apply_fill(100_000_000, 50_000_000_000_000).is_err());
    }

    #[test]
    fn test_wrapper_cancel() {
        let legacy = create_test_legacy_order();
        let mut wrapper = OrderStateWrapper::from_legacy(&legacy).unwrap();

        wrapper.acknowledge().unwrap();
        wrapper.cancel().unwrap();

        assert_eq!(wrapper.status(), OrderStatus::Cancelled);
        assert!(wrapper.is_terminal());
    }

    #[test]
    fn test_wrapper_reject() {
        let legacy = create_test_legacy_order();
        let mut wrapper = OrderStateWrapper::from_legacy(&legacy).unwrap();

        wrapper.reject("Insufficient funds".to_string()).unwrap();

        assert_eq!(wrapper.status(), OrderStatus::Rejected);
        assert!(wrapper.is_terminal());
    }

    #[test]
    fn test_round_trip_conversion() {
        let original = create_test_legacy_order();
        let pending = legacy_order_to_pending(&original).unwrap();
        let state = OrderState::Pending(pending);
        let converted = order_state_to_legacy(&state);

        // Should preserve key fields
        assert_eq!(converted.size, original.size);
        assert_eq!(converted.price, original.price);
        assert_eq!(converted.status, original.status);
    }
}
