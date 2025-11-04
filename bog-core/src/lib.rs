//! Bog Core - High-Performance Market Maker for Lighter DEX
//!
//! Bog is an HFT market making trading bot designed for sub-microsecond latency.
//! It integrates with Huginn's ultra-low-latency market data feed via shared memory IPC.
//!
//! ## Architecture
//! - **Zero heap allocations** in hot path
//! - **Cache-line aligned** data structures (64 bytes)
//! - **Lock-free** atomic operations
//! - **Const generics** for compile-time dispatch
//! - **Separate binaries** per strategy/execution combination
//!
//! ## Performance Targets
//! - Tick-to-trade latency: <500ns (leaves 500ns for network)
//! - Strategy calculation: <100ns
//! - Risk validation: <50ns
//! - Zero dynamic dispatch, zero allocations
//!
//! ## Core Modules
//! - `core`: Zero-overhead types (OrderId, Signal, Position)
//! - `data`: Huginn market data integration
//! - `orderbook`: Local orderbook representation (stub)
//! - `strategy`: Strategy implementations (moved to bog-strategies)
//! - `execution`: Execution engines (simulated, live)
//! - `risk`: Risk management
//! - `engine`: Main trading engine with const generics

// Core zero-overhead types (new architecture)
pub mod core;

// Public modules (legacy, being refactored)
// TODO: Remove runtime config in favor of compile-time Cargo features
// pub mod config;
pub mod data;
pub mod orderbook;
pub mod strategy;
pub mod execution;
pub mod risk;
pub mod engine;  // NEW: Const generic engine
pub mod utils;

// Performance utilities (NEW)
pub mod perf;

// Re-export core types (new architecture)
pub use core::{
    OrderId, Position, Signal, SignalAction, Side, OrderType, OrderStatus,
    fixed_point,
};

// Re-export legacy types (for backwards compatibility during refactor)
pub use data::{MarketFeed, MarketSnapshot};
pub use execution::ExecutionMode;
pub use risk::RiskManager;

// Re-export new engine types (NEW - const generic)
pub use engine::{Engine, EngineStats, Executor, Strategy};

// Re-export error types
pub use anyhow::{Result, Error};

/// Prelude for convenient imports
pub mod prelude {
    // Core types
    pub use crate::core::{OrderId, Position, Signal, SignalAction, Side, fixed_point};

    // Engine
    pub use crate::engine::{Engine, EngineStats, Executor, Strategy};

    // Data feed
    pub use crate::data::{MarketFeed, MarketSnapshot};

    // Performance utilities
    pub use crate::perf::{optimize_for_hft, pin_to_core, ObjectPool, Metrics};

    // Error types
    pub use crate::{Result, Error};
}
