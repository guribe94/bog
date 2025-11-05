//! Bog Strategies - Zero-Overhead HFT Trading Strategies
//!
//! Ultra-low-latency trading strategy implementations designed for sub-microsecond HFT.
//!
//! ## Performance Achieved
//!
//! **Target**: <100ns strategy calculation
//! **Measured**: **~5ns** average (20x faster than target) ✅
//!
//! All strategies are **zero-sized types (ZSTs)** - they occupy **0 bytes** of memory.
//!
//! ## Available Strategies
//!
//! ### [`SimpleSpread`] - Basic Market Making ✅ Production Ready
//!
//! Places symmetric quotes around mid-price with configurable spread and size.
//!
//! **Features:**
//! - Fixed spread market making (5bps, 10bps, or 20bps)
//! - Configurable order sizes (0.01, 0.1, or 1.0 BTC)
//! - Minimum spread filter (1bps, 5bps, or 10bps)
//! - Measured latency: ~5ns per signal generation
//!
//! **Configuration:**
//! ```toml
//! bog-strategies = { features = ["spread-10bps", "size-0.1", "min-spread-1bps"] }
//! ```
//!
//! ### [`InventoryBased`] - Avellaneda-Stoikov 🚧 Stub (Phase 9)
//!
//! Advanced inventory-risk-averse market making with dynamic spread adjustment.
//!
//! **Planned Features:**
//! - Skew quotes based on current inventory
//! - Dynamic spread based on volatility
//! - Risk aversion parameters (low, medium, high)
//! - Target inventory management
//!
//! ## Zero-Overhead Architecture
//!
//! ### Design Principles
//!
//! 1. **Zero-Sized Types (ZSTs)** - Strategies contain no data
//!    ```rust
//!    assert_eq!(std::mem::size_of::<SimpleSpread>(), 0);
//!    ```
//!
//! 2. **Compile-Time Parameters** - All configuration via Cargo features
//!    ```rust
//!    const SPREAD: u64 = 10_000_000;  // 10bps, set at compile time
//!    ```
//!
//! 3. **u64 Fixed-Point Arithmetic** - No `Decimal` heap allocations
//!    ```rust
//!    let mid_price: u64 = 50_000_000_000_000;  // $50k in 9 decimals
//!    ```
//!
//! 4. **Aggressive Inlining** - Full monomorphization by LLVM
//!    ```rust
//!    #[inline(always)]
//!    fn calculate(&mut self, snapshot: &MarketSnapshot) -> Signal
//!    ```
//!
//! ## Usage Example
//!
//! ```rust
//! use bog_strategies::SimpleSpread;
//! use bog_core::prelude::*;
//!
//! // Create strategy (0 bytes, compiles to pure code)
//! let mut strategy = SimpleSpread;
//!
//! // Calculate signal from market data
//! # use bog_core::data::MarketSnapshot;
//! # let snapshot = unsafe { std::mem::zeroed() };
//! let signal = strategy.calculate(&snapshot);
//!
//! // Signal is stack-allocated, 64 bytes (one cache line)
//! assert_eq!(std::mem::size_of_val(&signal), 64);
//! ```
//!
//! ## Compile-Time Configuration
//!
//! ### Spread Configuration
//! - `spread-5bps` - 5 basis points (aggressive)
//! - `spread-10bps` - 10 basis points (balanced) - **default**
//! - `spread-20bps` - 20 basis points (conservative)
//!
//! ### Order Size Configuration
//! - `size-small` - 0.01 BTC per order
//! - `size-medium` - 0.1 BTC per order - **default**
//! - `size-large` - 1.0 BTC per order
//!
//! ### Minimum Spread Filter
//! - `min-spread-1bps` - Trade if spread ≥1bp - **default**
//! - `min-spread-5bps` - Trade if spread ≥5bp
//! - `min-spread-10bps` - Trade if spread ≥10bp
//!
//! ### Inventory Risk Aversion (InventoryBased only)
//! - `risk-low` - Low risk aversion (γ=0.01)
//! - `risk-medium` - Medium risk aversion (γ=0.1) - **default**
//! - `risk-high` - High risk aversion (γ=1.0)
//!
//! ## Integration with bog-core
//!
//! Strategies implement the `bog_core::engine::Strategy` trait:
//!
//! ```rust,ignore
//! pub trait Strategy {
//!     fn calculate(&mut self, snapshot: &MarketSnapshot) -> Signal;
//!     fn name(&self) -> &'static str;
//! }
//! ```
//!
//! And are used with `Engine<S: Strategy, E: Executor>`:
//!
//! ```rust,ignore
//! let strategy = SimpleSpread;
//! let executor = SimulatedExecutor::new_default();
//! let engine = Engine::new(strategy, executor);
//! ```
//!
//! ## Performance Benchmarks
//!
//! From `bog-core/benches/engine_bench.rs`:
//!
//! | Benchmark | Target | Measured | Status |
//! |-----------|--------|----------|--------|
//! | Strategy calculation | <100ns | ~5ns | ✅ 20x under |
//! | Complete tick-to-trade | <1000ns | ~15ns | ✅ 67x under |
//! | Signal creation | N/A | ~1ns | ✅ Negligible |
//!
//! See [Performance Report](../PERFORMANCE_REPORT.md) for detailed benchmarks.

pub mod simple_spread;
pub mod inventory_based;

// Re-export strategies for convenience
pub use simple_spread::SimpleSpread;
pub use inventory_based::InventoryBased;
