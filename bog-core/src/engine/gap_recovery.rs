//! Automatic Gap Recovery System
//!
//! Handles sequence gaps in market data feeds with automatic resynchronization.
//! Critical for maintaining data integrity during network issues or Huginn restarts.

use crate::data::{MarketFeed, MarketSnapshot};
use anyhow::{anyhow, Result};
use std::time::{Duration, Instant};
use tracing::{debug, error, info, warn};

/// Configuration for automatic gap recovery
#[derive(Debug, Clone)]
pub struct GapRecoveryConfig {
    /// Enable automatic recovery on gap detection
    pub auto_recover: bool,

    /// Maximum gap size to attempt recovery (larger gaps may indicate major issues)
    pub max_recoverable_gap: u64,

    /// Timeout for snapshot fetch during recovery
    pub snapshot_timeout: Duration,

    /// Maximum recovery attempts before giving up
    pub max_recovery_attempts: u32,

    /// Delay between recovery attempts
    pub recovery_retry_delay: Duration,

    /// Pause trading during recovery
    pub pause_trading_during_recovery: bool,

    /// Alert on gap detection (even if recovered)
    pub alert_on_gap: bool,
}

impl Default for GapRecoveryConfig {
    fn default() -> Self {
        Self {
            auto_recover: true,
            max_recoverable_gap: 10000,               // Up to 10k messages
            snapshot_timeout: Duration::from_secs(2), // Reduced from 30s - with request_snapshot(), Huginn responds in <1s
            max_recovery_attempts: 10,                // Increased from 3 for more resilience
            recovery_retry_delay: Duration::from_millis(100), // Reduced from 500ms - faster retries to prevent buffer fill during waits
            pause_trading_during_recovery: true,
            alert_on_gap: true,
        }
    }
}

/// Gap recovery state and statistics
pub struct GapRecoveryManager {
    config: GapRecoveryConfig,
    total_gaps_detected: u64,
    total_gaps_recovered: u64,
    total_gaps_failed: u64,
    largest_gap_recovered: u64,
    last_recovery_time: Option<Instant>,
    recovery_in_progress: bool,
    consecutive_failures: u32,
}

impl GapRecoveryManager {
    /// Create a new gap recovery manager
    pub fn new(config: GapRecoveryConfig) -> Self {
        Self {
            config,
            total_gaps_detected: 0,
            total_gaps_recovered: 0,
            total_gaps_failed: 0,
            largest_gap_recovered: 0,
            last_recovery_time: None,
            recovery_in_progress: false,
            consecutive_failures: 0,
        }
    }

    /// Check if recovery is currently in progress
    pub fn is_recovering(&self) -> bool {
        self.recovery_in_progress
    }

    /// Check if trading should be paused
    pub fn should_pause_trading(&self) -> bool {
        self.recovery_in_progress && self.config.pause_trading_during_recovery
    }

