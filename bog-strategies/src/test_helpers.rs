//! Test helpers for creating market snapshots with depth
//!
//! These utilities make it easy to test depth-aware strategy logic
//! by providing configurable orderbook snapshots.

#[cfg(test)]
use bog_core::data::MarketSnapshot;

/// Create a basic test snapshot with top-of-book only
#[cfg(test)]
pub fn create_basic_snapshot(
    bid_price: u64,
    ask_price: u64,
    bid_size: u64,
    ask_size: u64,
) -> MarketSnapshot {
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
        best_bid_price: bid_price,
        best_bid_size: bid_size,
        best_ask_price: ask_price,
        best_ask_size: ask_size,
        bid_prices: [0; 10],
        bid_sizes: [0; 10],
        ask_prices: [0; 10],
        ask_sizes: [0; 10],
        snapshot_flags: 0,
        dex_type: 1,
        _padding: [0; 110],
    }
}

/// Create a snapshot with N levels of depth
///
/// # Arguments
/// * `best_bid` - Best bid price (level 0)
/// * `best_ask` - Best ask price (level 0)
/// * `depth_levels` - Number of levels to populate (1-10)
/// * `tick_size` - Price increment between levels
/// * `size_per_level` - Size at each level (uniform)
///
/// # Example
/// ```ignore
/// let snapshot = create_depth_snapshot(
///     50_000_000_000_000,  // $50,000 bid
///     50_010_000_000_000,  // $50,010 ask (2bp spread)
///     5,                    // 5 levels
///     5_000_000_000,       // $5 tick size
///     100_000_000,         // 0.1 BTC per level
/// );
/// ```
#[cfg(test)]
pub fn create_depth_snapshot(
    best_bid: u64,
    best_ask: u64,
    depth_levels: usize,
    tick_size: u64,
    size_per_level: u64,
) -> MarketSnapshot {
    assert!(depth_levels >= 1 && depth_levels <= 10, "depth_levels must be 1-10");
    assert!(best_bid < best_ask, "bid must be < ask");

    let mut snapshot = create_basic_snapshot(best_bid, best_ask, size_per_level, size_per_level);

    // Populate bid levels (descending prices)
    for level in 0..depth_levels {
        snapshot.bid_prices[level] = best_bid.saturating_sub(level as u64 * tick_size);
        snapshot.bid_sizes[level] = size_per_level;
    }

    // Populate ask levels (ascending prices)
    for level in 0..depth_levels {
        snapshot.ask_prices[level] = best_ask + (level as u64 * tick_size);
        snapshot.ask_sizes[level] = size_per_level;
    }

    snapshot
}

/// Create a snapshot with orderbook imbalance
///
/// # Arguments
/// * `best_bid` - Best bid price
/// * `best_ask` - Best ask price
/// * `depth_levels` - Number of levels to populate
/// * `tick_size` - Price increment between levels
/// * `bid_heavy_ratio` - Ratio of bid to ask sizes (e.g., 3.0 = 3x more bids)
///
/// # Example
/// ```ignore
/// // Create snapshot with 3x more bid liquidity (bullish imbalance)
/// let snapshot = create_imbalanced_snapshot(
///     50_000_000_000_000,
///     50_010_000_000_000,
///     5,
///     5_000_000_000,
///     3.0,  // 3x more bids than asks
/// );
/// ```
#[cfg(test)]
pub fn create_imbalanced_snapshot(
    best_bid: u64,
    best_ask: u64,
    depth_levels: usize,
    tick_size: u64,
    bid_heavy_ratio: f64,
) -> MarketSnapshot {
    assert!(depth_levels >= 1 && depth_levels <= 10);
    assert!(best_bid < best_ask);
    assert!(bid_heavy_ratio > 0.0);

    let base_size = 100_000_000u64; // 0.1 BTC base

    // Calculate sizes based on ratio
    let bid_size = (base_size as f64 * bid_heavy_ratio) as u64;
    let ask_size = base_size;

    let mut snapshot = create_basic_snapshot(best_bid, best_ask, bid_size, ask_size);

    // Populate bid levels with larger sizes
    for level in 0..depth_levels {
        snapshot.bid_prices[level] = best_bid.saturating_sub(level as u64 * tick_size);
        snapshot.bid_sizes[level] = bid_size;
    }

    // Populate ask levels with smaller sizes
    for level in 0..depth_levels {
        snapshot.ask_prices[level] = best_ask + (level as u64 * tick_size);
        snapshot.ask_sizes[level] = ask_size;
    }

    snapshot
}

