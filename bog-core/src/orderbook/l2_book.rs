//! L2 Orderbook - Full Market Depth Tracking
//!
//! This module implements a proper L2 (Level 2) orderbook that stores all 10 price levels
//! from Huginn's MarketSnapshot, enabling:
//! - Real VWAP calculations (not estimates)
//! - Real orderbook imbalance (buy vs sell pressure)
//! - Liquidity analysis at different depths
//! - Queue position estimation
//! - Price impact analysis
//!
//! ## Data Source
//!
//! Huginn provides 10 levels via MarketSnapshot (512 bytes):
//! - bid_prices[10], bid_sizes[10]
//! - ask_prices[10], ask_sizes[10]
//! - All in u64 fixed-point (9 decimals)
//!
//! ## Design
//!
//! - Zero-copy from MarketSnapshot (direct memcpy)
//! - Cache-aligned for performance
//! - Fast VWAP/imbalance calculations (<10ns)
//! - No heap allocations in hot path
//!
//! ## Performance
//!
//! - Sync from snapshot: ~20ns (cache-aligned copy)
//! - VWAP calculation: <10ns (inline loops)
//! - Imbalance calculation: <5ns
//! - Total overhead: ~35ns vs ~5ns for stub (acceptable for correctness)

use crate::data::MarketSnapshot;
use crate::orderbook::depth;
use rust_decimal::Decimal;

/// Number of price levels stored (from Huginn's configurable depth)
pub const DEPTH_LEVELS: usize = huginn::shm::ORDERBOOK_DEPTH;

/// L2 Orderbook - stores full market depth from Huginn
///
/// This structure mirrors the depth data from MarketSnapshot for fast synchronization.
/// All prices and sizes are stored in u64 fixed-point (9 decimals) to match Huginn's format.
#[derive(Debug, Clone)]
pub struct L2OrderBook {
    /// Market identifier
    pub market_id: u64,

    /// Bid side (sorted descending: [best, ..., worst])
    pub bid_prices: [u64; DEPTH_LEVELS],
    pub bid_sizes: [u64; DEPTH_LEVELS],

    /// Ask side (sorted ascending: [best, ..., worst])
    pub ask_prices: [u64; DEPTH_LEVELS],
    pub ask_sizes: [u64; DEPTH_LEVELS],

    /// Last sequence number processed
    pub last_sequence: u64,

    /// Timestamp of last update (for staleness detection)
    pub last_update_ns: u64,

    /// Flag indicating if depth levels (1-9) might be stale
    /// True if last update was incremental (only level 0 updated)
    pub depth_stale: bool,
}

impl L2OrderBook {
    /// Create a new empty L2 orderbook
    pub fn new(market_id: u64) -> Self {
        Self {
            market_id,
            bid_prices: [0; DEPTH_LEVELS],
            bid_sizes: [0; DEPTH_LEVELS],
            ask_prices: [0; DEPTH_LEVELS],
            ask_sizes: [0; DEPTH_LEVELS],
            last_sequence: 0,
            last_update_ns: 0,
            depth_stale: true, // Initially stale until full snapshot
        }
    }

    /// Synchronize from Huginn MarketSnapshot
    ///
    /// Detects snapshot type via IS_FULL_SNAPSHOT flag and chooses optimal path:
    /// - Full snapshot (flag=1): Rebuild all 10 levels (~50ns)
    /// - Incremental (flag=0): Update only top-of-book (~20ns)
    ///
    /// No validation is performed here (validation happens in circuit breaker).
    #[inline]
    pub fn sync_from_snapshot(&mut self, snapshot: &MarketSnapshot) {
        if snapshot.is_full_snapshot() {
            self.full_rebuild(snapshot);
        } else {
            self.incremental_update(snapshot);
        }
    }

    /// Full rebuild of all orderbook levels
    ///
    /// Called when IS_FULL_SNAPSHOT flag is set.
    /// Replaces ALL state with fresh data from Huginn.
    ///
    /// Performance: ~50ns (memcpy of 40 u64 values)
    #[inline]
    pub fn full_rebuild(&mut self, snapshot: &MarketSnapshot) {
        // Reconstruct L2 book from snapshot
        // Level 0 (Best) comes from dedicated fields (redundant if arrays are full, but safe)
        self.bid_prices[0] = snapshot.best_bid_price;
        self.bid_sizes[0] = snapshot.best_bid_size;
        self.ask_prices[0] = snapshot.best_ask_price;
        self.ask_sizes[0] = snapshot.best_ask_size;

        // Copy all depth levels (0-9) directly from snapshot arrays
        // Huginn ensures bid_prices[0] == best_bid_price for consistency
        self.bid_prices.copy_from_slice(&snapshot.bid_prices);
        self.bid_sizes.copy_from_slice(&snapshot.bid_sizes);
        self.ask_prices.copy_from_slice(&snapshot.ask_prices);
        self.ask_sizes.copy_from_slice(&snapshot.ask_sizes);

        self.last_sequence = snapshot.sequence;
        self.last_update_ns = snapshot.exchange_timestamp_ns;
        self.depth_stale = false; // Full rebuild guarantees fresh depth
    }

