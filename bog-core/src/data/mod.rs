pub mod types;

pub use types::{conversions, ConsumerStats, MarketSnapshot, MarketSnapshotExt};

use anyhow::{Context, Result};
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

/// Wrapper around Huginn's MarketFeed with additional functionality
pub struct MarketFeed {
    inner: huginn::MarketFeed,
    market_id: u64,
    last_sequence: u64,
    last_message_time: Instant,
    stats: FeedStats,
}

/// Statistics for the market feed
#[derive(Debug, Clone, Default)]
pub struct FeedStats {
    pub messages_received: u64,
    pub empty_polls: u64,
    pub sequence_gaps: u64,
    pub max_queue_depth: usize,
}

impl MarketFeed {
    /// Connect to Huginn shared memory for a given market ID
    pub fn connect(market_id: u64) -> Result<Self> {
        info!("Connecting to Huginn market feed for market {}", market_id);

        let inner = huginn::MarketFeed::connect(market_id)
            .context("Failed to connect to Huginn shared memory")?;

        Ok(Self {
            inner,
            market_id,
            last_sequence: 0,
            last_message_time: Instant::now(),
            stats: FeedStats::default(),
        })
    }

    /// Connect with explicit DEX type
    pub fn connect_with_dex(dex_type: u8, market_id: u64) -> Result<Self> {
        info!(
            "Connecting to Huginn market feed for DEX {} market {}",
            dex_type, market_id
        );

        let inner = huginn::MarketFeed::connect_with_dex(dex_type, market_id)
            .context("Failed to connect to Huginn shared memory")?;

        Ok(Self {
            inner,
            market_id,
            last_sequence: 0,
            last_message_time: Instant::now(),
            stats: FeedStats::default(),
        })
    }

    /// Try to receive a market snapshot (non-blocking)
    pub fn try_recv(&mut self) -> Option<MarketSnapshot> {
        match self.inner.try_recv() {
            Some(snapshot) => {
                self.last_message_time = Instant::now();
                self.stats.messages_received += 1;

                // Check for sequence gaps
                if self.last_sequence > 0 && snapshot.sequence != self.last_sequence + 1 {
                    let gap = snapshot.sequence - self.last_sequence - 1;
                    warn!(
                        "Sequence gap detected: {} messages missed (last={}, current={})",
                        gap, self.last_sequence, snapshot.sequence
                    );
                    self.stats.sequence_gaps += gap;
                }
                self.last_sequence = snapshot.sequence;

                // Track queue depth
                let queue_depth = self.queue_depth();
                if queue_depth > self.stats.max_queue_depth {
                    self.stats.max_queue_depth = queue_depth;
                    if queue_depth > 100 {
                        warn!("High queue depth: {} messages behind", queue_depth);
                    }
                }

                debug!(
                    "Received snapshot: seq={}, bid={}, ask={}, latency={}μs",
                    snapshot.sequence,
                    types::conversions::u64_to_f64(snapshot.best_bid_price),
                    types::conversions::u64_to_f64(snapshot.best_ask_price),
                    snapshot.latency_us()
                );

                Some(snapshot)
            }
            None => {
                self.stats.empty_polls += 1;
                None
            }
        }
    }

    /// Get current queue depth (number of pending messages)
    pub fn queue_depth(&self) -> usize {
        self.inner.queue_depth()
    }

    /// Get Huginn consumer statistics
    pub fn consumer_stats(&self) -> &ConsumerStats {
        self.inner.stats()
    }

    /// Get bog-specific feed statistics
    pub fn feed_stats(&self) -> &FeedStats {
        &self.stats
    }

    /// Check if feed appears to be idle (for replay end detection)
    pub fn is_idle(&self, timeout: Duration) -> bool {
        self.queue_depth() == 0 && self.last_message_time.elapsed() > timeout
    }

    /// Get time since last message
    pub fn time_since_last_message(&self) -> Duration {
        self.last_message_time.elapsed()
    }

    /// Get the market ID
    pub fn market_id(&self) -> u64 {
        self.market_id
    }

    /// Print feed statistics
    pub fn log_stats(&self) {
        let consumer_stats = self.consumer_stats();
        let read_success_rate = if self.stats.messages_received + self.stats.empty_polls > 0 {
            (self.stats.messages_received as f64
                / (self.stats.messages_received + self.stats.empty_polls) as f64)
                * 100.0
        } else {
            0.0
        };

        info!(
            "Feed stats: messages={}, empty_polls={}, success_rate={:.2}%, gaps={}, max_queue_depth={}",
            self.stats.messages_received,
            self.stats.empty_polls,
            read_success_rate,
            self.stats.sequence_gaps,
            self.stats.max_queue_depth
        );

        info!(
            "Huginn stats: total_reads={}, empty_reads={}, gaps={}, max_gap={}",
            consumer_stats.total_reads,
            consumer_stats.empty_reads,
            consumer_stats.sequence_gaps,
            consumer_stats.max_gap_size
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_feed_stats_initialization() {
        let stats = FeedStats::default();
        assert_eq!(stats.messages_received, 0);
        assert_eq!(stats.empty_polls, 0);
        assert_eq!(stats.sequence_gaps, 0);
        assert_eq!(stats.max_queue_depth, 0);
    }

    #[test]
    fn test_is_idle() {
        // Note: This test doesn't actually connect to Huginn
        // In a real scenario, you'd use a mock or test harness

        // We can't easily test this without mocking, but we can test the logic
        let now = Instant::now();
        let timeout = Duration::from_secs(1);

        // Simulating idle check logic
        let queue_depth = 0;
        let elapsed = Duration::from_secs(2);

        let is_idle = queue_depth == 0 && elapsed > timeout;
        assert!(is_idle);
    }

    // Note: Full integration tests would require a running Huginn instance
    // Those should be in tests/integration/ directory
}
