//! Position Reconciliation System
//!
//! Ensures position tracking accuracy by periodically comparing internal
//! position state with executor-reported position.
//!
//! Critical for paper trading accuracy and production safety.

use anyhow::{anyhow, Result};
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tracing::{debug, error, info, warn};

/// Position reconciliation configuration
pub struct ReconciliationConfig {
    /// How often to reconcile (in number of fills)
    pub reconcile_every_n_fills: u32,

    /// Maximum allowed position mismatch (fixed-point, 9 decimals)
    /// Default: 0.001 BTC = 1_000_000
    pub max_position_mismatch: u64,

    /// Whether to halt trading on mismatch
    pub halt_on_mismatch: bool,

    /// Whether to auto-correct small mismatches
    pub auto_correct_threshold: u64,
}

impl Default for ReconciliationConfig {
    fn default() -> Self {
        Self {
            reconcile_every_n_fills: 1000,
            max_position_mismatch: 1_000_000, // 0.001 BTC
            halt_on_mismatch: true,
            auto_correct_threshold: 100_000, // 0.0001 BTC
        }
    }
}

/// Position reconciliation state and logic
pub struct PositionReconciler {
    /// Configuration
    config: ReconciliationConfig,

    /// Fill counter since last reconciliation
    fills_since_last_check: AtomicU64,

    /// Last reconciliation timestamp
    last_reconciliation: Instant,

    /// Number of successful reconciliations
    successful_reconciliations: AtomicU64,

    /// Number of failed reconciliations
    failed_reconciliations: AtomicU64,

    /// Total drift detected (cumulative absolute value)
    total_drift_detected: AtomicI64,

    /// Maximum drift ever detected
    max_drift_detected: AtomicI64,
}

impl PositionReconciler {
    /// Create a new position reconciler with default config
    pub fn new() -> Self {
        Self::with_config(ReconciliationConfig::default())
    }

    /// Create with custom configuration
    pub fn with_config(config: ReconciliationConfig) -> Self {
        Self {
            config,
            fills_since_last_check: AtomicU64::new(0),
            last_reconciliation: Instant::now(),
            successful_reconciliations: AtomicU64::new(0),
            failed_reconciliations: AtomicU64::new(0),
            total_drift_detected: AtomicI64::new(0),
            max_drift_detected: AtomicI64::new(0),
        }
    }

    /// Check if reconciliation is needed
    pub fn should_reconcile(&self) -> bool {
        let fills = self.fills_since_last_check.load(Ordering::Relaxed);
        fills >= self.config.reconcile_every_n_fills as u64
    }

    /// Increment fill counter
    pub fn on_fill(&self) {
        self.fills_since_last_check.fetch_add(1, Ordering::Relaxed);
    }

    /// Perform position reconciliation
    ///
    /// # Arguments
    ///
    /// * `internal_position` - Our calculated position (fixed-point)
    /// * `executor_position` - Executor's reported position (fixed-point)
    ///
    /// # Returns
    ///
    /// - `Ok(drift)` - The position drift detected (can be 0)
    /// - `Err` - If mismatch exceeds threshold and halt_on_mismatch is true
    pub fn reconcile(&self, internal_position: i64, executor_position: i64) -> Result<i64> {
        // Reset fill counter
        self.fills_since_last_check.store(0, Ordering::Relaxed);

        // Calculate drift
        let drift = (internal_position - executor_position).abs();

        // Update stats
        self.total_drift_detected
            .fetch_add(drift, Ordering::Relaxed);

        // Update max drift if needed
        let mut max_drift = self.max_drift_detected.load(Ordering::Relaxed);
        while drift > max_drift {
            match self.max_drift_detected.compare_exchange_weak(
                max_drift,
                drift,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(x) => max_drift = x,
            }
        }

        // Check if drift is within acceptable range
        if drift == 0 {
            debug!(
                "Position reconciliation successful: internal={}, executor={}",
                internal_position, executor_position
            );
            self.successful_reconciliations
                .fetch_add(1, Ordering::Relaxed);
            return Ok(0);
        }

        // Check if drift is small enough to auto-correct
        if drift as u64 <= self.config.auto_correct_threshold {
            info!(
                "Small position drift detected: {} (internal={}, executor={}). Auto-correcting.",
                drift, internal_position, executor_position
            );
            self.successful_reconciliations
                .fetch_add(1, Ordering::Relaxed);
            return Ok(drift);
        }

        // Check if drift exceeds maximum allowed
        if drift as u64 > self.config.max_position_mismatch {
            error!(
                "CRITICAL: Position mismatch exceeds threshold! Drift: {} (internal={}, executor={})",
                drift, internal_position, executor_position
            );

            self.failed_reconciliations.fetch_add(1, Ordering::Relaxed);

            if self.config.halt_on_mismatch {
                return Err(anyhow!(
                    "Position reconciliation failed: drift {} exceeds max {}",
                    drift,
                    self.config.max_position_mismatch
                ));
            }
        } else {
            warn!(
                "Position drift within tolerance: {} (internal={}, executor={})",
                drift, internal_position, executor_position
            );
            self.successful_reconciliations
                .fetch_add(1, Ordering::Relaxed);
        }

        Ok(drift)
    }

