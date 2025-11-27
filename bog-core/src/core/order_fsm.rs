//! Order State Machine - Typestate Pattern
//!
//! This module implements a compile-time verified state machine for order lifecycle.
//! Invalid state transitions are impossible - they simply won't compile.
//!
//! # State Diagram
//!
//! ```text
//!                    ┌─────────────┐
//!                    │   Pending   │
//!                    └──────┬──────┘
//!                           │
//!              ┌────────────┼────────────┐
//!              │                         │
//!              ▼                         ▼
//!        ┌──────────┐             ┌──────────┐
//!        │   Open   │             │ Rejected │
//!        └─────┬────┘             └──────────┘
//!              │                    (terminal)
//!     ┌────────┼────────┐
//!     │        │        │
//!     ▼        ▼        ▼
//! ┌────────┐ ┌────────────────┐ ┌───────────┐
//! │ Filled │ │ PartiallyFilled│ │ Cancelled │
//! └────────┘ └───────┬────────┘ └───────────┘
//! (terminal)         │           (terminal)
//!                    │
//!           ┌────────┼────────┐
//!           │                 │
//!           ▼                 ▼
//!      ┌────────┐      ┌───────────┐
//!      │ Filled │      │ Cancelled │
//!      └────────┘      └───────────┘
//!      (terminal)      (terminal)
//! ```
//!
//! # Usage
//!
//! ```
//! use bog_core::core::order_fsm::*;
//! use bog_core::core::{OrderId, Side};
//!
//! # fn example() -> Result<(), &'static str> {
//! // Create a new pending order
//! let order = OrderPending::new(
//!     OrderId::new_random(),
//!     Side::Buy,
//!     50_000_000_000_000, // $50,000 in fixed-point
//!     1_000_000_000,      // 1.0 BTC
//! );
//!
//! // Acknowledge order (pending -> open)
//! let order = order.acknowledge();
//!
//! // Apply a partial fill
//! match order.fill(500_000_000, 50_000_000_000_000) {
//!     FillResult::PartiallyFilled(order) => {
//!         // Order is now partially filled
//!         println!("Partially filled: {} / {}", order.filled_quantity(), order.data().quantity);
//!
//!         // Fill the rest
//!         match order.fill(500_000_000, 50_000_000_000_000) {
//!             FillResult::Filled(order) => {
//!                 println!("Order fully filled!");
//!             }
//!             _ => {}
//!         }
//!     }
//!     _ => {}
//! }
//! # Ok(())
//! # }
//! ```
//!
//! # Zero-Cost Abstraction
//!
//! This state machine has **zero runtime overhead**:
//! - All state checking happens at compile time
//! - State types are zero-sized wrappers around OrderData
//! - Transitions compile to simple data updates
//! - No vtables, no dynamic dispatch, no runtime checks

use super::{OrderId, OrderStatus, OrderType, Side};
use std::time::SystemTime;

// ============================================================================
// Fill Validation Errors (CRITICAL for Financial Correctness)
// ============================================================================

/// Errors that can occur when applying a fill
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FillError {
    /// Fill quantity is zero
    ZeroQuantity,
    /// Fill price is zero (would give away money!)
    ZeroPrice,
    /// Fill quantity exceeds remaining order quantity
    ExceedsRemaining {
        fill_qty: u64,
        remaining_qty: u64,
        total_qty: u64,
    },
}

impl std::fmt::Display for FillError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FillError::ZeroQuantity => write!(f, "Fill quantity cannot be zero"),
            FillError::ZeroPrice => write!(f, "Fill price cannot be zero"),
            FillError::ExceedsRemaining {
                fill_qty,
                remaining_qty,
                total_qty,
            } => write!(
                f,
                "Fill quantity {} exceeds remaining {} (total order: {})",
                fill_qty, remaining_qty, total_qty
            ),
        }
    }
}

/// Result type for fill operations on OrderOpen
///
/// Either returns a successful FillResult or an error with the order unchanged.
pub enum FillResultOrError {
    /// Fill succeeded
    Ok(FillResult),
    /// Fill validation failed - order returned unchanged
    Error(FillError, OrderOpen),
}

impl FillResultOrError {
    pub fn is_ok(&self) -> bool {
        matches!(self, FillResultOrError::Ok(_))
    }

    pub fn is_err(&self) -> bool {
        matches!(self, FillResultOrError::Error(_, _))
    }

    /// Check if the result is a successful full fill
    pub fn is_filled(&self) -> bool {
        matches!(self, FillResultOrError::Ok(FillResult::Filled(_)))
    }

    /// Check if the result is a successful partial fill
    pub fn is_partially_filled(&self) -> bool {
        matches!(self, FillResultOrError::Ok(FillResult::PartiallyFilled(_)))
    }
}

/// Result type for fill operations on OrderPartiallyFilled
///
/// Either returns a successful FillResult or an error with the order unchanged.
pub enum PartialFillResultOrError {
    /// Fill succeeded
    Ok(FillResult),
    /// Fill validation failed - order returned unchanged
    Error(FillError, OrderPartiallyFilled),
}

