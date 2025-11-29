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

pub mod circuit_breaker_fsm;
pub mod connection_fsm;
pub mod errors;
pub mod order_fsm;
pub mod signal;
pub mod strategy_fsm;
pub mod types;

// Re-export commonly used types
pub use errors::{ConversionError, OverflowError, PositionError};
pub use signal::{Signal, SignalAction};
pub use types::{fixed_point, OrderId, OrderStatus, OrderType, Position, Side};

// Re-export order state machine types
pub use order_fsm::{
    FillError, FillResult, FillResultOrError, OrderCancelled, OrderData, OrderExpired, OrderFilled,
    OrderOpen, OrderPartiallyFilled, OrderPending, OrderRejected, OrderState,
    PartialFillResultOrError,
};

// Re-export circuit breaker state machine types
pub use circuit_breaker_fsm::{
    BinaryBreakerState, BinaryHalted, BinaryNormal, HaltReason, ThreeStateBreakerState,
    ThreeStateClosed, ThreeStateHalfOpen, ThreeStateHalfOrClosed, ThreeStateOpen,
    ThreeStateOpenOrHalf, ThreeStateResult,
};

// Re-export strategy state machine types
pub use strategy_fsm::{
    StrategyActive, StrategyData, StrategyInitializing, StrategyPaused, StrategyState,
    StrategyStopped,
};

// Re-export connection state machine types
pub use connection_fsm::{
    ConnectionConnected, ConnectionData, ConnectionDisconnected, ConnectionFailed,
    ConnectionReconnecting, ConnectionState as ConnectionFsmState, ReconnectResult, RetryResult,
};
