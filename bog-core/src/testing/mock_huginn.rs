//! Mock Huginn market data feed for testing
//!
//! Provides a programmable mock implementation of huginn::MarketFeed
//! for integration testing without requiring a real Huginn instance.

use huginn::{ConsumerStats, MarketSnapshot};
use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Mock implementation of Huginn MarketFeed for testing
pub struct MockHuginnFeed {
    market_id: u64,
    snapshots: VecDeque<MarketSnapshot>,
    stats: ConsumerStats,
    last_sequence: u64,
    inject_sequence_gap: bool,
    inject_latency: Option<Duration>,
    start_time: Instant,
}

impl MockHuginnFeed {
    /// Create a new mock feed for a market
    pub fn new(market_id: u64) -> Self {
        Self {
            market_id,
            snapshots: VecDeque::new(),
            stats: ConsumerStats {
                total_reads: 0,
                successful_reads: 0,
                empty_reads: 0,
                sequence_gaps: 0,
                last_sequence: 0,
            },
            last_sequence: 0,
            inject_sequence_gap: false,
            inject_latency: None,
            start_time: Instant::now(),
        }
    }

    /// Add a snapshot to the mock feed queue
    pub fn push_snapshot(&mut self, snapshot: MarketSnapshot) {
        self.snapshots.push_back(snapshot);
    }

    /// Add multiple snapshots at once
    pub fn push_snapshots(&mut self, snapshots: Vec<MarketSnapshot>) {
        for snapshot in snapshots {
            self.snapshots.push_back(snapshot);
        }
    }

    /// Generate a test snapshot with specified parameters
    pub fn generate_snapshot(
        &mut self,
        bid_price: u64,
        ask_price: u64,
        bid_size: u64,
        ask_size: u64,
    ) -> MarketSnapshot {
        self.last_sequence += 1;

        // Inject sequence gap if configured
        if self.inject_sequence_gap {
            self.last_sequence += 10;
            self.inject_sequence_gap = false;
        }

        MarketSnapshot {
            market_id: self.market_id,
            sequence: self.last_sequence,
            exchange_timestamp_ns: self.start_time.elapsed().as_nanos() as u64,
            best_bid_price: bid_price,
            best_ask_price: ask_price,
            best_bid_size: bid_size,
            best_ask_size: ask_size,
            bid_prices: [0; 10],
            bid_sizes: [0; 10],
            ask_prices: [0; 10],
            ask_sizes: [0; 10],
        }
    }

    /// Generate and push a test snapshot
    pub fn push_test_snapshot(
        &mut self,
        bid_price: u64,
        ask_price: u64,
        bid_size: u64,
        ask_size: u64,
    ) {
        let snapshot = self.generate_snapshot(bid_price, ask_price, bid_size, ask_size);
        self.push_snapshot(snapshot);
    }

    /// Configure the mock to inject a sequence gap on next snapshot
    pub fn inject_sequence_gap_next(&mut self) {
        self.inject_sequence_gap = true;
    }

    /// Configure simulated latency for each try_recv call
    pub fn set_latency(&mut self, latency: Duration) {
        self.inject_latency = Some(latency);
    }

    /// Clear simulated latency
    pub fn clear_latency(&mut self) {
        self.inject_latency = None;
    }

    /// Get number of pending snapshots
    pub fn pending_count(&self) -> usize {
        self.snapshots.len()
    }

    /// Clear all pending snapshots
    pub fn clear(&mut self) {
        self.snapshots.clear();
    }

    // Implement MarketFeed interface

    /// Mock try_recv - returns next snapshot from queue
    pub fn try_recv(&mut self) -> Option<MarketSnapshot> {
        // Simulate latency if configured
        if let Some(latency) = self.inject_latency {
            std::thread::sleep(latency);
        }

        self.stats.total_reads += 1;

        if let Some(snapshot) = self.snapshots.pop_front() {
            // Check for sequence gap
            if self.stats.last_sequence > 0 && snapshot.sequence > self.stats.last_sequence + 1 {
                self.stats.sequence_gaps += 1;
            }

            self.stats.successful_reads += 1;
            self.stats.last_sequence = snapshot.sequence;
            Some(snapshot)
        } else {
            self.stats.empty_reads += 1;
            None
        }
    }

    /// Get consumer statistics
    pub fn stats(&self) -> &ConsumerStats {
        &self.stats
    }