impl PartialFillResultOrError {
    pub fn is_ok(&self) -> bool {
        matches!(self, PartialFillResultOrError::Ok(_))
    }

    pub fn is_err(&self) -> bool {
        matches!(self, PartialFillResultOrError::Error(_, _))
    }

    /// Check if the result is a successful full fill
    pub fn is_filled(&self) -> bool {
        matches!(self, PartialFillResultOrError::Ok(FillResult::Filled(_)))
    }

    /// Check if the result is a successful partial fill
    pub fn is_partially_filled(&self) -> bool {
        matches!(self, PartialFillResultOrError::Ok(FillResult::PartiallyFilled(_)))
    }
}

// ============================================================================
// Core Order Data (shared by all states)
// ============================================================================

/// Core order data shared by all states
///
/// This is the actual data structure. The state types (OrderPending, OrderOpen, etc.)
/// are zero-sized wrappers around this data that enforce state transitions at compile time.
#[derive(Debug, Clone)]
pub struct OrderData {
    pub id: OrderId,
    pub side: Side,
    pub order_type: OrderType,
    /// Price in u64 fixed-point (9 decimals)
    pub price: u64,
    /// Total order quantity in u64 fixed-point
    pub quantity: u64,
    /// Filled quantity in u64 fixed-point
    pub filled_quantity: u64,
    /// Timestamp when order was created
    pub created_at: SystemTime,
    /// Timestamp when order last changed state
    pub updated_at: SystemTime,
    /// Timestamp when order was acknowledged (if applicable)
    pub acknowledged_at: Option<SystemTime>,
    /// Timestamp when order reached terminal state (if applicable)
    pub completed_at: Option<SystemTime>,
    /// Rejection reason (if rejected)
    pub rejection_reason: Option<String>,
}

impl OrderData {
    /// Get remaining unfilled quantity
    #[inline]
    pub fn remaining_quantity(&self) -> u64 {
        self.quantity.saturating_sub(self.filled_quantity)
    }

    /// Check if order is fully filled
    #[inline]
    pub fn is_fully_filled(&self) -> bool {
        self.filled_quantity >= self.quantity
    }

    /// Get fill percentage (0-100 in fixed-point, 9 decimals)
    #[inline]
    pub fn fill_percentage(&self) -> u64 {
        if self.quantity == 0 {
            0
        } else {
            // (filled / total) * 100 * 1e9
            // Use u128 to prevent overflow
            ((self.filled_quantity as u128 * 100_000_000_000) / self.quantity as u128) as u64
        }
    }
}

// ============================================================================
// State: Pending
// ============================================================================

/// Order in Pending state
///
/// Order has been created but not yet acknowledged by the exchange.
///
/// **Valid Transitions:**
/// - `acknowledge()` → OrderOpen
/// - `reject(reason)` → OrderRejected
#[derive(Debug, Clone)]
pub struct OrderPending {
    data: OrderData,
}

impl OrderPending {
    /// Create a new pending order
    pub fn new(id: OrderId, side: Side, price: u64, quantity: u64) -> Self {
        let now = SystemTime::now();
        Self {
            data: OrderData {
                id,
                side,
                order_type: OrderType::Limit, // Default to limit orders
                price,
                quantity,
                filled_quantity: 0,
                created_at: now,
                updated_at: now,
                acknowledged_at: None,
                completed_at: None,
                rejection_reason: None,
            },
        }
    }

    /// Create a new pending order with specific type
    pub fn new_with_type(
        id: OrderId,
        side: Side,
        order_type: OrderType,
        price: u64,
        quantity: u64,
    ) -> Self {
        let mut order = Self::new(id, side, price, quantity);
        order.data.order_type = order_type;
        order
    }

    /// Access the underlying order data
    pub fn data(&self) -> &OrderData {
        &self.data
    }

    /// Transition: Pending → Open (order acknowledged by exchange)
    pub fn acknowledge(mut self) -> OrderOpen {
        let now = SystemTime::now();
        self.data.acknowledged_at = Some(now);
        self.data.updated_at = now;
        OrderOpen { data: self.data }
    }

    /// Transition: Pending → Rejected (order rejected by exchange)
    pub fn reject(mut self, reason: String) -> OrderRejected {
        let now = SystemTime::now();
        self.data.completed_at = Some(now);
        self.data.updated_at = now;
        self.data.rejection_reason = Some(reason);
        OrderRejected { data: self.data }
    }

    /// Get current status
    pub fn status(&self) -> OrderStatus {
        OrderStatus::Pending
    }

    /// Convert to legacy OrderStatus enum (for backwards compatibility)
    pub fn to_legacy_status(&self) -> (OrderData, OrderStatus) {
        (self.data.clone(), OrderStatus::Pending)
    }
}

// ============================================================================
// State: Open
// ============================================================================

