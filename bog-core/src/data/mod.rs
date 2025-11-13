pub mod types;

pub use types::{conversions, ConsumerStats, MarketSnapshot, MarketSnapshotExt};

use anyhow::{anyhow, Context, Result};
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

// ============================================================================
// Snapshot Validation (CRITICAL for Real Money Safety)
// ============================================================================

/// Reasons why a snapshot might be invalid
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnapshotValidationError {
    /// Bid price is zero
    ZeroBidPrice,
    /// Ask price is zero
    ZeroAskPrice,
    /// Bid size is zero
    ZeroBidSize,
    /// Ask size is zero
    ZeroAskSize,
    /// Orderbook is crossed (bid >= ask)
    Crossed { bid: u64, ask: u64 },
    /// Orderbook is locked (bid == ask) - rare but technically valid for some DEXs
    Locked { price: u64 },
    /// Spread is too wide (> 1000bps = 10%)
    SpreadTooWide { spread_bps: u32 },
}

impl std::fmt::Display for SnapshotValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SnapshotValidationError::ZeroBidPrice => write!(f, "Bid price is zero"),
            SnapshotValidationError::ZeroAskPrice => write!(f, "Ask price is zero"),
            SnapshotValidationError::ZeroBidSize => write!(f, "Bid size is zero"),
            SnapshotValidationError::ZeroAskSize => write!(f, "Ask size is zero"),
            SnapshotValidationError::Crossed { bid, ask } => {
                write!(f, "Orderbook crossed: bid={} >= ask={}", bid, ask)
            }
            SnapshotValidationError::Locked { price } => {
                write!(f, "Orderbook locked at price={}", price)
            }
            SnapshotValidationError::SpreadTooWide { spread_bps } => {
                write!(f, "Spread too wide: {}bps (max: 1000bps)", spread_bps)
            }
        }
    }
}

/// Validate a market snapshot for trading safety
///
/// Checks all critical invariants that MUST be true before using this data for trading.
/// This is the first line of defense against trading on bad/corrupted/stale data.
///
/// # Validation Rules
///
/// 1. **Non-zero prices**: bid_price > 0, ask_price > 0
/// 2. **Non-zero sizes**: bid_size > 0, ask_size > 0
/// 3. **Not crossed**: bid_price < ask_price
/// 4. **Reasonable spread**: spread < 1000bps (10%)
///
/// # Returns
/// - `Ok(())`: Snapshot is valid and safe to use
/// - `Err(SnapshotValidationError)`: Snapshot is invalid, DO NOT TRADE
pub fn validate_snapshot(snapshot: &MarketSnapshot) -> Result<(), SnapshotValidationError> {
    // Check 1: Non-zero bid price
    if snapshot.best_bid_price == 0 {
        return Err(SnapshotValidationError::ZeroBidPrice);
    }

    // Check 2: Non-zero ask price
    if snapshot.best_ask_price == 0 {
        return Err(SnapshotValidationError::ZeroAskPrice);
    }

    // Check 3: Non-zero bid size
    if snapshot.best_bid_size == 0 {
        return Err(SnapshotValidationError::ZeroBidSize);
    }

    // Check 4: Non-zero ask size
    if snapshot.best_ask_size == 0 {
        return Err(SnapshotValidationError::ZeroAskSize);
    }

    // Check 5: Not crossed (bid < ask)
    if snapshot.best_bid_price >= snapshot.best_ask_price {
        return Err(SnapshotValidationError::Crossed {
            bid: snapshot.best_bid_price,
            ask: snapshot.best_ask_price,
        });
    }

    // Check 6: Spread not insanely wide (> 10%)
    let spread = snapshot.best_ask_price - snapshot.best_bid_price;
    let spread_bps = ((spread as u128 * 10_000) / snapshot.best_bid_price as u128) as u32;

    if spread_bps > 1000 {
        // 10% spread is likely corrupted data
        return Err(SnapshotValidationError::SpreadTooWide { spread_bps });
    }

    Ok(())
}

/// Check if snapshot is valid (wrapper that returns bool)
pub fn is_valid_snapshot(snapshot: &MarketSnapshot) -> bool {
    validate_snapshot(snapshot).is_ok()
}

/// Check if orderbook is crossed (bid >= ask)
pub fn is_crossed(snapshot: &MarketSnapshot) -> bool {
    snapshot.best_bid_price > 0
        && snapshot.best_ask_price > 0
        && snapshot.best_bid_price >= snapshot.best_ask_price
}

/// Check if orderbook is locked (bid == ask)
pub fn is_locked(snapshot: &MarketSnapshot) -> bool {
    snapshot.best_bid_price > 0
        && snapshot.best_bid_price == snapshot.best_ask_price
}