    /// Incremental update of top-of-book only
    ///
    /// Called when IS_FULL_SNAPSHOT flag is NOT set.
    /// Updates only the best bid/ask prices and sizes (level 0).
    /// Deeper levels (1-9) are preserved from previous state.
    ///
    /// This is used during continuous trading when only top-of-book changes.
    /// Deeper levels only update when a full snapshot arrives.
    ///
    /// Performance: ~20ns (copy 4 u64 values)
    #[inline]
    pub fn incremental_update(&mut self, snapshot: &MarketSnapshot) {
        // Check for sequence gaps
        if let Some(gap) = self.check_sequence_gap(snapshot.sequence) {
            if gap >= 1 {
                // Gap >= 1 means we missed at least one update.
                // Since this is incremental, we might have missed updates to levels 1-9.
                // We log this critical warning. The Engine or CircuitBreaker should decide whether to halt.
                // For now, we assume levels 1-9 are potentially stale.
                tracing::warn!(
                    "SEQUENCE GAP detected in incremental update: {} -> {} (gap: {}). Depth levels 1-9 may be stale.",
                    self.last_sequence,
                    snapshot.sequence,
                    gap
                );
                self.depth_stale = true;

                // Clear potentially stale depth levels (1-9) to prevent strategies using invalid data
                self.bid_prices[1..].fill(0);
                self.bid_sizes[1..].fill(0);
                self.ask_prices[1..].fill(0);
                self.ask_sizes[1..].fill(0);
            }
        }

        // Update only level 0 (best bid/ask)
        self.bid_prices[0] = snapshot.best_bid_price;
        self.bid_sizes[0] = snapshot.best_bid_size;
        self.ask_prices[0] = snapshot.best_ask_price;
        self.ask_sizes[0] = snapshot.best_ask_size;

        // Preserve levels 1-9 from previous state
        // (deeper levels don't change in incremental updates)

        self.last_sequence = snapshot.sequence;
        self.last_update_ns = snapshot.exchange_timestamp_ns;
        // self.depth_stale = true; // REMOVED: Incremental updates preserve depth if no gap
    }

    /// Get best bid price (level 0)
    ///
    /// Note: This returns the corrected best bid (max of all bid levels) to handle
    /// cases where Lighter DEX sends orderbook levels in unexpected order.
    #[inline(always)]
    pub fn best_bid_price(&self) -> u64 {
        // Find max bid price across all levels (handles incorrectly ordered data)
        let mut max_bid = self.bid_prices[0];
        for &price in &self.bid_prices[1..] {
            if price > max_bid {
                max_bid = price;
            }
        }
        max_bid
    }

    /// Get best ask price (level 0)
    ///
    /// Note: This returns the corrected best ask (min of all non-zero ask levels) to handle
    /// cases where Lighter DEX sends orderbook levels in unexpected order.
    #[inline(always)]
    pub fn best_ask_price(&self) -> u64 {
        // Find min ask price across all levels, ignoring zeros (handles incorrectly ordered data)
        let mut min_ask = self.ask_prices[0];
        for &price in &self.ask_prices[1..] {
            if price > 0 && (min_ask == 0 || price < min_ask) {
                min_ask = price;
            }
        }
        min_ask
    }

    /// Get raw bid price at level 0 (as received from Huginn, without correction)
    #[inline(always)]
    pub fn raw_bid_price(&self) -> u64 {
        self.bid_prices[0]
    }

    /// Get raw ask price at level 0 (as received from Huginn, without correction)
    #[inline(always)]
    pub fn raw_ask_price(&self) -> u64 {
        self.ask_prices[0]
    }

    /// Get best bid size (level 0)
    #[inline(always)]
    pub fn best_bid_size(&self) -> u64 {
        self.bid_sizes[0]
    }

    /// Get best ask size (level 0)
    #[inline(always)]
    pub fn best_ask_size(&self) -> u64 {
        self.ask_sizes[0]
    }

    /// Calculate mid price (average of best bid and ask)
    ///
    /// Uses overflow-safe calculation: bid/2 + ask/2 + (bid%2 + ask%2)/2
    #[inline]
    pub fn mid_price(&self) -> u64 {
        let bid = self.best_bid_price();
        let ask = self.best_ask_price();

        if bid == 0 || ask == 0 {
            return 0;
        }

        // Overflow-safe mid price calculation
        bid / 2 + ask / 2 + (bid % 2 + ask % 2) / 2
    }

    /// Calculate spread in basis points
    ///
    /// Returns 0 if orderbook is invalid.
    #[inline]
    pub fn spread_bps(&self) -> u32 {
        depth::spread_bps_from_prices(self.best_bid_price(), self.best_ask_price())
    }