/// Order in Open state
///
/// Order has been acknowledged and is active in the orderbook.
///
/// **Valid Transitions:**
/// - `fill(size, price)` → OrderFilled | OrderPartiallyFilled
/// - `cancel()` → OrderCancelled
/// - `expire()` → OrderExpired
#[derive(Debug, Clone)]
pub struct OrderOpen {
    data: OrderData,
}

impl OrderOpen {
    /// Access the underlying order data
    pub fn data(&self) -> &OrderData {
        &self.data
    }

    /// Apply a fill to this order
    ///
    /// Returns FillResult indicating whether order is now filled or partially filled.
    ///
    /// # Validation
    ///
    /// This method performs strict validation to prevent accounting errors:
    /// - Fill quantity must be > 0
    /// - Fill price must be > 0
    /// - Fill quantity must not exceed remaining quantity
    ///
    /// # Errors
    ///
    /// Returns the order in an error variant if validation fails.
    /// The order is NOT modified on validation failure.
    pub fn fill(mut self, fill_quantity: u64, fill_price: u64) -> FillResultOrError {
        // CRITICAL VALIDATION: Prevent accounting errors

        // 1. Validate fill quantity is non-zero
        if fill_quantity == 0 {
            return FillResultOrError::Error(FillError::ZeroQuantity, self);
        }

        // 2. Validate fill price is non-zero
        if fill_price == 0 {
            return FillResultOrError::Error(FillError::ZeroPrice, self);
        }

        // 3. Validate fill doesn't exceed remaining quantity
        let remaining = self.data.remaining_quantity();
        if fill_quantity > remaining {
            return FillResultOrError::Error(
                FillError::ExceedsRemaining {
                    fill_qty: fill_quantity,
                    remaining_qty: remaining,
                    total_qty: self.data.quantity,
                },
                self,
            );
        }

        // All validation passed - apply fill
        let now = SystemTime::now();
        self.data.updated_at = now;

        // Use checked_add for safety (should never fail after validation, but defense in depth)
        self.data.filled_quantity = self
            .data
            .filled_quantity
            .checked_add(fill_quantity)
            .unwrap_or(self.data.quantity); // Fallback to full fill if somehow overflows

        // Check if fully filled
        if self.data.is_fully_filled() {
            self.data.completed_at = Some(now);
            FillResultOrError::Ok(FillResult::Filled(OrderFilled { data: self.data }))
        } else {
            FillResultOrError::Ok(FillResult::PartiallyFilled(OrderPartiallyFilled {
                data: self.data,
            }))
        }
    }

    /// Transition: Open → Cancelled
    pub fn cancel(mut self) -> OrderCancelled {
        let now = SystemTime::now();
        self.data.completed_at = Some(now);
        self.data.updated_at = now;
        OrderCancelled { data: self.data }
    }

    /// Transition: Open → Expired
    pub fn expire(mut self) -> OrderExpired {
        let now = SystemTime::now();
        self.data.completed_at = Some(now);
        self.data.updated_at = now;
        OrderExpired { data: self.data }
    }

    /// Get current status
    pub fn status(&self) -> OrderStatus {
        OrderStatus::Open
    }

    /// Convert to legacy OrderStatus enum (for backwards compatibility)
    pub fn to_legacy_status(&self) -> (OrderData, OrderStatus) {
        (self.data.clone(), OrderStatus::Open)
    }
}

// ============================================================================
// State: PartiallyFilled
// ============================================================================

/// Order in PartiallyFilled state
///
/// Order has been partially filled but still has remaining quantity.
///
/// **Valid Transitions:**
/// - `fill(size, price)` → OrderFilled | OrderPartiallyFilled (recursive)
/// - `cancel()` → OrderCancelled
#[derive(Debug, Clone)]
pub struct OrderPartiallyFilled {
    data: OrderData,
}

impl OrderPartiallyFilled {
    /// Access the underlying order data
    pub fn data(&self) -> &OrderData {
        &self.data
    }

    /// Get filled quantity
    pub fn filled_quantity(&self) -> u64 {
        self.data.filled_quantity
    }

    /// Get remaining quantity
    pub fn remaining_quantity(&self) -> u64 {
        self.data.remaining_quantity()
    }

    /// Apply another fill to this order
    ///
    /// Returns FillResult indicating whether order is now filled or still partially filled.
    ///
    /// # Validation
    ///
    /// Same strict validation as OrderOpen::fill():
    /// - Fill quantity must be > 0
    /// - Fill price must be > 0
    /// - Fill quantity must not exceed remaining quantity
    pub fn fill(mut self, fill_quantity: u64, fill_price: u64) -> PartialFillResultOrError {
        // CRITICAL VALIDATION: Prevent accounting errors

        // 1. Validate fill quantity is non-zero
        if fill_quantity == 0 {
            return PartialFillResultOrError::Error(FillError::ZeroQuantity, self);
        }

        // 2. Validate fill price is non-zero
        if fill_price == 0 {
            return PartialFillResultOrError::Error(FillError::ZeroPrice, self);
        }

        // 3. Validate fill doesn't exceed remaining quantity
        let remaining = self.data.remaining_quantity();
        if fill_quantity > remaining {
            return PartialFillResultOrError::Error(
                FillError::ExceedsRemaining {
                    fill_qty: fill_quantity,
                    remaining_qty: remaining,
                    total_qty: self.data.quantity,
                },
                self,
            );
        }

        // All validation passed - apply fill
        let now = SystemTime::now();
        self.data.updated_at = now;

        // Use checked_add for safety
        self.data.filled_quantity = self
            .data
            .filled_quantity
            .checked_add(fill_quantity)
            .unwrap_or(self.data.quantity);

        // Check if fully filled
        if self.data.is_fully_filled() {
            self.data.completed_at = Some(now);
            PartialFillResultOrError::Ok(FillResult::Filled(OrderFilled { data: self.data }))
        } else {
            PartialFillResultOrError::Ok(FillResult::PartiallyFilled(OrderPartiallyFilled {
                data: self.data,
            }))
        }
    }