/// Create a snapshot with varying liquidity across levels
///
/// # Arguments
/// * `best_bid` - Best bid price
/// * `best_ask` - Best ask price
/// * `depth_levels` - Number of levels to populate
/// * `tick_size` - Price increment between levels
/// * `sizes` - Vec of sizes for each level (length must match depth_levels)
///
/// # Example
/// ```ignore
/// // Create snapshot with declining liquidity at each level
/// let snapshot = create_multi_level_snapshot(
///     50_000_000_000_000,
///     50_010_000_000_000,
///     3,
///     5_000_000_000,
///     vec![
///         1_000_000_000,  // Level 0: 1 BTC
///         500_000_000,    // Level 1: 0.5 BTC
///         250_000_000,    // Level 2: 0.25 BTC
///     ],
/// );
/// ```
#[cfg(test)]
pub fn create_multi_level_snapshot(
    best_bid: u64,
    best_ask: u64,
    depth_levels: usize,
    tick_size: u64,
    sizes: Vec<u64>,
) -> MarketSnapshot {
    assert!(depth_levels >= 1 && depth_levels <= 10);
    assert_eq!(sizes.len(), depth_levels, "sizes.len() must equal depth_levels");
    assert!(best_bid < best_ask);

    let mut snapshot = create_basic_snapshot(best_bid, best_ask, sizes[0], sizes[0]);

    // Populate bid levels
    for (level, &size) in sizes.iter().enumerate().take(depth_levels) {
        snapshot.bid_prices[level] = best_bid.saturating_sub(level as u64 * tick_size);
        snapshot.bid_sizes[level] = size;
    }

    // Populate ask levels
    for (level, &size) in sizes.iter().enumerate().take(depth_levels) {
        snapshot.ask_prices[level] = best_ask + (level as u64 * tick_size);
        snapshot.ask_sizes[level] = size;
    }

    snapshot
}