    /// Calculate orderbook imbalance using all depth levels
    ///
    /// Returns value from -100 to +100:
    /// - Negative: More sell pressure (ask side heavier)
    /// - Positive: More buy pressure (bid side heavier)
    /// - 0: Balanced
    ///
    /// Uses all available levels for accurate reading.
    #[inline]
    pub fn imbalance(&self) -> i64 {
        depth::calculate_imbalance_i64(
            &self.bid_prices,
            &self.bid_sizes,
            &self.ask_prices,
            &self.ask_sizes,
            DEPTH_LEVELS,
        )
    }

    /// Calculate VWAP (Volume-Weighted Average Price) for a given side and depth
    ///
    /// - `is_bid`: true for bid side VWAP, false for ask side
    /// - `max_levels`: number of levels to include (1-10)
    ///
    /// Returns None if orderbook is invalid.
    #[inline]
    pub fn vwap(&self, is_bid: bool, max_levels: usize) -> Option<u64> {
        let max_levels = max_levels.min(DEPTH_LEVELS);

        if is_bid {
            depth::calculate_vwap_u64(&self.bid_prices, &self.bid_sizes, max_levels)
        } else {
            depth::calculate_vwap_u64(&self.ask_prices, &self.ask_sizes, max_levels)
        }
    }

    /// Calculate total liquidity on a given side up to max_levels
    ///
    /// Returns total size available across all levels.
    #[inline]
    pub fn total_liquidity(&self, is_bid: bool, max_levels: usize) -> u64 {
        let max_levels = max_levels.min(DEPTH_LEVELS);
        let sizes = if is_bid {
            &self.bid_sizes
        } else {
            &self.ask_sizes
        };

        sizes.iter().take(max_levels).sum()
    }

    /// Get liquidity available within N basis points of mid price
    ///
    /// Returns (bid_liquidity, ask_liquidity) in u64 fixed-point.
    pub fn liquidity_within_bps(&self, bps: u32) -> (u64, u64) {
        let mid = self.mid_price();
        if mid == 0 {
            return (0, 0);
        }

        // Calculate price bounds
        let distance = (mid * bps as u64) / 10_000;
        let bid_threshold = mid.saturating_sub(distance);
        let ask_threshold = mid.saturating_add(distance);

        // Sum bid liquidity above threshold
        let mut bid_liq = 0u64;
        for i in 0..DEPTH_LEVELS {
            if self.bid_prices[i] >= bid_threshold {
                bid_liq = bid_liq.saturating_add(self.bid_sizes[i]);
            } else {
                break; // Prices are sorted, can stop early
            }
        }

        // Sum ask liquidity below threshold
        let mut ask_liq = 0u64;
        for i in 0..DEPTH_LEVELS {
            if self.ask_prices[i] <= ask_threshold && self.ask_prices[i] > 0 {
                ask_liq = ask_liq.saturating_add(self.ask_sizes[i]);
            } else {
                break; // Prices are sorted, can stop early
            }
        }

        (bid_liq, ask_liq)
    }

    /// Estimate queue position for our order at a given price
    ///
    /// Assumes FIFO queue and that we're joining at the back.
    /// Returns None if price level doesn't exist in visible depth.
    pub fn estimate_queue_position(&self, is_bid: bool, our_price: u64) -> Option<QueuePosition> {
        let (prices, sizes) = if is_bid {
            (&self.bid_prices, &self.bid_sizes)
        } else {
            (&self.ask_prices, &self.ask_sizes)
        };

        // Find our price level
        for i in 0..DEPTH_LEVELS {
            if prices[i] == our_price {
                // Found our level
                // Assume we're at the back of the queue (worst case)
                let size_ahead = sizes[i];
                let total_size = sizes[i];

                return Some(QueuePosition {
                    level: i,
                    size_ahead,
                    total_size,
                    position_ratio: 1.0, // Back of queue
                });
            }
        }

        // Price not in visible depth
        None
    }

    /// Check if orderbook is crossed (bid >= ask) - invalid state
    #[inline]
    pub fn is_crossed(&self) -> bool {
        let bid = self.best_bid_price();
        let ask = self.best_ask_price();
        bid > 0 && ask > 0 && bid >= ask
    }

    /// Check if orderbook is locked (bid == ask) - rare but valid
    #[inline]
    pub fn is_locked(&self) -> bool {
        let bid = self.best_bid_price();
        let ask = self.best_ask_price();
        bid > 0 && bid == ask
    }

    /// Check if orderbook is valid (has both sides, not crossed)
    #[inline]
    pub fn is_valid(&self) -> bool {
        let bid = self.best_bid_price();
        let ask = self.best_ask_price();

        bid > 0
            && ask > 0
            && self.best_bid_size() > 0
            && self.best_ask_size() > 0
            && !self.is_crossed()
    }