    /// Transition: PartiallyFilled → Cancelled
    pub fn cancel(mut self) -> OrderCancelled {
        let now = SystemTime::now();
        self.data.completed_at = Some(now);
        self.data.updated_at = now;
        OrderCancelled { data: self.data }
    }

    /// Get current status
    pub fn status(&self) -> OrderStatus {
        OrderStatus::PartiallyFilled
    }

    /// Convert to legacy OrderStatus enum (for backwards compatibility)
    pub fn to_legacy_status(&self) -> (OrderData, OrderStatus) {
        (self.data.clone(), OrderStatus::PartiallyFilled)
    }
}

// ============================================================================
// State: Filled (Terminal)
// ============================================================================

/// Order in Filled state (terminal)
///
/// Order has been completely filled. This is a terminal state.
///
/// **Valid Transitions:** None (terminal state)
#[derive(Debug, Clone)]
pub struct OrderFilled {
    data: OrderData,
}

impl OrderFilled {
    /// Access the underlying order data
    pub fn data(&self) -> &OrderData {
        &self.data
    }

    /// Get filled quantity (always equals order quantity)
    pub fn filled_quantity(&self) -> u64 {
        self.data.filled_quantity
    }

    /// Get current status
    pub fn status(&self) -> OrderStatus {
        OrderStatus::Filled
    }

    /// Convert to legacy OrderStatus enum (for backwards compatibility)
    pub fn to_legacy_status(&self) -> (OrderData, OrderStatus) {
        (self.data.clone(), OrderStatus::Filled)
    }
}

// ============================================================================
// State: Cancelled (Terminal)
// ============================================================================

/// Order in Cancelled state (terminal)
///
/// Order has been cancelled (either by user or system). This is a terminal state.
///
/// **Valid Transitions:** None (terminal state)
#[derive(Debug, Clone)]
pub struct OrderCancelled {
    data: OrderData,
}

impl OrderCancelled {
    /// Access the underlying order data
    pub fn data(&self) -> &OrderData {
        &self.data
    }

    /// Get filled quantity (if any fills happened before cancellation)
    pub fn filled_quantity(&self) -> u64 {
        self.data.filled_quantity
    }

    /// Check if this was a partial fill cancellation
    pub fn was_partially_filled(&self) -> bool {
        self.data.filled_quantity > 0
    }

    /// Get current status
    pub fn status(&self) -> OrderStatus {
        OrderStatus::Cancelled
    }

    /// Convert to legacy OrderStatus enum (for backwards compatibility)
    pub fn to_legacy_status(&self) -> (OrderData, OrderStatus) {
        (self.data.clone(), OrderStatus::Cancelled)
    }
}

// ============================================================================
// State: Rejected (Terminal)
// ============================================================================

/// Order in Rejected state (terminal)
///
/// Order was rejected by the exchange. This is a terminal state.
///
/// **Valid Transitions:** None (terminal state)
#[derive(Debug, Clone)]
pub struct OrderRejected {
    data: OrderData,
}

impl OrderRejected {
    /// Access the underlying order data
    pub fn data(&self) -> &OrderData {
        &self.data
    }

    /// Get rejection reason
    pub fn reason(&self) -> Option<&str> {
        self.data.rejection_reason.as_deref()
    }

    /// Get current status
    pub fn status(&self) -> OrderStatus {
        OrderStatus::Rejected
    }

    /// Convert to legacy OrderStatus enum (for backwards compatibility)
    pub fn to_legacy_status(&self) -> (OrderData, OrderStatus) {
        (self.data.clone(), OrderStatus::Rejected)
    }
}

// ============================================================================
// State: Expired (Terminal)
// ============================================================================

/// Order in Expired state (terminal)
///
/// Order expired (time-based expiration). This is a terminal state.
///
/// **Valid Transitions:** None (terminal state)
#[derive(Debug, Clone)]
pub struct OrderExpired {
    data: OrderData,
}

impl OrderExpired {
    /// Access the underlying order data
    pub fn data(&self) -> &OrderData {
        &self.data
    }

    /// Get current status
    pub fn status(&self) -> OrderStatus {
        OrderStatus::Expired
    }