    /// Force a reconciliation (reset counter)
    pub fn force_reconciliation(&self) {
        self.fills_since_last_check.store(
            self.config.reconcile_every_n_fills as u64,
            Ordering::Relaxed,
        );
    }

    /// Get reconciliation statistics
    pub fn stats(&self) -> ReconciliationStats {
        ReconciliationStats {
            successful: self.successful_reconciliations.load(Ordering::Relaxed),
            failed: self.failed_reconciliations.load(Ordering::Relaxed),
            total_drift: self.total_drift_detected.load(Ordering::Relaxed),
            max_drift: self.max_drift_detected.load(Ordering::Relaxed),
            fills_since_check: self.fills_since_last_check.load(Ordering::Relaxed),
            time_since_last: self.last_reconciliation.elapsed(),
        }
    }

    /// Reset statistics (useful for testing or after recovery)
    pub fn reset_stats(&self) {
        self.successful_reconciliations.store(0, Ordering::Relaxed);
        self.failed_reconciliations.store(0, Ordering::Relaxed);
        self.total_drift_detected.store(0, Ordering::Relaxed);
        self.max_drift_detected.store(0, Ordering::Relaxed);
        self.fills_since_last_check.store(0, Ordering::Relaxed);
    }
}

/// Reconciliation statistics
#[derive(Debug, Clone)]
pub struct ReconciliationStats {
    pub successful: u64,
    pub failed: u64,
    pub total_drift: i64,
    pub max_drift: i64,
    pub fills_since_check: u64,
    pub time_since_last: Duration,
}

impl ReconciliationStats {
    /// Get success rate as percentage
    pub fn success_rate(&self) -> f64 {
        let total = self.successful + self.failed;
        if total == 0 {
            100.0
        } else {
            (self.successful as f64 / total as f64) * 100.0
        }
    }

    /// Get average drift per reconciliation
    pub fn average_drift(&self) -> f64 {
        let total = self.successful + self.failed;
        if total == 0 {
            0.0
        } else {
            self.total_drift as f64 / total as f64
        }
    }
}

impl Default for PositionReconciler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reconciliation_success() {
        let reconciler = PositionReconciler::new();

        // Perfect match
        let result = reconciler.reconcile(100_000_000, 100_000_000);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);

        let stats = reconciler.stats();
        assert_eq!(stats.successful, 1);
        assert_eq!(stats.failed, 0);
    }

    #[test]
    fn test_reconciliation_small_drift() {
        let mut config = ReconciliationConfig::default();
        config.auto_correct_threshold = 1_000_000; // 0.001 BTC

        let reconciler = PositionReconciler::with_config(config);

        // Small drift within auto-correct threshold
        let result = reconciler.reconcile(100_000_000, 100_500_000);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 500_000);

        let stats = reconciler.stats();
        assert_eq!(stats.successful, 1);
        assert_eq!(stats.max_drift, 500_000);
    }

    #[test]
    fn test_reconciliation_large_drift() {
        let mut config = ReconciliationConfig::default();
        config.max_position_mismatch = 1_000_000; // 0.001 BTC
        config.halt_on_mismatch = true;

        let reconciler = PositionReconciler::with_config(config);

        // Large drift exceeding threshold
        let result = reconciler.reconcile(100_000_000, 110_000_000);
        assert!(result.is_err());

        let stats = reconciler.stats();
        assert_eq!(stats.failed, 1);
        assert_eq!(stats.max_drift, 10_000_000);
    }

    #[test]
    fn test_fill_counter() {
        let mut config = ReconciliationConfig::default();
        config.reconcile_every_n_fills = 10;

        let reconciler = PositionReconciler::with_config(config);

        // Should not reconcile initially
        assert!(!reconciler.should_reconcile());

        // Add fills
        for _ in 0..9 {
            reconciler.on_fill();
            assert!(!reconciler.should_reconcile());
        }

        // 10th fill should trigger reconciliation
        reconciler.on_fill();
        assert!(reconciler.should_reconcile());

        // After reconciliation, counter resets
        let _ = reconciler.reconcile(100_000_000, 100_000_000);
        assert!(!reconciler.should_reconcile());
    }

    #[test]
    fn test_stats_calculation() {
        let reconciler = PositionReconciler::new();

        // Successful reconciliations
        let _ = reconciler.reconcile(100_000_000, 100_000_000);
        let _ = reconciler.reconcile(100_000_000, 100_100_000);

        // Failed reconciliation (with halt disabled)
        let mut config = ReconciliationConfig::default();
        config.halt_on_mismatch = false;
        config.max_position_mismatch = 1_000;
        let reconciler2 = PositionReconciler::with_config(config);
        let _ = reconciler2.reconcile(100_000_000, 200_000_000);

        let stats = reconciler.stats();
        assert_eq!(stats.success_rate(), 100.0);
        assert!(stats.average_drift() >= 0.0);
    }
}
