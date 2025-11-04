//! Bog Strategies - HFT Trading Strategy Implementations
//!
//! This crate contains zero-overhead strategy implementations using:
//! - Zero-sized types (ZSTs) - no memory overhead
//! - Const parameters from Cargo features - compile-time configuration
//! - u64 fixed-point arithmetic - no heap allocations
//! - #[inline(always)] - maximum performance
//!
//! ## Strategies
//! - **SimpleSpread**: Basic market making with fixed spread
//! - **InventoryBased**: Avellaneda-Stoikov inventory management (stub)
//!
//! ## Performance Characteristics
//! - Strategy struct size: 0 bytes (zero-sized types)
//! - Signal generation: <100ns target
//! - Zero heap allocations
//! - Full compile-time optimization via monomorphization
//!
//! ## Configuration
//! All parameters are set via Cargo features at compile time:
//!
//! ```toml
//! [features]
//! spread-10bps = []
//! size-0.1 = []
//! min-spread-1bps = []
//! ```

pub mod simple_spread;
pub mod inventory_based;

// Re-export strategies for convenience
pub use simple_spread::SimpleSpread;
pub use inventory_based::InventoryBased;