    /// Handle a detected gap with automatic recovery
    ///
    /// # Arguments
    /// - `feed`: The market feed with the gap
    /// - `gap_size`: Number of missed messages
    /// - `last_seq`: Last known good sequence number
    /// - `current_seq`: Current sequence number (after gap)
    ///
    /// # Returns
    /// - `Ok(Some(snapshot))`: Gap recovered, returns recovery snapshot
    /// - `Ok(None)`: Gap ignored (auto_recover disabled or gap too large)
    /// - `Err(_)`: Recovery failed after all attempts
    pub fn handle_gap(
        &mut self,
        feed: &mut MarketFeed,
        gap_size: u64,
        last_seq: u64,
        current_seq: u64,
    ) -> Result<Option<MarketSnapshot>> {
        self.total_gaps_detected += 1;

        // Alert if configured
        if self.config.alert_on_gap {
            warn!(
                "SEQUENCE GAP DETECTED: {} messages missed ({}→{})",
                gap_size, last_seq, current_seq
            );
        }

        // Check if gap is too large to recover
        if gap_size > self.config.max_recoverable_gap {
            error!(
                "Gap too large for automatic recovery: {} > {} max",
                gap_size, self.config.max_recoverable_gap
            );
            self.total_gaps_failed += 1;
            self.consecutive_failures += 1;
            return Err(anyhow!(
                "Gap of {} messages exceeds recovery limit of {}",
                gap_size,
                self.config.max_recoverable_gap
            ));
        }

        // Check if automatic recovery is disabled
        if !self.config.auto_recover {
            warn!("Automatic gap recovery disabled, manual intervention required");
            return Ok(None);
        }

        // Start recovery
        self.recovery_in_progress = true;
        info!(
            "Starting automatic gap recovery (attempt 1/{})",
            self.config.max_recovery_attempts
        );

        let start_time = Instant::now();
        let mut last_error = None;

        // Try recovery with retries
        for attempt in 1..=self.config.max_recovery_attempts {
            match self.attempt_recovery(feed, current_seq) {
                Ok(snapshot) => {
                    // Success!
                    let recovery_time = start_time.elapsed();
                    self.recovery_in_progress = false;
                    self.total_gaps_recovered += 1;
                    self.consecutive_failures = 0;
                    self.last_recovery_time = Some(Instant::now());

                    if gap_size > self.largest_gap_recovered {
                        self.largest_gap_recovered = gap_size;
                    }

                    info!(
                        "Gap recovery successful! Recovered {} messages in {:.3}s (attempt {}/{})",
                        gap_size,
                        recovery_time.as_secs_f64(),
                        attempt,
                        self.config.max_recovery_attempts
                    );

                    return Ok(Some(snapshot));
                }
                Err(e) => {
                    last_error = Some(e);

                    if attempt < self.config.max_recovery_attempts {
                        warn!(
                            "Gap recovery attempt {} failed, retrying in {:?}...",
                            attempt, self.config.recovery_retry_delay
                        );
                        std::thread::sleep(self.config.recovery_retry_delay);
                    }
                }
            }
        }

        // All attempts failed
        self.recovery_in_progress = false;
        self.total_gaps_failed += 1;
        self.consecutive_failures += 1;

        let error_msg = format!(
            "Gap recovery failed after {} attempts: {}",
            self.config.max_recovery_attempts,
            last_error
                .map(|e| e.to_string())
                .unwrap_or_else(|| "Unknown error".to_string())
        );

        error!("{}", error_msg);
        Err(anyhow!(error_msg))
    }

    /// Attempt a single recovery
    ///
    /// # Arguments
    /// - `feed`: The market feed to recover
    /// - `current_seq`: The sequence number that triggered the gap (received_seq)
    fn attempt_recovery(&self, feed: &mut MarketFeed, current_seq: u64) -> Result<MarketSnapshot> {
        debug!("Saving current position for recovery...");
        let checkpoint = feed.save_position();

        // CRITICAL FIX: Request snapshot from Huginn BEFORE waiting for it
        // This increments an atomic counter that Huginn's snapshot polling task monitors
        info!("REQUESTING snapshot from Huginn (atomic signal via shared memory)...");
        feed.request_snapshot();

        info!(
            "Waiting for snapshot from Huginn (timeout: {:?})...",
            self.config.snapshot_timeout
        );
        let snapshot = feed
            .fetch_snapshot(Some(self.config.snapshot_timeout))
            .ok_or_else(|| {
                anyhow!(
                "Snapshot timeout after {:?}. Check Huginn logs for 'Snapshot request detected'.",
                self.config.snapshot_timeout
            )
            })?;

        info!(
            "Snapshot received successfully: seq={}",
            snapshot.sequence
        );

        // Rewind to checkpoint to replay any buffered messages
        debug!("Rewinding to checkpoint for message replay...");
        feed.rewind_to(checkpoint);

        // CRITICAL FIX (Bug #11): Use current_seq instead of snapshot.sequence for gap detector reset
        //
        // The exchange returns CACHED snapshots with OLD sequence numbers (can be 5000+ behind).
        // If we reset the gap detector to the snapshot's old sequence, the next live message
        // will immediately trigger another gap (since live stream is far ahead).
        //
        // Solution: Reset gap detector to (current_seq - 1) so the current message is accepted.
        // The snapshot's orderbook data is still valid - we just ignore its stale sequence.
        //
        // Example:
        //   - Gap detected: expected 1,882,792, received 1,882,809
        //   - Snapshot fetched with seq 1,875,988 (stale!)
        //   - OLD behavior: reset to 1,875,988 → next message at 1,882,809 → gap of 6821!
        //   - NEW behavior: reset to 1,882,808 → next message at 1,882,809 → no gap!
        let reset_seq = current_seq.saturating_sub(1);
        info!(
            "Gap detector reset: using current_seq-1={} (snapshot had stale seq={})",
            reset_seq, snapshot.sequence
        );
        feed.mark_recovery_complete(reset_seq);

        Ok(snapshot)
    }

