//! Production-grade MarketSnapshot builder
//!
//! Eliminates ALL hardcoded array initializations and provides a fluent API
//! for creating test snapshots with proper depth configuration.
//!
//! # Design Goals
//!
//! 1. **Zero hardcoded values**: All arrays sized by `ORDERBOOK_DEPTH`
//! 2. **Type safety**: Compile-time guarantees for depth configuration
//! 3. **Ergonomic API**: Fluent builder pattern for readability
//! 4. **Production ready**: Used in both tests and runtime code
//!
//! # Example
//!
//! ```rust
//! use bog_core::data::snapshot_builder::SnapshotBuilder;
//!
//! // Simple incremental snapshot
//! let snapshot = SnapshotBuilder::new()
//!     .market_id(1000025)
//!     .sequence(12345)
//!     .best_bid(100_000_000_000, 1_000_000_000)
//!     .best_ask(100_100_000_000, 1_000_000_000)
//!     .incremental_snapshot()
//!     .build();
//!
//! // Full snapshot with depth
//! let bid_prices = vec![100_000_000_000, 99_900_000_000];
//! let bid_sizes = vec![1_000_000_000, 2_000_000_000];
//! let ask_prices = vec![100_100_000_000, 100_200_000_000];
//! let ask_sizes = vec![1_000_000_000, 2_000_000_000];
//!
//! let snapshot = SnapshotBuilder::new()
//!     .with_depth(&bid_prices, &bid_sizes, &ask_prices, &ask_sizes);
//! ```

use super::constants::{ORDERBOOK_DEPTH, PADDING_SIZE};
use huginn::shm::MarketSnapshot;

/// Builder for MarketSnapshot with zero hardcoded values
///
/// This builder ensures all snapshots are created with proper array sizing
/// based on the compile-time `ORDERBOOK_DEPTH` configuration.
#[derive(Debug, Clone)]
pub struct SnapshotBuilder {
    market_id: u64,
    sequence: u64,
    exchange_timestamp_ns: u64,
    local_recv_ns: u64,
    local_publish_ns: u64,
    best_bid_price: u64,
    best_bid_size: u64,
    best_ask_price: u64,
    best_ask_size: u64,
    snapshot_flags: u8,
    dex_type: u8,
}

impl Default for SnapshotBuilder {
    fn default() -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;

        Self {
            market_id: 1000001,
            sequence: 1,
            exchange_timestamp_ns: now,
            local_recv_ns: now,
            local_publish_ns: now,
            best_bid_price: 100_000_000_000, // $100 in fixed-point
            best_bid_size: 1_000_000_000,    // 1.0 BTC
            best_ask_price: 100_100_000_000, // $100.10 (10bps spread)
            best_ask_size: 1_000_000_000,
            snapshot_flags: 0, // Incremental by default
            dex_type: 1,       // Lighter DEX
        }
    }
}

impl SnapshotBuilder {
    /// Create a new snapshot builder with sensible defaults
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the market ID
    pub fn market_id(mut self, market_id: u64) -> Self {
        self.market_id = market_id;
        self
    }

    /// Set the sequence number
    pub fn sequence(mut self, sequence: u64) -> Self {
        self.sequence = sequence;
        self
    }

    /// Set best bid price and size
    pub fn best_bid(mut self, price: u64, size: u64) -> Self {
        self.best_bid_price = price;
        self.best_bid_size = size;
        self
    }

    /// Set best ask price and size
    pub fn best_ask(mut self, price: u64, size: u64) -> Self {
        self.best_ask_price = price;
        self.best_ask_size = size;
        self
    }

    /// Mark this as a full snapshot (all depth levels populated)
    pub fn full_snapshot(mut self) -> Self {
        self.snapshot_flags = 1;
        self
    }

    /// Mark this as an incremental snapshot (only best bid/ask updated)
    pub fn incremental_snapshot(mut self) -> Self {
        self.snapshot_flags = 0;
        self
    }

    /// Set all timestamp fields to the same value
    pub fn timestamp(mut self, timestamp_ns: u64) -> Self {
        self.exchange_timestamp_ns = timestamp_ns;
        self.local_recv_ns = timestamp_ns;
        self.local_publish_ns = timestamp_ns;
        self
    }

    /// Set DEX type (1 = Lighter, 2 = Binance, etc.)
    pub fn dex_type(mut self, dex_type: u8) -> Self {
        self.dex_type = dex_type;
        self
    }

    /// Build the snapshot with all depth arrays initialized to zero
    ///
    /// This is the standard build method - all depth arrays will be zero-filled.
    /// Use this for incremental snapshots or when depth data isn't needed.
    pub fn build(self) -> MarketSnapshot {
        MarketSnapshot {
            // SeqLock generation counters (0 = not yet published via ring buffer)
            generation_start: 0,
            generation_end: 0,  // Adjacent to generation_start to avoid alignment padding
            market_id: self.market_id,
            sequence: self.sequence,
            exchange_timestamp_ns: self.exchange_timestamp_ns,
            local_recv_ns: self.local_recv_ns,
            local_publish_ns: self.local_publish_ns,
            best_bid_price: self.best_bid_price,
            best_bid_size: self.best_bid_size,
            best_ask_price: self.best_ask_price,
            best_ask_size: self.best_ask_size,
            bid_prices: [0; ORDERBOOK_DEPTH], // NO HARDCODING
            bid_sizes: [0; ORDERBOOK_DEPTH],  // NO HARDCODING
            ask_prices: [0; ORDERBOOK_DEPTH], // NO HARDCODING
            ask_sizes: [0; ORDERBOOK_DEPTH],  // NO HARDCODING
            snapshot_flags: self.snapshot_flags,
            dex_type: self.dex_type,
            _padding: [0; PADDING_SIZE], // NO HARDCODING
        }
    }

