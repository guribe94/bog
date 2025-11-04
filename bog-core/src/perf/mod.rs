//! Performance Utilities for HFT Trading
//!
//! This module provides low-level performance optimizations:
//! - **CPU affinity**: Pin threads to specific cores
//! - **Lock-free metrics**: Cache-aligned atomic counters
//! - **Object pools**: Pre-allocated pools for zero-allocation hot paths
//!
//! All utilities are designed for sub-microsecond latency requirements.

pub mod cpu;
pub mod metrics;
pub mod pools;

// Re-exports for convenience
pub use cpu::{num_cores, optimize_for_hft, pin_to_core, set_realtime_priority};
pub use metrics::{CacheAligned, Metrics, MetricsSnapshot};
pub use pools::{ObjectPool, PoolGuard, PoolStats};
