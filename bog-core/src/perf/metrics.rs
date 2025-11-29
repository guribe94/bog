//! Lock-Free Performance Metrics
//!
//! Cache-aligned atomic counters for zero-overhead performance tracking.
//! All counters use relaxed ordering for maximum performance.

use std::sync::atomic::{AtomicU64, Ordering};

/// Cache-aligned metrics structure
///
/// Each metric is an atomic counter on its own cache line to prevent
/// false sharing between CPU cores.
#[repr(C, align(64))]
pub struct Metrics {
    /// Total market updates received
    pub updates_received: AtomicU64,

    /// Padding to next cache line
    _padding1: [u8; 56],

    /// Trading signals generated
    pub signals_generated: AtomicU64,

    /// Padding to next cache line
    _padding2: [u8; 56],

    /// Orders placed
    pub orders_placed: AtomicU64,

    /// Padding to next cache line
    _padding3: [u8; 56],

    /// Fills received
    pub fills_received: AtomicU64,

    /// Padding to next cache line
    _padding4: [u8; 56],

    /// Total latency in nanoseconds (cumulative)
    pub total_latency_ns: AtomicU64,

    /// Padding to next cache line
    _padding5: [u8; 56],
}

impl Metrics {
    /// Create new metrics with all counters at zero
    pub const fn new() -> Self {
        Self {
            updates_received: AtomicU64::new(0),
            _padding1: [0; 56],
            signals_generated: AtomicU64::new(0),
            _padding2: [0; 56],
            orders_placed: AtomicU64::new(0),
            _padding3: [0; 56],
            fills_received: AtomicU64::new(0),
            _padding4: [0; 56],
            total_latency_ns: AtomicU64::new(0),
            _padding5: [0; 56],
        }
    }

    /// Increment updates counter
    #[inline(always)]
    pub fn inc_updates(&self) {
        self.updates_received.fetch_add(1, Ordering::Relaxed);
    }

    /// Increment signals counter
    #[inline(always)]
    pub fn inc_signals(&self) {
        self.signals_generated.fetch_add(1, Ordering::Relaxed);
    }

    /// Increment orders counter
    #[inline(always)]
    pub fn inc_orders(&self) {
        self.orders_placed.fetch_add(1, Ordering::Relaxed);
    }

    /// Increment fills counter
    #[inline(always)]
    pub fn inc_fills(&self) {
        self.fills_received.fetch_add(1, Ordering::Relaxed);
    }

    /// Add latency measurement
    #[inline(always)]
    pub fn add_latency(&self, latency_ns: u64) {
        self.total_latency_ns
            .fetch_add(latency_ns, Ordering::Relaxed);
    }

    /// Get snapshot of all metrics
    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            updates_received: self.updates_received.load(Ordering::Relaxed),
            signals_generated: self.signals_generated.load(Ordering::Relaxed),
            orders_placed: self.orders_placed.load(Ordering::Relaxed),
            fills_received: self.fills_received.load(Ordering::Relaxed),
            total_latency_ns: self.total_latency_ns.load(Ordering::Relaxed),
        }
    }

    /// Reset all counters to zero
    pub fn reset(&self) {
        self.updates_received.store(0, Ordering::Relaxed);
        self.signals_generated.store(0, Ordering::Relaxed);
        self.orders_placed.store(0, Ordering::Relaxed);
        self.fills_received.store(0, Ordering::Relaxed);
        self.total_latency_ns.store(0, Ordering::Relaxed);
    }
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Snapshot of metrics at a point in time
#[derive(Debug, Clone, Copy)]
pub struct MetricsSnapshot {
    pub updates_received: u64,
    pub signals_generated: u64,
    pub orders_placed: u64,
    pub fills_received: u64,
    pub total_latency_ns: u64,
}

impl MetricsSnapshot {
    /// Calculate average latency per update
    pub fn avg_latency_ns(&self) -> f64 {
        if self.updates_received > 0 {
            self.total_latency_ns as f64 / self.updates_received as f64
        } else {
            0.0
        }
    }

    /// Calculate signal generation rate
    pub fn signal_rate(&self) -> f64 {
        if self.updates_received > 0 {
            self.signals_generated as f64 / self.updates_received as f64
        } else {
            0.0
        }
    }

    /// Calculate fill rate (fills per order)
    pub fn fill_rate(&self) -> f64 {
        if self.orders_placed > 0 {
            self.fills_received as f64 / self.orders_placed as f64
        } else {
            0.0
        }
    }
}

/// Cache-aligned wrapper for any type
///
/// Useful for ensuring types are on their own cache line.
#[repr(C, align(64))]
pub struct CacheAligned<T> {
    inner: T,
}

impl<T> CacheAligned<T> {
    /// Create new cache-aligned value
    pub const fn new(inner: T) -> Self {
        Self { inner }
    }

    /// Get reference to inner value
    pub fn get(&self) -> &T {
        &self.inner
    }

    /// Get mutable reference to inner value
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.inner
    }

    /// Unwrap the inner value
    pub fn into_inner(self) -> T {
        self.inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_alignment() {
        // Each counter should be on its own cache line
        assert_eq!(std::mem::align_of::<Metrics>(), 64);

        // Check that counters are actually separated
        let metrics = Metrics::new();
        let _base = &metrics as *const _ as usize;
        let updates_ptr = &metrics.updates_received as *const _ as usize;
        let signals_ptr = &metrics.signals_generated as *const _ as usize;

        // Signals should be 64 bytes away from updates
        assert_eq!(signals_ptr - updates_ptr, 64);
    }

    #[test]
    fn test_metrics_operations() {
        let metrics = Metrics::new();

        metrics.inc_updates();
        metrics.inc_updates();
        metrics.inc_signals();
        metrics.inc_orders();
        metrics.add_latency(100);

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.updates_received, 2);
        assert_eq!(snapshot.signals_generated, 1);
        assert_eq!(snapshot.orders_placed, 1);
        assert_eq!(snapshot.total_latency_ns, 100);
    }

    #[test]
    fn test_metrics_snapshot_calculations() {
        let snapshot = MetricsSnapshot {
            updates_received: 100,
            signals_generated: 50,
            orders_placed: 25,
            fills_received: 20,
            total_latency_ns: 10_000,
        };

        assert_eq!(snapshot.avg_latency_ns(), 100.0);
        assert_eq!(snapshot.signal_rate(), 0.5);
        assert_eq!(snapshot.fill_rate(), 0.8);
    }

    #[test]
    fn test_cache_aligned() {
        let aligned = CacheAligned::new(42u64);
        assert_eq!(std::mem::align_of::<CacheAligned<u64>>(), 64);
        assert_eq!(*aligned.get(), 42);
    }

    #[test]
    fn test_metrics_reset() {
        let metrics = Metrics::new();

        metrics.inc_updates();
        metrics.inc_signals();
        assert_eq!(metrics.snapshot().updates_received, 1);

        metrics.reset();
        assert_eq!(metrics.snapshot().updates_received, 0);
        assert_eq!(metrics.snapshot().signals_generated, 0);
    }
}