    /// Get recovery statistics
    pub fn stats(&self) -> GapRecoveryStats {
        GapRecoveryStats {
            total_gaps_detected: self.total_gaps_detected,
            total_gaps_recovered: self.total_gaps_recovered,
            total_gaps_failed: self.total_gaps_failed,
            largest_gap_recovered: self.largest_gap_recovered,
            consecutive_failures: self.consecutive_failures,
            recovery_success_rate: if self.total_gaps_detected > 0 {
                (self.total_gaps_recovered as f64 / self.total_gaps_detected as f64) * 100.0
            } else {
                100.0
            },
            time_since_last_recovery: self
                .last_recovery_time
                .map(|t| t.elapsed())
                .unwrap_or(Duration::from_secs(0)),
        }
    }

    /// Reset statistics (useful after reconnection)
    pub fn reset_stats(&mut self) {
        self.total_gaps_detected = 0;
        self.total_gaps_recovered = 0;
        self.total_gaps_failed = 0;
        self.largest_gap_recovered = 0;
        self.consecutive_failures = 0;
        self.last_recovery_time = None;
    }

    /// Check if we should give up (too many consecutive failures)
    pub fn should_abandon(&self) -> bool {
        self.consecutive_failures >= 5
    }
}

/// Gap recovery statistics
#[derive(Debug, Clone)]
pub struct GapRecoveryStats {
    pub total_gaps_detected: u64,
    pub total_gaps_recovered: u64,
    pub total_gaps_failed: u64,
    pub largest_gap_recovered: u64,
    pub consecutive_failures: u32,
    pub recovery_success_rate: f64,
    pub time_since_last_recovery: Duration,
}

impl GapRecoveryStats {
    /// Log statistics
    pub fn log(&self) {
        info!(
            "Gap Recovery Stats: detected={}, recovered={}, failed={}, success_rate={:.1}%, largest_gap={}, consecutive_failures={}",
            self.total_gaps_detected,
            self.total_gaps_recovered,
            self.total_gaps_failed,
            self.recovery_success_rate,
            self.largest_gap_recovered,
            self.consecutive_failures
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gap_recovery_config_defaults() {
        let config = GapRecoveryConfig::default();
        assert!(config.auto_recover);
        assert_eq!(config.max_recoverable_gap, 10000);
        assert!(config.pause_trading_during_recovery);
        assert!(config.alert_on_gap);
    }

    #[test]
    fn test_gap_recovery_manager_initialization() {
        let manager = GapRecoveryManager::new(GapRecoveryConfig::default());
        assert!(!manager.is_recovering());
        assert!(!manager.should_pause_trading());
        assert_eq!(manager.total_gaps_detected, 0);
        assert_eq!(manager.total_gaps_recovered, 0);
    }

    #[test]
    fn test_should_abandon_after_failures() {
        let mut manager = GapRecoveryManager::new(GapRecoveryConfig::default());
        assert!(!manager.should_abandon());

        manager.consecutive_failures = 5;
        assert!(manager.should_abandon());
    }

    #[test]
    fn test_stats_calculation() {
        let mut manager = GapRecoveryManager::new(GapRecoveryConfig::default());
        manager.total_gaps_detected = 10;
        manager.total_gaps_recovered = 8;
        manager.total_gaps_failed = 2;
        manager.largest_gap_recovered = 1000;

        let stats = manager.stats();
        assert_eq!(stats.total_gaps_detected, 10);
        assert_eq!(stats.total_gaps_recovered, 8);
        assert_eq!(stats.total_gaps_failed, 2);
        assert_eq!(stats.recovery_success_rate, 80.0);
        assert_eq!(stats.largest_gap_recovered, 1000);
    }
}