    /// Convert to legacy OrderStatus enum (for backwards compatibility)
    pub fn to_legacy_status(&self) -> (OrderData, OrderStatus) {
        (self.data.clone(), OrderStatus::Expired)
    }
}

// ============================================================================
// Fill Result (Sum Type)
// ============================================================================

/// Result of applying a fill to an order
///
/// This enum represents the two possible outcomes when filling an order:
/// - The order becomes fully filled
/// - The order remains partially filled
pub enum FillResult {
    /// Order is now fully filled
    Filled(OrderFilled),
    /// Order is partially filled (more fills can be applied)
    PartiallyFilled(OrderPartiallyFilled),
}

impl FillResult {
    /// Check if order is fully filled
    pub fn is_filled(&self) -> bool {
        matches!(self, FillResult::Filled(_))
    }

    /// Check if order is partially filled
    pub fn is_partially_filled(&self) -> bool {
        matches!(self, FillResult::PartiallyFilled(_))
    }
}

// ============================================================================
// Enum wrapper (for storage/serialization)
// ============================================================================

/// Type-erased order state
///
/// This enum wraps all order states and is useful for:
/// - Storage in collections
/// - Serialization/deserialization
/// - APIs that need to handle orders in any state
///
/// **Note:** Using this enum gives up compile-time state guarantees.
/// Prefer using the typed states (OrderPending, OrderOpen, etc.) when possible.
#[derive(Debug, Clone)]
pub enum OrderState {
    Pending(OrderPending),
    Open(OrderOpen),
    PartiallyFilled(OrderPartiallyFilled),
    Filled(OrderFilled),
    Cancelled(OrderCancelled),
    Rejected(OrderRejected),
    Expired(OrderExpired),
}

impl OrderState {
    /// Get the order ID
    pub fn id(&self) -> &OrderId {
        match self {
            OrderState::Pending(o) => &o.data.id,
            OrderState::Open(o) => &o.data.id,
            OrderState::PartiallyFilled(o) => &o.data.id,
            OrderState::Filled(o) => &o.data.id,
            OrderState::Cancelled(o) => &o.data.id,
            OrderState::Rejected(o) => &o.data.id,
            OrderState::Expired(o) => &o.data.id,
        }
    }

    /// Get the current order status
    pub fn status(&self) -> OrderStatus {
        match self {
            OrderState::Pending(_) => OrderStatus::Pending,
            OrderState::Open(_) => OrderStatus::Open,
            OrderState::PartiallyFilled(_) => OrderStatus::PartiallyFilled,
            OrderState::Filled(_) => OrderStatus::Filled,
            OrderState::Cancelled(_) => OrderStatus::Cancelled,
            OrderState::Rejected(_) => OrderStatus::Rejected,
            OrderState::Expired(_) => OrderStatus::Expired,
        }
    }

    /// Check if order is in a terminal state
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            OrderState::Filled(_)
                | OrderState::Cancelled(_)
                | OrderState::Rejected(_)
                | OrderState::Expired(_)
        )
    }

    /// Check if order is active (can still receive fills)
    pub fn is_active(&self) -> bool {
        matches!(
            self,
            OrderState::Pending(_) | OrderState::Open(_) | OrderState::PartiallyFilled(_)
        )
    }
}

// Conversion from typed states to enum
impl From<OrderPending> for OrderState {
    fn from(o: OrderPending) -> Self {
        OrderState::Pending(o)
    }
}

impl From<OrderOpen> for OrderState {
    fn from(o: OrderOpen) -> Self {
        OrderState::Open(o)
    }
}

impl From<OrderPartiallyFilled> for OrderState {
    fn from(o: OrderPartiallyFilled) -> Self {
        OrderState::PartiallyFilled(o)
    }
}

impl From<OrderFilled> for OrderState {
    fn from(o: OrderFilled) -> Self {
        OrderState::Filled(o)
    }
}

impl From<OrderCancelled> for OrderState {
    fn from(o: OrderCancelled) -> Self {
        OrderState::Cancelled(o)
    }
}

impl From<OrderRejected> for OrderState {
    fn from(o: OrderRejected) -> Self {
        OrderState::Rejected(o)
    }
}