    /// Reset statistics
    pub fn reset_stats(&mut self) {
        self.stats = ConsumerStats {
            total_reads: 0,
            successful_reads: 0,
            empty_reads: 0,
            sequence_gaps: 0,
            last_sequence: self.stats.last_sequence,
        };
    }

    /// Get market ID
    pub fn market_id(&self) -> u64 {
        self.market_id
    }

    /// Get queue depth (number of pending snapshots)
    pub fn queue_depth(&self) -> usize {
        self.snapshots.len()
    }

    /// Check if caught up (no pending snapshots)
    pub fn is_caught_up(&self) -> bool {
        self.snapshots.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_feed_creation() {
        let feed = MockHuginnFeed::new(1);
        assert_eq!(feed.market_id(), 1);
        assert_eq!(feed.queue_depth(), 0);
        assert!(feed.is_caught_up());
    }

    #[test]
    fn test_push_and_receive() {
        let mut feed = MockHuginnFeed::new(1);

        feed.push_test_snapshot(50000_000000000, 50005_000000000, 1_000000000, 1_000000000);
        assert_eq!(feed.queue_depth(), 1);
        assert!(!feed.is_caught_up());

        let snapshot = feed.try_recv();
        assert!(snapshot.is_some());
        let snapshot = snapshot.unwrap();
        assert_eq!(snapshot.best_bid_price, 50000_000000000);
        assert_eq!(snapshot.best_ask_price, 50005_000000000);
        assert_eq!(snapshot.sequence, 1);

        assert_eq!(feed.queue_depth(), 0);
        assert!(feed.is_caught_up());
    }

    #[test]
    fn test_empty_recv() {
        let mut feed = MockHuginnFeed::new(1);

        let snapshot = feed.try_recv();
        assert!(snapshot.is_none());
        assert_eq!(feed.stats().empty_reads, 1);
    }

    #[test]
    fn test_sequence_gap_detection() {
        let mut feed = MockHuginnFeed::new(1);

        // Normal snapshot
        feed.push_test_snapshot(50000_000000000, 50005_000000000, 1_000000000, 1_000000000);
        feed.try_recv();

        // Inject gap
        feed.inject_sequence_gap_next();
        feed.push_test_snapshot(50000_000000000, 50005_000000000, 1_000000000, 1_000000000);
        feed.try_recv();

        // Should detect the gap
        assert_eq!(feed.stats().sequence_gaps, 1);
        assert_eq!(feed.stats().last_sequence, 12); // 1 + 10 (gap) + 1
    }

    #[test]
    fn test_statistics() {
        let mut feed = MockHuginnFeed::new(1);

        // Add 3 snapshots
        for _ in 0..3 {
            feed.push_test_snapshot(50000_000000000, 50005_000000000, 1_000000000, 1_000000000);
        }

        // Receive all
        for _ in 0..3 {
            feed.try_recv();
        }

        // Try one more (empty)
        feed.try_recv();

        let stats = feed.stats();
        assert_eq!(stats.total_reads, 4);
        assert_eq!(stats.successful_reads, 3);
        assert_eq!(stats.empty_reads, 1);
        assert_eq!(stats.read_success_rate(), 0.75);
    }

    #[test]
    fn test_reset_stats() {
        let mut feed = MockHuginnFeed::new(1);

        feed.push_test_snapshot(50000_000000000, 50005_000000000, 1_000000000, 1_000000000);
        feed.try_recv();

        assert_eq!(feed.stats().total_reads, 1);

        feed.reset_stats();
        assert_eq!(feed.stats().total_reads, 0);
        assert_eq!(feed.stats().successful_reads, 0);
    }

    #[test]
    fn test_latency_injection() {
        let mut feed = MockHuginnFeed::new(1);
        feed.set_latency(Duration::from_millis(10));

        feed.push_test_snapshot(50000_000000000, 50005_000000000, 1_000000000, 1_000000000);

        let start = Instant::now();
        feed.try_recv();
        let elapsed = start.elapsed();

        assert!(elapsed >= Duration::from_millis(10));
    }

    #[test]
    fn test_clear() {
        let mut feed = MockHuginnFeed::new(1);

        for _ in 0..5 {
            feed.push_test_snapshot(50000_000000000, 50005_000000000, 1_000000000, 1_000000000);
        }

        assert_eq!(feed.pending_count(), 5);
        feed.clear();
        assert_eq!(feed.pending_count(), 0);
    }
}