    /// Build with custom depth data (for full snapshots)
    ///
    /// Automatically sets `snapshot_flags = 1` (full snapshot).
    ///
    /// # Arguments
    ///
    /// * `bid_prices` - Descending bid prices (best to worst)
    /// * `bid_sizes` - Corresponding bid sizes
    /// * `ask_prices` - Ascending ask prices (best to worst)
    /// * `ask_sizes` - Corresponding ask sizes
    ///
    /// # Panics
    ///
    /// Panics if the slice lengths don't match or exceed `ORDERBOOK_DEPTH`.
    pub fn with_depth(
        self,
        bid_prices: &[u64],
        bid_sizes: &[u64],
        ask_prices: &[u64],
        ask_sizes: &[u64],
    ) -> MarketSnapshot {
        assert_eq!(
            bid_prices.len(),
            bid_sizes.len(),
            "Bid prices and sizes must have equal length"
        );
        assert_eq!(
            ask_prices.len(),
            ask_sizes.len(),
            "Ask prices and sizes must have equal length"
        );
        assert!(
            bid_prices.len() <= ORDERBOOK_DEPTH,
            "Bid depth {} exceeds ORDERBOOK_DEPTH {}",
            bid_prices.len(),
            ORDERBOOK_DEPTH
        );
        assert!(
            ask_prices.len() <= ORDERBOOK_DEPTH,
            "Ask depth {} exceeds ORDERBOOK_DEPTH {}",
            ask_prices.len(),
            ORDERBOOK_DEPTH
        );

        let mut snapshot = self.full_snapshot().build();

        // Copy depth data up to provided length
        let bid_len = bid_prices.len();
        let ask_len = ask_prices.len();

        snapshot.bid_prices[..bid_len].copy_from_slice(&bid_prices[..bid_len]);
        snapshot.bid_sizes[..bid_len].copy_from_slice(&bid_sizes[..bid_len]);
        snapshot.ask_prices[..ask_len].copy_from_slice(&ask_prices[..ask_len]);
        snapshot.ask_sizes[..ask_len].copy_from_slice(&ask_sizes[..ask_len]);

        // Set best bid/ask from depth if not already set
        if bid_len > 0 {
            snapshot.best_bid_price = bid_prices[0];
            snapshot.best_bid_size = bid_sizes[0];
        }
        if ask_len > 0 {
            snapshot.best_ask_price = ask_prices[0];
            snapshot.best_ask_size = ask_sizes[0];
        }

        snapshot
    }
}