    /// Get all bid levels as (price, size) pairs
    ///
    /// Returns only levels with non-zero prices.
    pub fn bid_levels(&self) -> Vec<(u64, u64)> {
        self.bid_prices
            .iter()
            .zip(self.bid_sizes.iter())
            .take_while(|(price, _)| **price > 0)
            .map(|(p, s)| (*p, *s))
            .collect()
    }

    /// Get all ask levels as (price, size) pairs
    ///
    /// Returns only levels with non-zero prices.
    pub fn ask_levels(&self) -> Vec<(u64, u64)> {
        self.ask_prices
            .iter()
            .zip(self.ask_sizes.iter())
            .take_while(|(price, _)| **price > 0)
            .map(|(p, s)| (*p, *s))
            .collect()
    }

    /// Get number of valid bid levels
    pub fn bid_depth(&self) -> usize {
        self.bid_prices.iter().take_while(|&&p| p > 0).count()
    }

    /// Get number of valid ask levels
    pub fn ask_depth(&self) -> usize {
        self.ask_prices.iter().take_while(|&&p| p > 0).count()
    }

    /// Check for sequence gaps (potential data loss)
    ///
    /// Returns the gap size if detected, None otherwise.
    pub fn check_sequence_gap(&self, new_sequence: u64) -> Option<u64> {
        if self.last_sequence == 0 {
            return None; // First update
        }

        let expected = self.last_sequence + 1;
        if new_sequence > expected {
            Some(new_sequence - expected)
        } else {
            None
        }
    }

    /// Get orderbook age in nanoseconds
    pub fn age_ns(&self, current_time_ns: u64) -> u64 {
        if current_time_ns >= self.last_update_ns {
            current_time_ns - self.last_update_ns
        } else {
            0 // Clock skew
        }
    }
}

/// Queue position information for an order at a specific price level
#[derive(Debug, Clone, Copy)]
pub struct QueuePosition {
    /// Which level (0 = best, 9 = worst visible)
    pub level: usize,
    /// Estimated size ahead of us in queue
    pub size_ahead: u64,
    /// Total size at this level
    pub total_size: u64,
    /// Position in queue (0.0 = front, 1.0 = back)
    pub position_ratio: f64,
}

impl QueuePosition {
    /// Estimate fill probability based on queue position
    ///
    /// Front of queue: ~80% fill rate
    /// Back of queue: ~40% fill rate
    pub fn fill_probability(&self) -> f64 {
        let front_rate = 0.8;
        let back_rate = 0.4;

        // Linear interpolation based on position
        front_rate + (back_rate - front_rate) * self.position_ratio
    }
}

// ============================================================================
// Decimal Conversion Utilities (for backwards compatibility)
// ============================================================================

/// Convert u64 fixed-point to Decimal (9 decimals)
#[inline]
fn u64_to_decimal(value: u64) -> Decimal {
    Decimal::from(value) / Decimal::from(1_000_000_000)
}

impl L2OrderBook {
    /// Get best bid as Decimal (for backwards compatibility)
    pub fn best_bid(&self) -> Decimal {
        u64_to_decimal(self.best_bid_price())
    }

    /// Get best ask as Decimal (for backwards compatibility)
    pub fn best_ask(&self) -> Decimal {
        u64_to_decimal(self.best_ask_price())
    }

    /// Get mid price as Decimal (for backwards compatibility)
    pub fn mid_price_decimal(&self) -> Decimal {
        u64_to_decimal(self.mid_price())
    }

