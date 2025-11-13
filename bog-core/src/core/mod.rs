//! Core zero-overhead types for HFT trading
//!
//! This module provides the fundamental building blocks for ultra-low-latency trading:
//! - `OrderId`: u128-based order identifiers (zero heap allocation)
//! - `Signal`: 64-byte stack-allocated trading signals (cache-line aligned)
//! - `Position`: Cache-aligned atomic position state (lock-free)
//! - `OrderFSM`: Compile-time verified order state machine (typestate pattern)
//! - Fixed-point arithmetic utilities
//!
//! All types are designed to minimize latency:
//! - Copy semantics where possible (no allocations)
//! - Cache-line alignment (64 bytes)
//! - Atomic operations (lock-free)
//! - Minimal memory footprint
//! - Compile-time state verification (zero runtime overhead)

pub mod errors;
pub mod signal;
pub mod types;
pub mod order_fsm;
pub mod circuit_breaker_fsm;
pub mod strategy_fsm;
pub mod connection_fsm;

// Re-export commonly used types
pub use errors::{ConversionError, OverflowError, PositionError};
pub use signal::{Signal, SignalAction};
pub use types::{
    fixed_point, OrderId, OrderStatus, OrderType, Position, Side,
};

// Re-export order state machine types
pub use order_fsm::{
    OrderPending, OrderOpen, OrderPartiallyFilled, OrderFilled,
    OrderCancelled, OrderRejected, OrderExpired, OrderState, FillResult, OrderData,
    FillError, FillResultOrError, PartialFillResultOrError,
};

// Re-export circuit breaker state machine types
pub use circuit_breaker_fsm::{
    BinaryNormal, BinaryHalted, BinaryBreakerState, HaltReason,
    ThreeStateClosed, ThreeStateOpen, ThreeStateHalfOpen, ThreeStateBreakerState,
    ThreeStateResult, ThreeStateOpenOrHalf, ThreeStateHalfOrClosed,
};

// Re-export strategy state machine types
pub use strategy_fsm::{
    StrategyInitializing, StrategyActive, StrategyPaused, StrategyStopped,
    StrategyState, StrategyData,
};

// Re-export connection state machine types
pub use connection_fsm::{
    ConnectionDisconnected, ConnectionConnected, ConnectionReconnecting, ConnectionFailed,
    ConnectionState as ConnectionFsmState, ConnectionData, RetryResult, ReconnectResult,
};