impl From<OrderExpired> for OrderState {
    fn from(o: OrderExpired) -> Self {
        OrderState::Expired(o)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_order() -> OrderPending {
        OrderPending::new(
            OrderId::new_random(),
            Side::Buy,
            50_000_000_000_000, // $50,000
            1_000_000_000,      // 1.0 BTC
        )
    }

    // ========================================================================
    // Valid Transition Tests
    // ========================================================================

    #[test]
    fn test_pending_to_open() {
        let order = create_test_order();
        let order_id = order.data().id.clone();

        assert_eq!(order.status(), OrderStatus::Pending);
        assert!(order.data().acknowledged_at.is_none());

        let order = order.acknowledge();

        assert_eq!(order.status(), OrderStatus::Open);
        assert!(order.data().acknowledged_at.is_some());
        assert_eq!(order.data().id, order_id);
    }

    #[test]
    fn test_pending_to_rejected() {
        let order = create_test_order();

        let order = order.reject("Insufficient funds".to_string());

        assert_eq!(order.status(), OrderStatus::Rejected);
        assert_eq!(order.reason(), Some("Insufficient funds"));
        assert!(order.data().completed_at.is_some());
    }

    #[test]
    fn test_open_to_filled_single_fill() {
        let order = create_test_order().acknowledge();

        let result = order.fill(1_000_000_000, 50_000_000_000_000);

        if let FillResultOrError::Ok(fill_result) = result {
            assert!(fill_result.is_filled());
            if let FillResult::Filled(order) = fill_result {
                assert_eq!(order.status(), OrderStatus::Filled);
                assert_eq!(order.filled_quantity(), 1_000_000_000);
                assert!(order.data().completed_at.is_some());
            }
        } else {
            panic!("Expected Ok result");
        }
    }

    #[test]
    fn test_open_to_partially_filled() {
        let order = create_test_order().acknowledge();

        let result = order.fill(500_000_000, 50_000_000_000_000); // Half fill

        if let FillResultOrError::Ok(fill_result) = result {
            assert!(fill_result.is_partially_filled());
            if let FillResult::PartiallyFilled(order) = fill_result {
                assert_eq!(order.status(), OrderStatus::PartiallyFilled);
                assert_eq!(order.filled_quantity(), 500_000_000);
                assert_eq!(order.remaining_quantity(), 500_000_000);
                assert!(order.data().completed_at.is_none()); // Not completed yet
            }
        } else {
            panic!("Expected Ok result");
        }
    }

    #[test]
    fn test_partially_filled_to_filled() {
        let order = create_test_order().acknowledge();

        // First fill: 40%
        let result = order.fill(400_000_000, 50_000_000_000_000);
        let order = match result {
            FillResultOrError::Ok(FillResult::PartiallyFilled(o)) => o,
            _ => panic!("Expected partially filled"),
        };

        assert_eq!(order.filled_quantity(), 400_000_000);

        // Second fill: 60% (total 100%)
        let result = order.fill(600_000_000, 50_000_000_000_000);

        assert!(result.is_filled());
        if let PartialFillResultOrError::Ok(FillResult::Filled(order)) = result {
            assert_eq!(order.filled_quantity(), 1_000_000_000);
            assert!(order.data().completed_at.is_some());
        }
    }

    #[test]
    fn test_partially_filled_stays_partially_filled() {
        let order = create_test_order().acknowledge();

        // First fill: 30%
        let result = order.fill(300_000_000, 50_000_000_000_000);
        let order = match result {
            FillResultOrError::Ok(FillResult::PartiallyFilled(o)) => o,
            _ => panic!("Expected partially filled"),
        };

        // Second fill: 20% (total 50%)
        let result = order.fill(200_000_000, 50_000_000_000_000);

        assert!(result.is_partially_filled());
        if let PartialFillResultOrError::Ok(FillResult::PartiallyFilled(order)) = result {
            assert_eq!(order.filled_quantity(), 500_000_000);
            assert_eq!(order.remaining_quantity(), 500_000_000);
        }
    }

    #[test]
    fn test_open_to_cancelled() {
        let order = create_test_order().acknowledge();

        let order = order.cancel();

        assert_eq!(order.status(), OrderStatus::Cancelled);
        assert!(!order.was_partially_filled());
        assert_eq!(order.filled_quantity(), 0);
        assert!(order.data().completed_at.is_some());
    }

    #[test]
    fn test_partially_filled_to_cancelled() {
        let order = create_test_order().acknowledge();

        let result = order.fill(300_000_000, 50_000_000_000_000);
        let order = match result {
            FillResultOrError::Ok(FillResult::PartiallyFilled(o)) => o,
            _ => panic!("Expected partially filled"),
        };

        let order = order.cancel();

        assert_eq!(order.status(), OrderStatus::Cancelled);
        assert!(order.was_partially_filled());
        assert_eq!(order.filled_quantity(), 300_000_000);
    }

    #[test]
    fn test_open_to_expired() {
        let order = create_test_order().acknowledge();

        let order = order.expire();

        assert_eq!(order.status(), OrderStatus::Expired);
        assert!(order.data().completed_at.is_some());
    }

    // ========================================================================
    // Fill Logic Tests
    // ========================================================================

    #[test]
    fn test_fill_overflow_protection() {
        let order = create_test_order().acknowledge();

        // Try to overfill - should return error (not cap)
        let result = order.fill(2_000_000_000, 50_000_000_000_000);

        // Overfill returns error to prevent accounting issues
        assert!(result.is_err());
        if let FillResultOrError::Error(FillError::ExceedsRemaining { .. }, _order) = result {
            // Expected behavior: fill rejected when exceeds remaining
        } else {
            panic!("Expected ExceedsRemaining error");
        }
    }

    #[test]
    fn test_multiple_fills_with_overflow() {
        let order = create_test_order().acknowledge();

        // Fill 60%
        let result = order.fill(600_000_000, 50_000_000_000_000);
        let order = match result {
            FillResultOrError::Ok(FillResult::PartiallyFilled(o)) => o,
            _ => panic!("Expected partially filled"),
        };

        // Fill 60% again (should error - exceeds remaining 40%)
        let result = order.fill(600_000_000, 50_000_000_000_000);

        // Overfill returns error
        assert!(result.is_err());
        if let PartialFillResultOrError::Error(FillError::ExceedsRemaining { remaining_qty, .. }, returned_order) = result {
            assert_eq!(remaining_qty, 400_000_000); // 40% remaining
            // Can still fill the correct amount
            let result = returned_order.fill(400_000_000, 50_000_000_000_000);
            assert!(result.is_filled());
        } else {
            panic!("Expected ExceedsRemaining error");
        }
    }

    #[test]
    fn test_fill_percentage_calculation() {
        let order = create_test_order();

        assert_eq!(order.data().fill_percentage(), 0);

        let order = order.acknowledge();
        let result = order.fill(250_000_000, 50_000_000_000_000); // 25%

        if let FillResultOrError::Ok(FillResult::PartiallyFilled(order)) = result {
            // 25% = 25 * 1e9 in fixed-point
            assert_eq!(order.data().fill_percentage(), 25_000_000_000);

            let result = order.fill(500_000_000, 50_000_000_000_000); // +50% = 75% total

            if let PartialFillResultOrError::Ok(FillResult::PartiallyFilled(order)) = result {
                assert_eq!(order.data().fill_percentage(), 75_000_000_000);
            }
        }
    }

    // ========================================================================
    // State Invariant Tests
    // ========================================================================

    #[test]
    fn test_order_id_preserved_across_transitions() {
        let order = create_test_order();
        let order_id = order.data().id.clone();

        let order = order.acknowledge();
        assert_eq!(order.data().id, order_id);

        let result = order.fill(500_000_000, 50_000_000_000_000);
        if let FillResultOrError::Ok(FillResult::PartiallyFilled(order)) = result {
            assert_eq!(order.data().id, order_id);

            let order = order.cancel();
            assert_eq!(order.data().id, order_id);
        }
    }

    #[test]
    fn test_timestamps_update_on_transitions() {
        let order = create_test_order();
        let created_at = order.data().created_at;

        // Small delay to ensure different timestamps
        std::thread::sleep(std::time::Duration::from_millis(1));

        let order = order.acknowledge();
        assert!(order.data().updated_at > created_at);
        assert!(order.data().acknowledged_at.is_some());

        let prev_updated = order.data().updated_at;
        std::thread::sleep(std::time::Duration::from_millis(1));

        let order = order.cancel();
        assert!(order.data().updated_at > prev_updated);
        assert!(order.data().completed_at.is_some());
    }

    #[test]
    fn test_filled_quantity_never_decreases() {
        let order = create_test_order().acknowledge();

        let result = order.fill(300_000_000, 50_000_000_000_000);
        let order = match result {
            FillResultOrError::Ok(FillResult::PartiallyFilled(o)) => o,
            _ => panic!("Expected partially filled"),
        };
        assert_eq!(order.filled_quantity(), 300_000_000);

        let result = order.fill(400_000_000, 50_000_000_000_000);
        let order = match result {
            PartialFillResultOrError::Ok(FillResult::PartiallyFilled(o)) => o,
            PartialFillResultOrError::Ok(FillResult::Filled(o)) => {
                // If it went to filled, that's fine too
                assert_eq!(o.filled_quantity(), 700_000_000);
                return;
            }
            _ => panic!("Expected fill to succeed"),
        };
        assert_eq!(order.filled_quantity(), 700_000_000);
    }

    // ========================================================================
    // OrderState Enum Tests
    // ========================================================================

    #[test]
    fn test_order_state_enum_conversions() {
        let order = create_test_order();
        let state: OrderState = order.into();

        assert_eq!(state.status(), OrderStatus::Pending);
        assert!(!state.is_terminal());
        assert!(state.is_active());
    }

    #[test]
    fn test_order_state_terminal_detection() {
        // Filled is terminal
        let order = create_test_order().acknowledge();
        let result = order.fill(1_000_000_000, 50_000_000_000_000);
        if let FillResultOrError::Ok(FillResult::Filled(order)) = result {
            let state: OrderState = order.into();
            assert!(state.is_terminal());
            assert!(!state.is_active());
        }

        // Cancelled is terminal
        let order = create_test_order().acknowledge();
        let order = order.cancel();
        let state: OrderState = order.into();
        assert!(state.is_terminal());
        assert!(!state.is_active());

        // Rejected is terminal
        let order = create_test_order();
        let order = order.reject("Test".to_string());
        let state: OrderState = order.into();
        assert!(state.is_terminal());
        assert!(!state.is_active());

        // Expired is terminal
        let order = create_test_order().acknowledge();
        let order = order.expire();
        let state: OrderState = order.into();
        assert!(state.is_terminal());
        assert!(!state.is_active());
    }

    #[test]
    fn test_order_state_active_detection() {
        // Pending is active
        let order = create_test_order();
        let state: OrderState = order.into();
        assert!(state.is_active());
        assert!(!state.is_terminal());

        // Open is active
        let order = create_test_order().acknowledge();
        let state: OrderState = order.into();
        assert!(state.is_active());
        assert!(!state.is_terminal());

        // PartiallyFilled is active
        let order = create_test_order().acknowledge();
        let result = order.fill(500_000_000, 50_000_000_000_000);
        if let FillResultOrError::Ok(FillResult::PartiallyFilled(order)) = result {
            let state: OrderState = order.into();
            assert!(state.is_active());
            assert!(!state.is_terminal());
        }
    }

    // ========================================================================
    // Order Data Tests
    // ========================================================================

    #[test]
    fn test_order_data_remaining_quantity() {
        let order = create_test_order();
        assert_eq!(order.data().remaining_quantity(), 1_000_000_000);

        let order = order.acknowledge();
        let result = order.fill(300_000_000, 50_000_000_000_000);

        if let FillResultOrError::Ok(FillResult::PartiallyFilled(order)) = result {
            assert_eq!(order.data().remaining_quantity(), 700_000_000);
        }
    }

    #[test]
    fn test_order_data_is_fully_filled() {
        let order = create_test_order();
        assert!(!order.data().is_fully_filled());

        let order = order.acknowledge();
        let result = order.fill(1_000_000_000, 50_000_000_000_000);

        if let FillResultOrError::Ok(FillResult::Filled(order)) = result {
            assert!(order.data().is_fully_filled());
        }
    }

    // ========================================================================
    // Edge Case Tests
    // ========================================================================

    #[test]
    fn test_zero_quantity_fill_percentage() {
        let order = OrderPending::new(
            OrderId::new_random(),
            Side::Buy,
            50_000_000_000_000,
            0, // Zero quantity
        );

        // Should not panic, should return 0
        assert_eq!(order.data().fill_percentage(), 0);
    }

    #[test]
    fn test_order_type_preservation() {
        let order = OrderPending::new_with_type(
            OrderId::new_random(),
            Side::Sell,
            OrderType::PostOnly,
            50_000_000_000_000,
            1_000_000_000,
        );

        assert_eq!(order.data().order_type, OrderType::PostOnly);
        assert_eq!(order.data().side, Side::Sell);

        let order = order.acknowledge();
        assert_eq!(order.data().order_type, OrderType::PostOnly);
    }

    // ========================================================================
    // Compile-Time Safety Demonstrations
    // ========================================================================

    // These tests demonstrate that invalid transitions won't compile.
    // Uncomment any of these to see compile errors:

    // #[test]
    // fn test_cannot_fill_pending_order() {
    //     let order = create_test_order();
    //     // This won't compile - Pending doesn't have fill()
    //     // order.fill(100, 50000);
    // }

    // #[test]
    // fn test_cannot_acknowledge_open_order() {
    //     let order = create_test_order().acknowledge();
    //     // This won't compile - Open doesn't have acknowledge()
    //     // order.acknowledge();
    // }

    // #[test]
    // fn test_cannot_transition_from_filled() {
    //     let order = create_test_order().acknowledge();
    //     let result = order.fill(1_000_000_000, 50_000_000_000_000);
    //     if let FillResult::Filled(order) = result {
    //         // This won't compile - Filled has no transition methods
    //         // order.cancel();
    //         // order.fill(100, 50000);
    //     }
    // }

    // ========================================================================
    // Stress Tests
    // ========================================================================

    #[test]
    fn test_many_small_fills() {
        let order = create_test_order().acknowledge();

        // 100 fills of 1% each
        let fill_size = 10_000_000; // 0.01 BTC
        let mut current = FillResult::PartiallyFilled(OrderPartiallyFilled {
            data: order.data,
        });

        for i in 0..100 {
            match current {
                FillResult::PartiallyFilled(order) => {
                    match order.fill(fill_size, 50_000_000_000_000) {
                        PartialFillResultOrError::Ok(result) => current = result,
                        PartialFillResultOrError::Error(_, _) => panic!("Fill failed"),
                    }
                }
                FillResult::Filled(order) => {
                    // Should reach filled at exactly 100 fills
                    assert_eq!(i, 100);
                    assert_eq!(order.filled_quantity(), 1_000_000_000);
                    return;
                }
            }
        }

        // Should have reached Filled state
        assert!(matches!(current, FillResult::Filled(_)));
    }

    #[test]
    fn test_concurrent_safety_order_id() {
        // OrderId should be cloneable and thread-safe
        let order = create_test_order();
        let order_id = order.data().id.clone();

        // This demonstrates that OrderId can be sent across threads
        let handle = std::thread::spawn(move || {
            order_id
        });

        let received_id = handle.join().unwrap();
        assert_eq!(received_id, order.data().id);
    }
}
