//! Core zero-overhead types for HFT trading
//!
//! This module provides the fundamental building blocks for ultra-low-latency trading:
//! - `OrderId`: u128-based order identifiers (zero heap allocation)
//! - `Signal`: 64-byte stack-allocated trading signals (cache-line aligned)
//! - `Position`: Cache-aligned atomic position state (lock-free)
//! - Fixed-point arithmetic utilities
//!
//! All types are designed to minimize latency:
//! - Copy semantics where possible (no allocations)
//! - Cache-line alignment (64 bytes)
//! - Atomic operations (lock-free)
//! - Minimal memory footprint

pub mod errors;
pub mod signal;
pub mod types;

// Re-export commonly used types
pub use errors::{ConversionError, OverflowError, PositionError};
pub use signal::{Signal, SignalAction};
pub use types::{
    fixed_point, OrderId, OrderStatus, OrderType, Position, Side,
};