    /// Calculate VWAP as Decimal (for backwards compatibility)
    pub fn vwap_decimal(&self, is_bid: bool, max_levels: usize) -> Option<Decimal> {
        self.vwap(is_bid, max_levels).map(u64_to_decimal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::MarketSnapshot;

    fn create_test_snapshot() -> MarketSnapshot {
        let mut snapshot = unsafe { std::mem::zeroed::<MarketSnapshot>() };
        snapshot.market_id = 1;
        snapshot.sequence = 100;
        snapshot.exchange_timestamp_ns = 1000000000;
        snapshot.snapshot_flags = 0; // Default: incremental (no flag)

        // Best bid/ask (level 0)
        snapshot.best_bid_price = 50_000_000_000_000; // $50,000
        snapshot.best_bid_size = 1_000_000_000; // 1.0 BTC
        snapshot.best_ask_price = 50_010_000_000_000; // $50,010
        snapshot.best_ask_size = 1_500_000_000; // 1.5 BTC

        // Fill in depth levels (bids descending, asks ascending)
        for i in 0..DEPTH_LEVELS {
            snapshot.bid_prices[i] = 50_000_000_000_000 - (i as u64 * 10_000_000_000); // -$10 per level
            snapshot.bid_sizes[i] = 1_000_000_000 + (i as u64 * 100_000_000); // +0.1 BTC per level

            snapshot.ask_prices[i] = 50_010_000_000_000 + (i as u64 * 10_000_000_000); // +$10 per level
            snapshot.ask_sizes[i] = 1_500_000_000 + (i as u64 * 100_000_000); // +0.1 BTC per level
        }

        snapshot
    }

    fn set_full_snapshot(snapshot: &mut MarketSnapshot, is_full: bool) {
        if is_full {
            snapshot.snapshot_flags = 0x01; // IS_FULL_SNAPSHOT flag
        } else {
            snapshot.snapshot_flags = 0x00;
        }
    }

    fn is_full_snapshot(snapshot: &MarketSnapshot) -> bool {
        (snapshot.snapshot_flags & 0x01) != 0
    }

    #[test]
    fn test_l2_orderbook_creation() {
        let book = L2OrderBook::new(1);

        assert_eq!(book.market_id, 1);
        assert_eq!(book.best_bid_price(), 0);
        assert_eq!(book.best_ask_price(), 0);
        assert_eq!(book.last_sequence, 0);
    }

    #[test]
    fn test_sync_from_snapshot() {
        let mut book = L2OrderBook::new(1);
        let snapshot = create_test_snapshot();

        book.sync_from_snapshot(&snapshot);

        assert_eq!(book.market_id, 1);
        assert_eq!(book.last_sequence, 100);
        assert_eq!(book.best_bid_price(), 50_000_000_000_000);
        assert_eq!(book.best_ask_price(), 50_010_000_000_000);
    }

    #[test]
    fn test_all_levels_synced() {
        let mut book = L2OrderBook::new(1);
        let mut snapshot = create_test_snapshot();

        // Must use full snapshot for all levels to be synced
        // (incremental only syncs level 0)
        set_full_snapshot(&mut snapshot, true);

        book.sync_from_snapshot(&snapshot);

        // Verify all 10 levels were copied
        for i in 0..DEPTH_LEVELS {
            assert_eq!(book.bid_prices[i], snapshot.bid_prices[i]);
            assert_eq!(book.bid_sizes[i], snapshot.bid_sizes[i]);
            assert_eq!(book.ask_prices[i], snapshot.ask_prices[i]);
            assert_eq!(book.ask_sizes[i], snapshot.ask_sizes[i]);
        }
    }

    #[test]
    fn test_mid_price_calculation() {
        let mut book = L2OrderBook::new(1);
        let snapshot = create_test_snapshot();
        book.sync_from_snapshot(&snapshot);

        let mid = book.mid_price();

        // Mid should be average of 50000 and 50010 = 50005
        assert_eq!(mid, 50_005_000_000_000);
    }

    #[test]
    fn test_spread_bps_calculation() {
        let mut book = L2OrderBook::new(1);
        let snapshot = create_test_snapshot();
        book.sync_from_snapshot(&snapshot);

        let spread = book.spread_bps();

        // Spread: (50010 - 50000) / 50000 * 10000 = 2 bps
        assert_eq!(spread, 2);
    }

    #[test]
    fn test_imbalance_calculation() {
        let mut book = L2OrderBook::new(1);
        let snapshot = create_test_snapshot();
        book.sync_from_snapshot(&snapshot);

        // Ask side has more size (1.5 BTC vs 1.0 BTC at best level)
        // Plus each level adds 0.1 BTC
        let imbalance = book.imbalance();

        // Should be negative (more sell pressure)
        assert!(imbalance < 0, "Imbalance should be negative (ask-heavy)");
    }

    #[test]
    fn test_vwap_calculation() {
        let mut book = L2OrderBook::new(1);
        let mut snapshot = create_test_snapshot();
        set_full_snapshot(&mut snapshot, true); // Need full snapshot for depth levels
        book.sync_from_snapshot(&snapshot);

        // Calculate bid VWAP for 3 levels
        let vwap_bid = book.vwap(true, 3);
        assert!(vwap_bid.is_some());

        // VWAP should be slightly below best bid due to worse prices at deeper levels
        let vwap = vwap_bid.unwrap();
        assert!(vwap < book.best_bid_price());
        assert!(vwap > 0);

        // Calculate ask VWAP for 3 levels
        let vwap_ask = book.vwap(false, 3);
        assert!(vwap_ask.is_some());

        // VWAP should be slightly above best ask
        let vwap = vwap_ask.unwrap();
        assert!(vwap > book.best_ask_price());
    }

    #[test]
    fn test_total_liquidity() {
        let mut book = L2OrderBook::new(1);
        let mut snapshot = create_test_snapshot();
        set_full_snapshot(&mut snapshot, true); // Need full snapshot for depth levels
        book.sync_from_snapshot(&snapshot);

        let bid_liq = book.total_liquidity(true, 10);
        let ask_liq = book.total_liquidity(false, 10);

        assert!(bid_liq > 0);
        assert!(ask_liq > 0);
        assert!(bid_liq > book.best_bid_size()); // Total > best level
    }

    #[test]
    fn test_liquidity_within_bps() {
        let mut book = L2OrderBook::new(1);
        let snapshot = create_test_snapshot();
        book.sync_from_snapshot(&snapshot);

        // Get liquidity within 5 bps of mid
        let (bid_liq, ask_liq) = book.liquidity_within_bps(5);

        assert!(bid_liq >= book.best_bid_size());
        assert!(ask_liq >= book.best_ask_size());
    }

    #[test]
    fn test_queue_position_estimation() {
        let mut book = L2OrderBook::new(1);
        let snapshot = create_test_snapshot();
        book.sync_from_snapshot(&snapshot);

        // Estimate position at best bid
        let pos = book.estimate_queue_position(true, book.best_bid_price());

        assert!(pos.is_some());
        let pos = pos.unwrap();
        assert_eq!(pos.level, 0);
        assert_eq!(pos.total_size, book.best_bid_size());

        // Fill probability should be reasonable
        let prob = pos.fill_probability();
        assert!(prob >= 0.4 && prob <= 0.8);
    }

    #[test]
    fn test_is_crossed_detection() {
        let mut book = L2OrderBook::new(1);

        // Set crossed book (bid > ask)
        book.bid_prices[0] = 50_000_000_000_000;
        book.ask_prices[0] = 49_990_000_000_000;

        assert!(book.is_crossed());
        assert!(!book.is_valid());
    }

    #[test]
    fn test_sequence_gap_detection() {
        let mut book = L2OrderBook::new(1);
        book.last_sequence = 100;

        // Normal increment
        assert!(book.check_sequence_gap(101).is_none());

        // Gap detected
        let gap = book.check_sequence_gap(105);
        assert_eq!(gap, Some(4));
    }

    #[test]
    fn test_bid_and_ask_levels() {
        let mut book = L2OrderBook::new(1);
        let mut snapshot = create_test_snapshot();
        set_full_snapshot(&mut snapshot, true); // Need full snapshot for depth levels
        book.sync_from_snapshot(&snapshot);

        let bid_levels = book.bid_levels();
        let ask_levels = book.ask_levels();

        assert_eq!(bid_levels.len(), 10);
        assert_eq!(ask_levels.len(), 10);

        // Verify sorted order
        for i in 1..bid_levels.len() {
            assert!(
                bid_levels[i].0 < bid_levels[i - 1].0,
                "Bids should be descending"
            );
        }

        for i in 1..ask_levels.len() {
            assert!(
                ask_levels[i].0 > ask_levels[i - 1].0,
                "Asks should be ascending"
            );
        }
    }

    #[test]
    fn test_depth_counting() {
        let mut book = L2OrderBook::new(1);
        let mut snapshot = create_test_snapshot();
        set_full_snapshot(&mut snapshot, true); // Need full snapshot for depth levels
        book.sync_from_snapshot(&snapshot);

        assert_eq!(book.bid_depth(), 10);
        assert_eq!(book.ask_depth(), 10);

        // Partially filled book
        book.bid_prices[5..].fill(0);
        assert_eq!(book.bid_depth(), 5);
    }

    #[test]
    fn test_decimal_conversion_compatibility() {
        let mut book = L2OrderBook::new(1);
        let snapshot = create_test_snapshot();
        book.sync_from_snapshot(&snapshot);

        let bid_decimal = book.best_bid();
        let ask_decimal = book.best_ask();
        let mid_decimal = book.mid_price_decimal();

        assert_eq!(bid_decimal.to_string(), "50000");
        assert_eq!(ask_decimal.to_string(), "50010");
        assert_eq!(mid_decimal.to_string(), "50005");
    }

    // ========================================================================
    // SNAPSHOT FLAG HANDLING TESTS
    // ========================================================================

    #[test]
    fn test_full_rebuild_replaces_all_state() {
        let mut book = L2OrderBook::new(1);

        // First snapshot A
        let mut snapshot_a = create_test_snapshot();
        set_full_snapshot(&mut snapshot_a, true);
        snapshot_a.sequence = 100;
        book.full_rebuild(&snapshot_a);

        assert_eq!(book.best_bid_price(), 50_000_000_000_000);
        assert_eq!(book.best_ask_price(), 50_010_000_000_000);
        assert_eq!(book.last_sequence, 100);

        // Second snapshot B with different prices
        let mut snapshot_b = create_test_snapshot();
        set_full_snapshot(&mut snapshot_b, true);
        snapshot_b.sequence = 101;
        // Change prices dramatically
        snapshot_b.best_bid_price = 51_000_000_000_000; // $51,000
        snapshot_b.best_ask_price = 51_010_000_000_000; // $51,010
        for i in 0..DEPTH_LEVELS {
            snapshot_b.bid_prices[i] = 51_000_000_000_000 - (i as u64 * 10_000_000_000);
            snapshot_b.ask_prices[i] = 51_010_000_000_000 + (i as u64 * 10_000_000_000);
        }
        book.full_rebuild(&snapshot_b);

        // Verify B completely replaced A
        assert_eq!(book.best_bid_price(), 51_000_000_000_000);
        assert_eq!(book.best_ask_price(), 51_010_000_000_000);
        assert_eq!(book.last_sequence, 101);
        // Verify all 10 levels match B (not A)
        for i in 0..DEPTH_LEVELS {
            assert_eq!(book.bid_prices[i], snapshot_b.bid_prices[i]);
            assert_eq!(book.ask_prices[i], snapshot_b.ask_prices[i]);
        }
    }

    #[test]
    fn test_incremental_update_only_changes_level_0() {
        let mut book = L2OrderBook::new(1);

        // Initialize with full snapshot
        let mut snapshot_full = create_test_snapshot();
        set_full_snapshot(&mut snapshot_full, true);
        book.full_rebuild(&snapshot_full);

        // Store level 2-9 prices from initial state
        let saved_bid_level_2 = book.bid_prices[2];
        let saved_bid_level_5 = book.bid_prices[5];
        let saved_ask_level_2 = book.ask_prices[2];
        let saved_ask_level_5 = book.ask_prices[5];

        // Now send incremental update with different top-of-book
        let mut snapshot_incr = create_test_snapshot();
        set_full_snapshot(&mut snapshot_incr, false);
        snapshot_incr.sequence = 101;
        snapshot_incr.best_bid_price = 49_500_000_000_000; // Different
        snapshot_incr.best_ask_price = 49_510_000_000_000; // Different
        book.incremental_update(&snapshot_incr);

        // Verify level 0 changed (use raw array access, not best_bid_price() which finds max)
        // best_bid_price() returns max across all levels to handle misordered Lighter data
        assert_eq!(book.bid_prices[0], 49_500_000_000_000);
        assert_eq!(book.ask_prices[0], 49_510_000_000_000);
        assert_eq!(book.last_sequence, 101);

        // Verify levels 1-9 preserved (NOT updated from incremental)
        assert_eq!(book.bid_prices[2], saved_bid_level_2);
        assert_eq!(book.bid_prices[5], saved_bid_level_5);
        assert_eq!(book.ask_prices[2], saved_ask_level_2);
        assert_eq!(book.ask_prices[5], saved_ask_level_5);
    }

    #[test]
    fn test_sync_dispatches_to_full_rebuild_when_full_flag_set() {
        let mut book = L2OrderBook::new(1);

        let mut snapshot = create_test_snapshot();
        set_full_snapshot(&mut snapshot, true);
        snapshot.sequence = 50;

        // Call generic sync_from_snapshot which should detect flag
        book.sync_from_snapshot(&snapshot);

        // Verify full rebuild was performed (all levels updated)
        assert_eq!(book.last_sequence, 50);
        for i in 0..DEPTH_LEVELS {
            assert_eq!(book.bid_prices[i], snapshot.bid_prices[i]);
            assert_eq!(book.ask_prices[i], snapshot.ask_prices[i]);
        }
    }

    #[test]
    fn test_sync_dispatches_to_incremental_when_full_flag_clear() {
        let mut book = L2OrderBook::new(1);

        // Initialize with full snapshot first
        let mut snapshot_full = create_test_snapshot();
        set_full_snapshot(&mut snapshot_full, true);
        book.sync_from_snapshot(&snapshot_full);

        let saved_level_5 = book.bid_prices[5];

        // Now send incremental update
        let mut snapshot_incr = create_test_snapshot();
        set_full_snapshot(&mut snapshot_incr, false);
        snapshot_incr.best_bid_price = 45_000_000_000_000;

        book.sync_from_snapshot(&snapshot_incr);

        // Verify only level 0 updated (use raw array, best_bid_price() returns max across all levels)
        assert_eq!(book.bid_prices[0], 45_000_000_000_000);
        // Level 5 should still be the saved value
        assert_eq!(book.bid_prices[5], saved_level_5);
    }

    #[test]
    fn test_full_rebuild_maintains_orderbook_invariants() {
        let mut book = L2OrderBook::new(1);
        let mut snapshot = create_test_snapshot();
        set_full_snapshot(&mut snapshot, true);

        book.full_rebuild(&snapshot);

        // Verify invariants are maintained
        assert!(
            !book.is_crossed(),
            "Orderbook should not be crossed after rebuild"
        );

        // Bids should be descending
        for i in 1..10 {
            assert!(
                book.bid_prices[i] <= book.bid_prices[i - 1],
                "Bids should be descending"
            );
        }

        // Asks should be ascending
        for i in 1..10 {
            assert!(
                book.ask_prices[i] >= book.ask_prices[i - 1],
                "Asks should be ascending"
            );
        }
    }

    #[test]
    fn test_incremental_preserves_deeper_level_sizes() {
        let mut book = L2OrderBook::new(1);

        // Initialize
        let mut snapshot_full = create_test_snapshot();
        set_full_snapshot(&mut snapshot_full, true);
        book.full_rebuild(&snapshot_full);

        // Save all sizes from levels 1-9
        let saved_bid_sizes: Vec<u64> = book.bid_sizes[1..10].to_vec();
        let saved_ask_sizes: Vec<u64> = book.ask_sizes[1..10].to_vec();

        // Apply incremental with different sizes at level 0
        let mut snapshot_incr = create_test_snapshot();
        set_full_snapshot(&mut snapshot_incr, false);
        snapshot_incr.best_bid_size = 999_999_999; // Very different
        snapshot_incr.best_ask_size = 888_888_888; // Very different
        book.incremental_update(&snapshot_incr);

        // Verify level 0 changed
        assert_eq!(book.best_bid_size(), 999_999_999);
        assert_eq!(book.best_ask_size(), 888_888_888);

        // Verify levels 1-9 preserved
        for i in 1..10 {
            assert_eq!(book.bid_sizes[i], saved_bid_sizes[i - 1]);
            assert_eq!(book.ask_sizes[i], saved_ask_sizes[i - 1]);
        }
    }

    #[test]
    fn test_empty_orderbook_incremental_update() {
        let mut book = L2OrderBook::new(1);
        // Don't initialize - book is empty

        let mut snapshot = create_test_snapshot();
        set_full_snapshot(&mut snapshot, false);
        book.incremental_update(&snapshot);

        // Should update level 0 even if book was empty
        assert_eq!(book.best_bid_price(), 50_000_000_000_000);
        assert_eq!(book.best_ask_price(), 50_010_000_000_000);
        // But levels 1-9 should remain zero (no previous state)
        assert_eq!(book.bid_prices[1], 0);
        assert_eq!(book.ask_prices[1], 0);
    }

    #[test]
    fn test_state_machine_full_to_incremental_to_full() {
        let mut book = L2OrderBook::new(1);

        // 1. Full snapshot
        let mut snap1 = create_test_snapshot();
        set_full_snapshot(&mut snap1, true);
        snap1.sequence = 1;
        book.sync_from_snapshot(&snap1);
        let level_5_after_full = book.bid_prices[5];

        // 2. Incremental update (changes level 0 only)
        let mut snap2 = create_test_snapshot();
        set_full_snapshot(&mut snap2, false);
        snap2.sequence = 2;
        snap2.best_bid_price = 40_000_000_000_000;
        book.sync_from_snapshot(&snap2);

        // Use raw array access for level 0 since best_bid_price() returns max across all levels
        assert_eq!(book.bid_prices[0], 40_000_000_000_000);
        assert_eq!(book.bid_prices[5], level_5_after_full); // Preserved

        // 3. Another full snapshot (resets everything)
        let mut snap3 = create_test_snapshot();
        set_full_snapshot(&mut snap3, true);
        snap3.sequence = 3;
        snap3.bid_prices[5] = 48_000_000_000_000; // Different from snap1
        book.sync_from_snapshot(&snap3);

        assert_eq!(book.bid_prices[5], 48_000_000_000_000); // Updated
        assert_eq!(book.last_sequence, 3);
    }

    #[test]
    fn test_snapshot_flag_type_checking() {
        let mut snapshot = create_test_snapshot();

        // Test full snapshot flag
        set_full_snapshot(&mut snapshot, true);
        assert!(is_full_snapshot(&snapshot), "Flag should be set");

        // Test incremental (flag clear)
        set_full_snapshot(&mut snapshot, false);
        assert!(!is_full_snapshot(&snapshot), "Flag should be clear");
    }

    #[test]
    fn test_sequence_gap_clears_depth() {
        let mut book = L2OrderBook::new(1);

        // Initialize with full snapshot
        let mut snapshot_full = create_test_snapshot();
        set_full_snapshot(&mut snapshot_full, true);
        snapshot_full.sequence = 100;
        book.full_rebuild(&snapshot_full);

        // Verify depth is populated
        assert_ne!(book.bid_prices[5], 0);
        assert_ne!(book.ask_prices[5], 0);
        assert!(!book.depth_stale);

        // Send incremental update with GAP (100 -> 105, gap of 4)
        let mut snapshot_gap = create_test_snapshot();
        set_full_snapshot(&mut snapshot_gap, false);
        snapshot_gap.sequence = 105;

        book.incremental_update(&snapshot_gap);

        // Assert depth is cleared
        for i in 1..DEPTH_LEVELS {
            assert_eq!(book.bid_prices[i], 0, "Bid level {} should be 0", i);
            assert_eq!(book.ask_prices[i], 0, "Ask level {} should be 0", i);
            assert_eq!(book.bid_sizes[i], 0, "Bid size {} should be 0", i);
            assert_eq!(book.ask_sizes[i], 0, "Ask size {} should be 0", i);
        }

        // Level 0 should be updated
        assert_eq!(book.bid_prices[0], snapshot_gap.best_bid_price);
        assert_eq!(book.ask_prices[0], snapshot_gap.best_ask_price);

        // Depth stale flag should be set
        assert!(book.depth_stale);
    }
}