/// Create a snapshot with sparse depth (some levels empty)
///
/// Useful for testing edge cases where not all levels are populated
#[cfg(test)]
pub fn create_sparse_depth_snapshot(
    best_bid: u64,
    best_ask: u64,
    populated_levels: &[usize],
    tick_size: u64,
    size_per_level: u64,
) -> MarketSnapshot {
    assert!(best_bid < best_ask);

    let mut snapshot = create_basic_snapshot(best_bid, best_ask, size_per_level, size_per_level);

    // Only populate specified levels
    for &level in populated_levels {
        assert!(level < 10, "level must be < 10");

        snapshot.bid_prices[level] = best_bid.saturating_sub(level as u64 * tick_size);
        snapshot.bid_sizes[level] = size_per_level;
        snapshot.ask_prices[level] = best_ask + (level as u64 * tick_size);
        snapshot.ask_sizes[level] = size_per_level;
    }

    snapshot
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_basic_snapshot() {
        let snapshot = create_basic_snapshot(
            50_000_000_000_000,
            50_010_000_000_000,
            1_000_000_000,
            1_000_000_000,
        );

        assert_eq!(snapshot.best_bid_price, 50_000_000_000_000);
        assert_eq!(snapshot.best_ask_price, 50_010_000_000_000);
        assert_eq!(snapshot.best_bid_size, 1_000_000_000);
        assert_eq!(snapshot.best_ask_size, 1_000_000_000);

        // Depth should be empty
        assert_eq!(snapshot.bid_prices, [0; 10]);
        assert_eq!(snapshot.ask_prices, [0; 10]);
    }

    #[test]
    fn test_create_depth_snapshot_5_levels() {
        let snapshot = create_depth_snapshot(
            50_000_000_000_000,  // $50,000
            50_010_000_000_000,  // $50,010
            5,                    // 5 levels
            5_000_000_000,       // $5 tick
            100_000_000,         // 0.1 BTC
        );

        // Check bid levels descend
        assert_eq!(snapshot.bid_prices[0], 50_000_000_000_000);
        assert_eq!(snapshot.bid_prices[1], 49_995_000_000_000);
        assert_eq!(snapshot.bid_prices[2], 49_990_000_000_000);
        assert_eq!(snapshot.bid_prices[3], 49_985_000_000_000);
        assert_eq!(snapshot.bid_prices[4], 49_980_000_000_000);

        // Check ask levels ascend
        assert_eq!(snapshot.ask_prices[0], 50_010_000_000_000);
        assert_eq!(snapshot.ask_prices[1], 50_015_000_000_000);
        assert_eq!(snapshot.ask_prices[2], 50_020_000_000_000);
        assert_eq!(snapshot.ask_prices[3], 50_025_000_000_000);
        assert_eq!(snapshot.ask_prices[4], 50_030_000_000_000);

        // Levels 5-9 should be zero
        assert_eq!(snapshot.bid_prices[5], 0);
        assert_eq!(snapshot.ask_prices[5], 0);
    }

    #[test]
    fn test_create_imbalanced_snapshot_bullish() {
        let snapshot = create_imbalanced_snapshot(
            50_000_000_000_000,
            50_010_000_000_000,
            3,
            5_000_000_000,
            3.0,  // 3x more bids
        );

        // Bid sizes should be 3x ask sizes
        assert_eq!(snapshot.best_bid_size, 300_000_000);  // 0.3 BTC
        assert_eq!(snapshot.best_ask_size, 100_000_000);  // 0.1 BTC
        assert_eq!(snapshot.bid_sizes[0], 300_000_000);
        assert_eq!(snapshot.ask_sizes[0], 100_000_000);
    }

    #[test]
    fn test_create_multi_level_snapshot_varying_sizes() {
        let snapshot = create_multi_level_snapshot(
            50_000_000_000_000,
            50_010_000_000_000,
            3,
            5_000_000_000,
            vec![1_000_000_000, 500_000_000, 250_000_000],
        );

        // Check sizes decrease at each level
        assert_eq!(snapshot.bid_sizes[0], 1_000_000_000);
        assert_eq!(snapshot.bid_sizes[1], 500_000_000);
        assert_eq!(snapshot.bid_sizes[2], 250_000_000);
        assert_eq!(snapshot.ask_sizes[0], 1_000_000_000);
        assert_eq!(snapshot.ask_sizes[1], 500_000_000);
        assert_eq!(snapshot.ask_sizes[2], 250_000_000);
    }

    #[test]
    fn test_create_sparse_depth_snapshot() {
        let snapshot = create_sparse_depth_snapshot(
            50_000_000_000_000,
            50_010_000_000_000,
            &[0, 2, 5],  // Only levels 0, 2, 5 populated
            5_000_000_000,
            100_000_000,
        );

        // Level 0 populated
        assert_eq!(snapshot.bid_prices[0], 50_000_000_000_000);
        assert_eq!(snapshot.bid_sizes[0], 100_000_000);

        // Level 1 empty
        assert_eq!(snapshot.bid_prices[1], 0);
        assert_eq!(snapshot.bid_sizes[1], 0);

        // Level 2 populated
        assert_eq!(snapshot.bid_prices[2], 49_990_000_000_000);
        assert_eq!(snapshot.bid_sizes[2], 100_000_000);

        // Level 5 populated
        assert_eq!(snapshot.bid_prices[5], 49_975_000_000_000);
        assert_eq!(snapshot.bid_sizes[5], 100_000_000);
    }

    #[test]
    #[should_panic(expected = "bid must be < ask")]
    fn test_crossed_orderbook_panics() {
        create_depth_snapshot(
            50_010_000_000_000,  // bid > ask (invalid)
            50_000_000_000_000,
            5,
            5_000_000_000,
            100_000_000,
        );
    }

    #[test]
    #[should_panic(expected = "depth_levels must be 1-10")]
    fn test_invalid_depth_levels_panics() {
        create_depth_snapshot(
            50_000_000_000_000,
            50_010_000_000_000,
            11,  // Invalid: > 10
            5_000_000_000,
            100_000_000,
        );
    }
}
