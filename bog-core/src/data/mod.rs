pub mod constants;
pub mod snapshot_builder;
pub mod types;
pub mod validator;

pub use constants::{ORDERBOOK_DEPTH, PADDING_SIZE, SNAPSHOT_SIZE};
pub use snapshot_builder::{create_realistic_depth_snapshot, SnapshotBuilder};
pub use types::{conversions, ConsumerStats, MarketSnapshot, MarketSnapshotExt};
pub use validator::{SnapshotValidator, ValidationConfig, ValidationError};

use crate::resilience::{FeedHealth, GapDetector, HealthConfig, StaleDataBreaker, StaleDataConfig};
use anyhow::{Context, Result};
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
    /// Snapshot is too old (stale data)
    StaleData { age_ns: u64, max_age_ns: u64 },
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
            SnapshotValidationError::StaleData { age_ns, max_age_ns } => {
                write!(
                    f,
                    "Snapshot is stale: age={}ms > max={}ms",
                    age_ns / 1_000_000,
                    max_age_ns / 1_000_000
                )
            }
        }
    }
}

/// Validate a market snapshot for trading safety (with cached time to avoid syscalls)
///
/// This version uses a provided current time to avoid syscalls in hot paths.
///
/// # Arguments
/// - `snapshot`: The market snapshot to validate
/// - `max_age_ns`: Maximum acceptable age in nanoseconds
/// - `now_ns`: Current time in nanoseconds (cached to avoid syscalls)
///
/// # Returns
/// - `Ok(())`: Snapshot is valid and safe to use
/// - `Err(SnapshotValidationError)`: Snapshot is invalid, DO NOT TRADE
pub fn validate_snapshot_with_time(
    snapshot: &MarketSnapshot,
    max_age_ns: u64,
    now_ns: u64,
) -> Result<(), SnapshotValidationError> {
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

    // Check 7: Not stale (data age within acceptable threshold) - using cached time
    if is_stale_with_time(snapshot, max_age_ns, now_ns) {
        let age_ns = now_ns.saturating_sub(snapshot.exchange_timestamp_ns);

        return Err(SnapshotValidationError::StaleData { age_ns, max_age_ns });
    }

    Ok(())
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
/// 5. **Not stale**: age <= max_age_ns
///
/// # Arguments
/// - `snapshot`: The market snapshot to validate
/// - `max_age_ns`: Maximum acceptable age in nanoseconds (e.g., 5_000_000_000 for 5 seconds)
///
/// # Returns
/// - `Ok(())`: Snapshot is valid and safe to use
/// - `Err(SnapshotValidationError)`: Snapshot is invalid, DO NOT TRADE
///
/// # Note
/// This function performs a syscall to get current time. For hot paths with frequent validation,
/// consider using `validate_snapshot_with_time` with a cached timestamp.
pub fn validate_snapshot(
    snapshot: &MarketSnapshot,
    max_age_ns: u64,
) -> Result<(), SnapshotValidationError> {
    // Get current time for validation
    let now_ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;

    // Delegate to cached version
    validate_snapshot_with_time(snapshot, max_age_ns, now_ns)
}

/// Check if snapshot is valid (wrapper that returns bool)
///
/// Uses a default max age of 5 seconds for staleness check.
pub fn is_valid_snapshot(snapshot: &MarketSnapshot) -> bool {
    const DEFAULT_MAX_AGE_NS: u64 = 5_000_000_000; // 5 seconds
    validate_snapshot(snapshot, DEFAULT_MAX_AGE_NS).is_ok()
}

/// Check if orderbook is crossed (bid >= ask)
pub fn is_crossed(snapshot: &MarketSnapshot) -> bool {
    snapshot.best_bid_price > 0
        && snapshot.best_ask_price > 0
        && snapshot.best_bid_price >= snapshot.best_ask_price
}

/// Check if orderbook is locked (bid == ask)
pub fn is_locked(snapshot: &MarketSnapshot) -> bool {
    snapshot.best_bid_price > 0 && snapshot.best_bid_price == snapshot.best_ask_price
}

/// Check if snapshot is stale (older than threshold) using a provided current time
/// This version avoids syscalls by accepting a cached timestamp
pub fn is_stale_with_time(snapshot: &MarketSnapshot, max_age_ns: u64, now_ns: u64) -> bool {
    if now_ns < snapshot.exchange_timestamp_ns {
        return false; // Future timestamp (clock skew)
    }

    let age_ns = now_ns - snapshot.exchange_timestamp_ns;
    age_ns > max_age_ns
}

/// Check if snapshot is stale (older than threshold)
/// Note: This performs a syscall. Consider using is_stale_with_time with a cached timestamp for hot paths.
pub fn is_stale(snapshot: &MarketSnapshot, max_age_ns: u64) -> bool {
    let now_ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;

    is_stale_with_time(snapshot, max_age_ns, now_ns)
}

trait SnapshotOps {
    fn save_position(&self) -> u64;
    fn request_snapshot(&mut self);
    fn fetch_snapshot(&mut self, timeout: Option<Duration>) -> Option<MarketSnapshot>;
    fn rewind_to(&mut self, position: u64);
}

fn perform_snapshot_recovery(
    feed: &mut impl SnapshotOps,
    timeout: Duration,
) -> Result<MarketSnapshot> {
    let checkpoint = feed.save_position();
    feed.request_snapshot();

    let snapshot = feed.fetch_snapshot(Some(timeout)).ok_or_else(|| {
        anyhow::anyhow!(
            "Snapshot timeout after {:.1}s. Verify the Huginn snapshot task is running and reachable.",
            timeout.as_secs_f64()
        )
    })?;

    feed.rewind_to(checkpoint);
    Ok(snapshot)
}

/// Configuration for epoch change monitoring
#[derive(Debug, Clone)]
pub struct EpochCheckConfig {
    /// Check epoch every N messages (0 = disabled)
    pub check_interval_messages: u64,
    /// Check epoch every M milliseconds (0 = disabled)
    pub check_interval_ms: u64,
}

impl Default for EpochCheckConfig {
    fn default() -> Self {
        Self {
            check_interval_messages: 1000, // Check every 1000 messages
            check_interval_ms: 100,        // Check every 100ms
        }
    }
}

/// Configuration for adaptive backpressure handling
#[derive(Debug, Clone)]
pub struct BackpressureConfig {
    /// Queue depth threshold to trigger backpressure (0 = disabled)
    pub queue_depth_threshold: usize,
    /// Throttle strategy when backpressured
    pub throttle_strategy: ThrottleStrategy,
}

/// Throttle strategy for backpressure
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ThrottleStrategy {
    /// Sleep for calculated duration
    Sleep,
    /// Yield CPU (spin_loop hint)
    Yield,
    /// Skip messages to reduce backlog
    Skip,
    /// No throttling
    Disabled,
}

impl Default for BackpressureConfig {
    fn default() -> Self {
        Self {
            queue_depth_threshold: 1000,
            throttle_strategy: ThrottleStrategy::Yield,
        }
    }
}

/// Wrapper around Huginn's MarketFeed with additional functionality
pub struct MarketFeed {
    inner: huginn::MarketFeed,
    market_id: u64,
    last_sequence: u64,
    last_message_time: Instant,
    stats: FeedStats,
    gap_detector: GapDetector,
    recovery_in_progress: bool,
    stale_breaker: StaleDataBreaker,
    health: FeedHealth,
    last_epoch: u64,
    epoch_check_counter: u64,
    epoch_check_config: EpochCheckConfig,
    last_epoch_check_time: Instant,
    backpressure_config: BackpressureConfig,
    backpressure_triggered: bool,
    /// Cached current time in nanoseconds (updated every 100ms to avoid syscalls)
    cached_time_ns: u64,
    /// When the time cache was last updated
    last_time_cache_update: Instant,
}

/// Statistics for the market feed
#[derive(Debug, Clone, Default)]
pub struct FeedStats {
    pub messages_received: u64,
    pub empty_polls: u64,
    pub sequence_gaps: u64,
    pub max_queue_depth: usize,
    pub epoch_changes: u64,
    pub messages_skipped: u64,
    pub skipped_bytes: u64,
    pub backpressure_events: u64,
    pub total_throttle_ms: u64,
}

impl MarketFeed {
    /// Connect to Huginn shared memory for a given encoded market ID
    ///
    /// # Arguments
    /// - `market_id`: Encoded market ID (dex_type * 1_000_000 + raw_market_id)
    ///               Example: 1_000_001 for Lighter market 1
    ///
    /// # Recommended
    /// Use [`connect_with_dex()`](#method.connect_with_dex) for better readability
    pub fn connect(market_id: types::EncodedMarketId) -> Result<Self> {
        info!("Connecting to Huginn market feed for market {}", market_id);

        let inner = huginn::MarketFeed::connect(market_id)
            .context("Failed to connect to Huginn shared memory")?;

        // Get initial epoch from huginn
        let initial_epoch = inner.epoch();

        // Initialize time cache to avoid syscalls in hot path
        let now_ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;

        Ok(Self {
            inner,
            market_id,
            last_sequence: 0,
            last_message_time: Instant::now(),
            stats: FeedStats::default(),
            gap_detector: GapDetector::new(),
            recovery_in_progress: false,
            stale_breaker: StaleDataBreaker::new(StaleDataConfig::default()),
            health: FeedHealth::new(HealthConfig::default()),
            last_epoch: initial_epoch,
            epoch_check_counter: 0,
            epoch_check_config: EpochCheckConfig::default(),
            last_epoch_check_time: Instant::now(),
            backpressure_config: BackpressureConfig::default(),
            backpressure_triggered: false,
            cached_time_ns: now_ns,
            last_time_cache_update: Instant::now(),
        })
    }

    /// Connect with explicit DEX type and raw market ID (recommended)
    ///
    /// # Arguments
    /// - `dex_type`: DEX identifier (1 = Lighter, 2 = Binance, etc.)
    /// - `market_id`: Raw DEX-specific market ID (e.g., 1, 2, 3...)
    ///
    /// Internally encodes to: `(dex_type * 1_000_000) + market_id`
    ///
    /// # Example
    /// ```ignore
    /// let feed = MarketFeed::connect_with_dex(1, 1)?; // Lighter market 1
    /// ```
    pub fn connect_with_dex(dex_type: u8, market_id: types::RawMarketId) -> Result<Self> {
        info!(
            "Connecting to Huginn market feed for DEX {} market {}",
            dex_type, market_id
        );

        let inner = huginn::MarketFeed::connect_with_dex(dex_type, market_id)
            .context("Failed to connect to Huginn shared memory")?;

        // Get initial epoch from huginn
        let initial_epoch = inner.epoch();

        // Initialize time cache to avoid syscalls in hot path
        let now_ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;

        Ok(Self {
            inner,
            market_id,
            last_sequence: 0,
            last_message_time: Instant::now(),
            stats: FeedStats::default(),
            gap_detector: GapDetector::new(),
            recovery_in_progress: false,
            stale_breaker: StaleDataBreaker::new(StaleDataConfig::default()),
            health: FeedHealth::new(HealthConfig::default()),
            last_epoch: initial_epoch,
            epoch_check_counter: 0,
            epoch_check_config: EpochCheckConfig::default(),
            last_epoch_check_time: Instant::now(),
            backpressure_config: BackpressureConfig::default(),
            backpressure_triggered: false,
            cached_time_ns: now_ns,
            last_time_cache_update: Instant::now(),
        })
    }

    /// Try to receive a market snapshot (non-blocking)
    pub fn try_recv(&mut self) -> Option<MarketSnapshot> {
        // Update time cache periodically to avoid syscalls in hot path (every 100ms)
        if self.last_time_cache_update.elapsed().as_millis() >= 100 {
            self.cached_time_ns = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos() as u64;
            self.last_time_cache_update = Instant::now();
        }

        match self.inner.try_recv() {
            Some(snapshot) => {
                self.last_message_time = Instant::now();
                self.stats.messages_received += 1;

                // Mark data as fresh for stale data detector
                self.stale_breaker.mark_fresh();
                self.health.report_message(snapshot.sequence);

                // Check for sequence gaps using wraparound-safe detector
                let gap_size = self.gap_detector.check(snapshot.sequence);
                if gap_size > 0 {
                    debug!(
                        "Sequence gap detected: {} messages missed (last={}, current={}) - recovery required",
                        gap_size, self.last_sequence, snapshot.sequence
                    );
                    self.stats.sequence_gaps += gap_size;
                    self.recovery_in_progress = true;
                    // Gap recovery would be triggered here by returning a special signal
                    // to the engine to initiate snapshot recovery
                }
                self.last_sequence = snapshot.sequence;

                // Track queue depth
                let queue_depth = self.queue_depth();
                if queue_depth > self.stats.max_queue_depth {
                    self.stats.max_queue_depth = queue_depth;
                    if queue_depth > 100 {
                        debug!("High queue depth: {} messages behind", queue_depth);
                    }
                }

                #[cfg(feature = "debug-logging")]
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

                // Mark empty poll for stale data detection
                self.stale_breaker.mark_empty_poll();
                self.health.report_empty_poll();

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

    /// Check if data is fresh (not stale or offline)
    pub fn is_data_fresh(&self) -> bool {
        self.stale_breaker.is_fresh()
    }

    /// Get stale data breaker state
    pub fn stale_state(&self) -> crate::resilience::StaleDataState {
        self.stale_breaker.state()
    }

    /// Get feed health status
    pub fn health_status(&self) -> crate::resilience::HealthStatus {
        self.health.status()
    }

    /// Check if feed is ready to trade
    pub fn is_ready(&self) -> bool {
        self.health.is_ready()
    }

    /// Get health information (for monitoring)
    pub fn health_info(&self) -> (crate::resilience::HealthStatus, u64, std::time::Duration) {
        (
            self.health.status(),
            self.health.message_count(),
            self.health.uptime(),
        )
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

        const MAX_DATA_AGE_NS: u64 = 5_000_000_000; // 5 seconds

        for attempt in 0..max_retries {
            match self.try_recv() {
                Some(snapshot) if validate_snapshot(&snapshot, MAX_DATA_AGE_NS).is_ok() => {
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
                        debug!(
                            "Ring buffer empty (attempt {}/{})",
                            attempt + 1,
                            max_retries
                        );
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

    // ========================================================================
    // SNAPSHOT PROTOCOL - Fast initialization and recovery
    // ========================================================================

    /// Save current read position for snapshot protocol
    ///
    /// Call this before requesting a snapshot. After snapshot arrival,
    /// call rewind_to() with this value to replay messages that arrived
    /// during the snapshot fetch.
    ///
    /// # Returns
    /// Current position in ring buffer
    ///
    /// # Example
    /// ```ignore
    /// let checkpoint = feed.save_position();
    /// feed.request_snapshot()?;
    /// // ... wait for snapshot ...
    /// feed.rewind_to(checkpoint)?;
    /// ```
    pub fn save_position(&self) -> u64 {
        self.inner.save_position()
    }

    /// Wait for and fetch a full snapshot from the ring buffer
    ///
    /// Blocks until a full snapshot arrives in the ring buffer, skipping
    /// over any incremental updates. Use this after detecting a gap or
    /// during initialization to get a complete orderbook state.
    ///
    /// # Arguments
    /// - `timeout`: Optional timeout duration. If None, returns immediately.
    ///
    /// # Returns
    /// - `Some(snapshot)`: A full snapshot was received
    /// - `None`: Timeout expired or no timeout provided and no snapshot available
    ///
    /// # Performance
    /// Blocks for up to the timeout duration
    pub fn fetch_snapshot(&mut self, timeout: Option<Duration>) -> Option<MarketSnapshot> {
        self.inner.fetch_snapshot(timeout)
    }

    /// Request a full orderbook snapshot from Huginn
    ///
    /// CRITICAL: This MUST be called before fetch_snapshot() for the snapshot
    /// protocol to work correctly. This increments an atomic counter in shared
    /// memory that Huginn's snapshot polling task checks every 100ms.
    ///
    /// # How It Works
    /// 1. This method increments snapshot_request_counter (atomic, ~10ns)
    /// 2. Huginn's snapshot polling task sees the counter change
    /// 3. Huginn spawns async task to fetch snapshot via temporary WebSocket
    /// 4. Huginn publishes snapshot to ring buffer with IS_FULL_SNAPSHOT flag
    /// 5. Your fetch_snapshot() call receives it
    ///
    /// # Rate Limiting
    /// Huginn enforces 1 snapshot per 10 seconds to protect the exchange.
    ///
    /// # Example
    /// ```ignore
    /// // Correct usage:
    /// feed.request_snapshot();                          // Signal Huginn
    /// let snap = feed.fetch_snapshot(Some(Duration::from_secs(2)))?;  // Wait for it
    ///
    /// // Wrong usage:
    /// let snap = feed.fetch_snapshot(Some(Duration::from_secs(30)))?;  // Times out!
    /// // Never requested, so Huginn never fetches, so fetch_snapshot waits forever
    /// ```
    ///
    /// # Performance
    /// ~10ns (single atomic increment in shared memory)
    pub fn request_snapshot(&mut self) {
        self.inner.request_snapshot()
    }

    /// Rewind consumer to a previous position for replay
    ///
    /// Used after snapshot fetch to replay updates that arrived while
    /// the snapshot was being fetched. Must be called before reading
    /// to see the replayed updates.
    ///
    /// # Arguments
    /// - `position`: Value returned by save_position()
    ///
    /// # Note
    /// Position becomes invalid if the ring buffer wraps around (typically ~1-2 seconds).
    /// No error is returned if the position is stale - the rewind will fail silently
    /// and the consumer will read from the current head position instead.
    ///
    /// # Performance
    /// <10ns (atomic write)
    pub fn rewind_to(&mut self, position: u64) {
        self.inner.rewind_to(position);
    }

    /// Peek at next message without advancing position
    ///
    /// Non-blocking check to see if data is available. Does NOT advance
    /// the consumer position, unlike try_recv().
    ///
    /// # Returns
    /// - `Some(snapshot)`: Next message is available
    /// - `None`: No new messages
    ///
    /// # Performance
    /// 50-150ns (same as try_recv)
    pub fn peek(&self) -> Option<MarketSnapshot> {
        self.inner.peek()
    }

    /// Initialize with full snapshot (fast initialization)
    ///
    /// Complete initialization flow that uses snapshot protocol:
    /// 1. Save current position (checkpoint)
    /// 2. Request full snapshot from Huginn
    /// 3. Wait for snapshot (with timeout)
    /// 4. Rewind to checkpoint
    /// 5. Replay incremental updates
    /// 6. Return complete state
    ///
    /// # Arguments
    /// - `timeout`: Maximum time to wait for snapshot (typically 5-10 seconds)
    ///
    /// # Returns
    /// - `Ok(snapshot)`: Full snapshot received and replayed
    /// - `Err(_)`: Timeout or rewind failed
    ///
    /// # Performance
    /// Initialization should complete in <1 second (vs 10 seconds with polling)
    pub fn initialize_with_snapshot(&mut self, timeout: Duration) -> Result<MarketSnapshot> {
        info!(
            "⏳ Initializing with snapshot protocol (timeout: {:.1}s)...",
            timeout.as_secs_f64()
        );

        let start = Instant::now();

        let snapshot = perform_snapshot_recovery(self, timeout)?;

        info!(
            "✅ Snapshot received: seq={}, bid={}, ask={}, took {:.3}s",
            snapshot.sequence,
            types::conversions::u64_to_f64(snapshot.best_bid_price),
            types::conversions::u64_to_f64(snapshot.best_ask_price),
            start.elapsed().as_secs_f64()
        );

        Ok(snapshot)
    }

    /// Get producer epoch (detects Huginn restarts)
    ///
    /// Epoch is incremented each time Huginn restarts. Can be used to detect
    /// when producer has been restarted and may need reconnection.
    ///
    /// # Example
    /// ```ignore
    /// let initial_epoch = feed.epoch();
    /// loop {
    ///     if feed.epoch() != initial_epoch {
    ///         eprintln!("Huginn restarted, reconnecting...");
    ///         // Reconnect
    ///     }
    /// }
    /// ```
    pub fn epoch(&self) -> u64 {
        self.inner.epoch()
    }

    /// Set the epoch check configuration
    ///
    /// # Arguments
    /// - `config`: Epoch check configuration (check interval in messages and time)
    pub fn set_epoch_check_config(&mut self, config: EpochCheckConfig) {
        self.epoch_check_config = config;
    }

    /// Check if epoch has changed (Huginn restarted) and update tracking
    ///
    /// Epoch increments each time Huginn restarts. This method detects changes
    /// and logs them for operational awareness.
    ///
    /// Checking respects the configured interval (messages and/or time based).
    ///
    /// # Returns
    /// - `true`: Epoch changed (Huginn restarted)
    /// - `false`: Epoch unchanged or check skipped due to interval
    ///
    /// # Performance
    /// <10ns (single u64 comparison, when check is skipped)
    /// <50ns (full check, when interval is met)
    pub fn check_epoch_change(&mut self) -> bool {
        // Check if we should skip this check based on interval
        let should_check = {
            let message_check = self.epoch_check_config.check_interval_messages > 0
                && self.epoch_check_counter >= self.epoch_check_config.check_interval_messages;

            let time_check = self.epoch_check_config.check_interval_ms > 0
                && self.last_epoch_check_time.elapsed().as_millis()
                    >= self.epoch_check_config.check_interval_ms as u128;

            message_check || time_check
        };

        if !should_check {
            self.epoch_check_counter += 1;
            return false;
        }

        // Time to check - read current epoch
        let current_epoch = self.inner.epoch();

        if current_epoch != self.last_epoch {
            warn!(
                "🔄 Huginn restart detected! Epoch changed: {} → {} (market_id={}) - triggering snapshot recovery",
                self.last_epoch, current_epoch, self.market_id
            );

            // Automatically trigger snapshot recovery on epoch change
            info!("AUTO-RECOVERY: Requesting snapshot due to epoch change");
            self.request_snapshot();
            self.recovery_in_progress = true;

            self.last_epoch = current_epoch;
            self.stats.epoch_changes += 1;
            self.epoch_check_counter = 0;
            self.last_epoch_check_time = Instant::now();
            true
        } else {
            self.epoch_check_counter = 0;
            self.last_epoch_check_time = Instant::now();
            false
        }
    }

    // ========================================================================
    // Conditional Processing (Peek-based skipping)
    // ========================================================================

    /// Peek at the next message to decide if it should be processed
    ///
    /// Useful for conditional processing - skip redundant updates when not trading,
    /// or skip incremental updates when waiting for snapshots.
    ///
    /// # Example
    /// ```ignore
    /// if let Some(snapshot) = feed.peek() {
    ///     if !snapshot.is_full_snapshot() && !is_trading() {
    ///         // Skip incremental updates while waiting for full snapshot
    ///         feed.try_recv();  // Consume without processing
    ///         continue;
    ///     }
    /// }
    /// ```
    pub fn peek_next(&self) -> Option<MarketSnapshot> {
        self.peek()
    }

    /// Skip the next message if it matches a condition
    ///
    /// Returns true if message was skipped (and consumed), false if processed.
    ///
    /// # Arguments
    /// - `skip_condition`: Closure returning true if message should be skipped
    ///
    /// # Returns
    /// - `true`: Message was skipped and consumed
    /// - `false`: Message should be processed
    pub fn skip_if<F>(&mut self, skip_condition: F) -> bool
    where
        F: Fn(&MarketSnapshot) -> bool,
    {
        if let Some(snapshot) = self.peek() {
            if skip_condition(&snapshot) {
                // Consume the message without processing
                let _ = self.try_recv();
                const SNAPSHOT_SIZE_BYTES: u64 = 512;
                self.stats.messages_skipped += 1;
                self.stats.skipped_bytes += SNAPSHOT_SIZE_BYTES;
                return true;
            }
        }
        false
    }

    /// Get the skip rate (percentage of messages skipped)
    pub fn skip_rate(&self) -> f64 {
        let total = self.stats.messages_received + self.stats.messages_skipped;
        if total == 0 {
            0.0
        } else {
            (self.stats.messages_skipped as f64 / total as f64) * 100.0
        }
    }

    // ========================================================================
    // Backpressure Handling
    // ========================================================================

    /// Set the backpressure configuration
    ///
    /// # Arguments
    /// - `config`: Backpressure configuration (threshold and throttle strategy)
    pub fn set_backpressure_config(&mut self, config: BackpressureConfig) {
        self.backpressure_config = config;
    }

    /// Check and handle backpressure based on queue depth
    ///
    /// Monitors the queue depth and applies throttling if configured threshold is exceeded.
    ///
    /// # Returns
    /// - `true`: Backpressure is active (consumer behind on processing)
    /// - `false`: Queue is normal
    ///
    /// # Performance
    /// <50ns (single queue_depth check + comparison)
    pub fn check_backpressure(&mut self) -> bool {
        if self.backpressure_config.queue_depth_threshold == 0 {
            return false; // Backpressure disabled
        }

        let queue_depth = self.queue_depth();
        let threshold = self.backpressure_config.queue_depth_threshold;
        let now_triggered = queue_depth > threshold;

        // Detect transition to backpressure
        if now_triggered && !self.backpressure_triggered {
            warn!(
                "⚠️ Backpressure triggered! Queue depth {} > threshold {}",
                queue_depth, threshold
            );
            self.stats.backpressure_events += 1;
            self.backpressure_triggered = true;
        }

        // Release backpressure when recovered
        if !now_triggered && self.backpressure_triggered {
            info!(
                "✅ Backpressure released. Queue depth normalized to {}",
                queue_depth
            );
            self.backpressure_triggered = false;
        }

        // Apply throttling if needed
        if now_triggered {
            match self.backpressure_config.throttle_strategy {
                ThrottleStrategy::Sleep => {
                    // Calculate throttle duration based on how far behind we are
                    let backlog = queue_depth - threshold;
                    let throttle_ms = ((backlog as f64 / 100.0).min(50.0)) as u64;
                    self.stats.total_throttle_ms += throttle_ms;
                    std::thread::sleep(std::time::Duration::from_millis(throttle_ms));
                }
                ThrottleStrategy::Yield => {
                    // Just yield CPU, let other threads run
                    std::hint::spin_loop();
                }
                ThrottleStrategy::Skip => {
                    // Skip next message to reduce backlog
                    let _ = self.try_recv();
                }
                ThrottleStrategy::Disabled => {
                    // No throttling, even though backpressured
                }
            }
        }

        now_triggered
    }

    /// Get backpressure status
    pub fn is_backpressured(&self) -> bool {
        self.backpressure_triggered
    }

    /// Get backpressure percentage (how often backpressured)
    pub fn backpressure_percentage(&self) -> f64 {
        if self.stats.backpressure_events == 0 {
            0.0
        } else {
            (self.stats.backpressure_events as f64 / self.stats.messages_received.max(1) as f64)
                * 100.0
        }
    }

    // ========================================================================
    // Gap Recovery
    // ========================================================================

    /// Check if a gap recovery is currently in progress
    pub fn is_recovery_in_progress(&self) -> bool {
        self.recovery_in_progress
    }

    /// Get the last detected gap size
    pub fn last_gap_size(&self) -> u64 {
        self.gap_detector.last_gap_size()
    }

    /// Check if a gap was detected
    pub fn gap_detected(&self) -> bool {
        self.gap_detector.gap_detected()
    }

    /// Mark recovery as complete and reset gap detector
    ///
    /// Called after successful snapshot recovery to resume normal operation
    pub fn mark_recovery_complete(&mut self, sequence: u64) {
        self.recovery_in_progress = false;
        self.gap_detector.reset_at_sequence(sequence);
        info!(
            "Gap recovery completed, resuming normal operation from sequence {}",
            sequence
        );
    }

    /// Trigger automatic gap recovery with snapshot resync
    ///
    /// This is the main recovery flow:
    /// 1. Save position
    /// 2. Request snapshot
    /// 3. Wait for snapshot (with timeout)
    /// 4. Rewind to saved position
    /// 5. Replay any buffered messages
    /// 6. Resume from recovered state
    pub fn recover_from_gap(&mut self, timeout: Duration) -> Result<()> {
        if !self.gap_detected() {
            return Ok(());
        }

        info!(
            "Initiating gap recovery: {} messages missed at sequence {}",
            self.last_gap_size(),
            self.gap_detector.last_sequence()
        );

        // Use the snapshot protocol to recover
        let snapshot = perform_snapshot_recovery(self, timeout)?;

        let reset_sequence = if self.gap_detector.last_sequence() > 0 {
            self.gap_detector.last_sequence().saturating_sub(1)
        } else {
            snapshot.sequence.saturating_sub(1)
        };

        self.mark_recovery_complete(reset_sequence);

        info!(
            "Gap recovery successful, snapshot seq={} (reset to {})",
            snapshot.sequence, reset_sequence
        );

        Ok(())
    }
}

impl SnapshotOps for MarketFeed {
    fn save_position(&self) -> u64 {
        self.inner.save_position()
    }

    fn request_snapshot(&mut self) {
        self.inner.request_snapshot();
    }

    fn fetch_snapshot(&mut self, timeout: Option<Duration>) -> Option<MarketSnapshot> {
        self.inner.fetch_snapshot(timeout)
    }

    fn rewind_to(&mut self, position: u64) {
        self.inner.rewind_to(position);
    }
}

#[cfg(test)]
mod snapshot_recovery_tests {
    use super::*;

    struct MockSnapshotOps {
        saved_position: u64,
        requested: bool,
        rewound: Vec<u64>,
        snapshot: Option<MarketSnapshot>,
    }

    impl MockSnapshotOps {
        fn new(saved_position: u64, snapshot: Option<MarketSnapshot>) -> Self {
            Self {
                saved_position,
                requested: false,
                rewound: Vec::new(),
                snapshot,
            }
        }
    }

    impl SnapshotOps for MockSnapshotOps {
        fn save_position(&self) -> u64 {
            self.saved_position
        }

        fn request_snapshot(&mut self) {
            self.requested = true;
        }

        fn fetch_snapshot(&mut self, _timeout: Option<Duration>) -> Option<MarketSnapshot> {
            self.snapshot.take()
        }

        fn rewind_to(&mut self, position: u64) {
            self.rewound.push(position);
        }
    }

    fn dummy_snapshot(sequence: u64) -> MarketSnapshot {
        MarketSnapshot {
            market_id: 1,
            sequence,
            exchange_timestamp_ns: 0,
            local_recv_ns: 0,
            local_publish_ns: 0,
            best_bid_price: 50_000_000_000_000,
            best_bid_size: 1_000_000_000,
            best_ask_price: 50_005_000_000_000,
            best_ask_size: 1_000_000_000,
            bid_prices: [0; 10],
            bid_sizes: [0; 10],
            ask_prices: [0; 10],
            ask_sizes: [0; 10],
            snapshot_flags: 0,
            dex_type: 1,
            _padding: [0; 54],
        }
    }

    #[test]
    fn test_perform_snapshot_recovery_requests_and_rewinds() {
        let mut mock = MockSnapshotOps::new(42, Some(dummy_snapshot(123)));
        let snapshot = perform_snapshot_recovery(&mut mock, Duration::from_secs(1)).unwrap();

        assert!(mock.requested);
        assert_eq!(mock.rewound, vec![42]);
        assert_eq!(snapshot.sequence, 123);
    }

    #[test]
    fn test_perform_snapshot_recovery_errors_on_timeout() {
        let mut mock = MockSnapshotOps::new(99, None);
        let err = perform_snapshot_recovery(&mut mock, Duration::from_secs(1)).unwrap_err();

        assert!(mock.requested);
        assert!(err.to_string().contains("Snapshot timeout"));
        assert!(mock.rewound.is_empty());
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
        let _now = Instant::now();
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
        const MAX_AGE: u64 = 5_000_000_000; // 5 seconds
        assert!(validate_snapshot(&snapshot, MAX_AGE).is_ok());
        assert!(is_valid_snapshot(&snapshot));
    }

    #[test]
    fn test_validate_snapshot_zero_bid() {
        let mut snapshot = create_valid_test_snapshot();
        snapshot.best_bid_price = 0;

        const MAX_AGE: u64 = 5_000_000_000;
        let result = validate_snapshot(&snapshot, MAX_AGE);
        assert!(matches!(result, Err(SnapshotValidationError::ZeroBidPrice)));
    }

    #[test]
    fn test_validate_snapshot_crossed() {
        let mut snapshot = create_valid_test_snapshot();
        snapshot.best_bid_price = 50_010_000_000_000;
        snapshot.best_ask_price = 50_000_000_000_000; // Bid > Ask

        const MAX_AGE: u64 = 5_000_000_000;
        let result = validate_snapshot(&snapshot, MAX_AGE);
        assert!(matches!(
            result,
            Err(SnapshotValidationError::Crossed { .. })
        ));
        assert!(is_crossed(&snapshot));
    }

    #[test]
    fn test_validate_snapshot_wide_spread() {
        let mut snapshot = create_valid_test_snapshot();
        snapshot.best_bid_price = 50_000_000_000_000;
        snapshot.best_ask_price = 60_000_000_000_000; // 20% spread!

        const MAX_AGE: u64 = 5_000_000_000;
        let result = validate_snapshot(&snapshot, MAX_AGE);
        assert!(matches!(
            result,
            Err(SnapshotValidationError::SpreadTooWide { .. })
        ));
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

        SnapshotBuilder::new()
            .market_id(1)
            .sequence(100)
            .timestamp(now_ns)
            .best_bid(50_000_000_000_000, 1_000_000_000)
            .best_ask(50_005_000_000_000, 1_000_000_000)
            .incremental_snapshot()
            .build()
    }
}