/// Helper: Create snapshot with realistic depth data for testing
///
/// Generates a full orderbook snapshot with `ORDERBOOK_DEPTH` levels,
/// evenly spaced prices, and uniform sizes.
///
/// # Arguments
///
/// * `mid_price` - Mid-market price in fixed-point (u64)
/// * `spread_bps` - Bid-ask spread in basis points (e.g., 10 = 0.10%)
///
/// # Example
///
/// ```rust
/// use bog_core::data::snapshot_builder::create_realistic_depth_snapshot;
///
/// // Create BTC snapshot at $100k with 10bps spread
/// let snapshot = create_realistic_depth_snapshot(
///     100_000_000_000_000, // $100k
///     10,                   // 10 bps (0.10%)
/// );
/// ```
pub fn create_realistic_depth_snapshot(mid_price: u64, spread_bps: u64) -> MarketSnapshot {
    let spread = (mid_price * spread_bps) / 10_000;
    let best_bid = mid_price - spread / 2;
    let best_ask = mid_price + spread / 2;

    // Generate ORDERBOOK_DEPTH levels with realistic prices
    let mut bid_prices = Vec::with_capacity(ORDERBOOK_DEPTH);
    let mut bid_sizes = Vec::with_capacity(ORDERBOOK_DEPTH);
    let mut ask_prices = Vec::with_capacity(ORDERBOOK_DEPTH);
    let mut ask_sizes = Vec::with_capacity(ORDERBOOK_DEPTH);

    for i in 0..ORDERBOOK_DEPTH {
        // Each level 10 bps apart
        let price_delta = (mid_price * 10) / 10_000; // 10 bps

        // Descending bid prices (each level worse)
        bid_prices.push(best_bid - (i as u64 * price_delta));
        bid_sizes.push(1_000_000_000); // 1.0 BTC per level

        // Ascending ask prices (each level worse)
        ask_prices.push(best_ask + (i as u64 * price_delta));
        ask_sizes.push(1_000_000_000);
    }

    SnapshotBuilder::new().with_depth(&bid_prices, &bid_sizes, &ask_prices, &ask_sizes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_creates_valid_snapshot() {
        let snapshot = SnapshotBuilder::new()
            .market_id(1000025)
            .sequence(12345)
            .best_bid(100_000_000_000, 1_000_000_000)
            .best_ask(100_100_000_000, 1_000_000_000)
            .incremental_snapshot()
            .build();

        assert_eq!(snapshot.market_id, 1000025);
        assert_eq!(snapshot.sequence, 12345);
        assert_eq!(snapshot.best_bid_price, 100_000_000_000);
        assert_eq!(snapshot.best_ask_price, 100_100_000_000);
        assert_eq!(snapshot.snapshot_flags, 0); // Incremental
    }

    #[test]
    fn test_depth_arrays_match_orderbook_depth() {
        let snapshot = SnapshotBuilder::new().build();

        // Verify arrays are sized by ORDERBOOK_DEPTH, not hardcoded
        assert_eq!(
            snapshot.bid_prices.len(),
            ORDERBOOK_DEPTH,
            "bid_prices must match ORDERBOOK_DEPTH"
        );
        assert_eq!(
            snapshot.bid_sizes.len(),
            ORDERBOOK_DEPTH,
            "bid_sizes must match ORDERBOOK_DEPTH"
        );
        assert_eq!(
            snapshot.ask_prices.len(),
            ORDERBOOK_DEPTH,
            "ask_prices must match ORDERBOOK_DEPTH"
        );
        assert_eq!(
            snapshot.ask_sizes.len(),
            ORDERBOOK_DEPTH,
            "ask_sizes must match ORDERBOOK_DEPTH"
        );
    }

    #[test]
    fn test_with_depth_populates_arrays() {
        let bid_prices = vec![100_000_000_000, 99_900_000_000];
        let bid_sizes = vec![1_000_000_000, 2_000_000_000];
        let ask_prices = vec![100_100_000_000, 100_200_000_000];
        let ask_sizes = vec![1_000_000_000, 2_000_000_000];

        let snapshot = SnapshotBuilder::new().market_id(1).with_depth(
            &bid_prices,
            &bid_sizes,
            &ask_prices,
            &ask_sizes,
        );

        // Verify depth data copied correctly
        assert_eq!(snapshot.bid_prices[0], 100_000_000_000);
        assert_eq!(snapshot.bid_prices[1], 99_900_000_000);
        assert_eq!(snapshot.bid_sizes[0], 1_000_000_000);
        assert_eq!(snapshot.bid_sizes[1], 2_000_000_000);

        assert_eq!(snapshot.ask_prices[0], 100_100_000_000);
        assert_eq!(snapshot.ask_prices[1], 100_200_000_000);

        // Verify it's marked as full snapshot
        assert_eq!(snapshot.snapshot_flags, 1);

        // Verify best bid/ask set from depth
        assert_eq!(snapshot.best_bid_price, 100_000_000_000);
        assert_eq!(snapshot.best_ask_price, 100_100_000_000);
    }

    #[test]
    fn test_realistic_depth_snapshot() {
        let snapshot = create_realistic_depth_snapshot(
            100_000_000_000_000, // $100k
            10,                  // 10 bps
        );

        // Verify it's a full snapshot
        assert_eq!(snapshot.snapshot_flags, 1);

        // Verify all depth levels populated
        for i in 0..ORDERBOOK_DEPTH {
            assert!(snapshot.bid_prices[i] > 0, "Bid price {} should be set", i);
            assert!(snapshot.bid_sizes[i] > 0, "Bid size {} should be set", i);
            assert!(snapshot.ask_prices[i] > 0, "Ask price {} should be set", i);
            assert!(snapshot.ask_sizes[i] > 0, "Ask size {} should be set", i);
        }

        // Verify bid prices are descending
        for i in 1..ORDERBOOK_DEPTH {
            assert!(
                snapshot.bid_prices[i] < snapshot.bid_prices[i - 1],
                "Bid prices must be descending"
            );
        }

        // Verify ask prices are ascending
        for i in 1..ORDERBOOK_DEPTH {
            assert!(
                snapshot.ask_prices[i] > snapshot.ask_prices[i - 1],
                "Ask prices must be ascending"
            );
        }
    }

    #[test]
    fn test_incremental_vs_full_snapshot() {
        let incremental = SnapshotBuilder::new().incremental_snapshot().build();
        assert_eq!(incremental.snapshot_flags, 0);

        let full = SnapshotBuilder::new().full_snapshot().build();
        assert_eq!(full.snapshot_flags, 1);
    }

    #[test]
    #[should_panic(expected = "Bid depth")]
    fn test_with_depth_panics_on_too_many_levels() {
        let too_many_prices: Vec<u64> = (0..ORDERBOOK_DEPTH + 1)
            .map(|i| 100_000_000_000 - i as u64 * 1_000_000_000)
            .collect();
        let sizes = vec![1_000_000_000; ORDERBOOK_DEPTH + 1];

        SnapshotBuilder::new().with_depth(&too_many_prices, &sizes, &[], &[]);
    }
}