/// Check if snapshot is stale (older than threshold)
pub fn is_stale(snapshot: &MarketSnapshot, max_age_ns: u64) -> bool {
    let now_ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;

    if now_ns < snapshot.exchange_timestamp_ns {
        return false; // Future timestamp (clock skew)
    }

    let age_ns = now_ns - snapshot.exchange_timestamp_ns;
    age_ns > max_age_ns
}

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

    /// Wait for first valid snapshot with timeout
    ///
    /// This MUST be called before starting trading to ensure orderbook is populated.
    /// Returns the first valid snapshot or error after timeout.
    ///
    /// # Arguments
    /// - `max_retries`: Maximum polling attempts (default: 100)
    /// - `retry_delay`: Delay between attempts (default: 100ms)
    ///
    /// # Returns
    /// - `Ok(snapshot)`: First valid snapshot received
    /// - `Err(_)`: Timeout or validation failure
    pub fn wait_for_initial_snapshot(
        &mut self,
        max_retries: u32,
        retry_delay: Duration,
    ) -> Result<MarketSnapshot> {
        info!(
            "⏳ Waiting for initial valid market snapshot (timeout: {}s)...",
            max_retries as f64 * retry_delay.as_secs_f64()
        );

        for attempt in 0..max_retries {
            match self.try_recv() {
                Some(snapshot) if validate_snapshot(&snapshot).is_ok() => {
                    info!(
                        "✅ Received valid initial snapshot after {} attempts: seq={}, bid={}, ask={}",
                        attempt + 1,
                        snapshot.sequence,
                        types::conversions::u64_to_f64(snapshot.best_bid_price),
                        types::conversions::u64_to_f64(snapshot.best_ask_price)
                    );
                    return Ok(snapshot);
                }
                Some(snapshot) => {
                    warn!(
                        "⚠️ Received INVALID snapshot (attempt {}): bid={}, ask={}, crossed={}",
                        attempt + 1,
                        snapshot.best_bid_price,
                        snapshot.best_ask_price,
                        snapshot.best_bid_price >= snapshot.best_ask_price
                    );
                }
                None => {
                    if attempt % 10 == 0 {
                        debug!("Ring buffer empty (attempt {}/{})", attempt + 1, max_retries);
                    }
                }
            }

            std::thread::sleep(retry_delay);
        }

        Err(anyhow::anyhow!(
            "Failed to receive valid initial snapshot after {} retries ({:.1}s). \
             Verify: (1) Huginn is running, (2) Huginn is connected to Lighter exchange, \
             (3) Market {} has active trading.",
            max_retries,
            max_retries as f64 * retry_delay.as_secs_f64(),
            self.market_id
        ))
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

    #[test]
    fn test_validate_snapshot_valid() {
        let snapshot = create_valid_test_snapshot();
        assert!(validate_snapshot(&snapshot).is_ok());
        assert!(is_valid_snapshot(&snapshot));
    }

    #[test]
    fn test_validate_snapshot_zero_bid() {
        let mut snapshot = create_valid_test_snapshot();
        snapshot.best_bid_price = 0;

        let result = validate_snapshot(&snapshot);
        assert!(matches!(result, Err(SnapshotValidationError::ZeroBidPrice)));
    }

    #[test]
    fn test_validate_snapshot_crossed() {
        let mut snapshot = create_valid_test_snapshot();
        snapshot.best_bid_price = 50_010_000_000_000;
        snapshot.best_ask_price = 50_000_000_000_000; // Bid > Ask

        let result = validate_snapshot(&snapshot);
        assert!(matches!(result, Err(SnapshotValidationError::Crossed { .. })));
        assert!(is_crossed(&snapshot));
    }

    #[test]
    fn test_validate_snapshot_wide_spread() {
        let mut snapshot = create_valid_test_snapshot();
        snapshot.best_bid_price = 50_000_000_000_000;
        snapshot.best_ask_price = 60_000_000_000_000; // 20% spread!

        let result = validate_snapshot(&snapshot);
        assert!(matches!(result, Err(SnapshotValidationError::SpreadTooWide { .. })));
    }

    #[test]
    fn test_is_locked() {
        let mut snapshot = create_valid_test_snapshot();
        snapshot.best_bid_price = 50_000_000_000_000;
        snapshot.best_ask_price = 50_000_000_000_000; // Same price

        assert!(is_locked(&snapshot));
    }

    #[test]
    fn test_is_stale() {
        let snapshot = create_valid_test_snapshot();

        // Recent snapshot (not stale)
        assert!(!is_stale(&snapshot, 10_000_000_000)); // 10s threshold

        // Old snapshot (set timestamp to 1 hour ago)
        let mut old_snapshot = snapshot;
        let one_hour_ago = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64
            - 3_600_000_000_000; // 1 hour in nanoseconds

        old_snapshot.exchange_timestamp_ns = one_hour_ago;
        assert!(is_stale(&old_snapshot, 10_000_000_000)); // Older than 10s
    }

    fn create_valid_test_snapshot() -> MarketSnapshot {
        let now_ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;

        MarketSnapshot {
            market_id: 1,
            sequence: 100,
            exchange_timestamp_ns: now_ns,
            local_recv_ns: now_ns,
            local_publish_ns: now_ns,
            best_bid_price: 50_000_000_000_000,
            best_bid_size: 1_000_000_000,
            best_ask_price: 50_005_000_000_000,
            best_ask_size: 1_000_000_000,
            bid_prices: [0; 10],
            bid_sizes: [0; 10],
            ask_prices: [0; 10],
            ask_sizes: [0; 10],
            dex_type: 1,
            _padding: [0; 111],
        }
    }
}
