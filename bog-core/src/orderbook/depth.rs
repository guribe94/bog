/// Orderbook depth analysis for MarketSnapshot
///
/// Zero-allocation, fixed-point calculations on shared memory data.
/// All prices and sizes are u64 fixed-point (9 decimal places).
///
/// **Performance target:** <10ns per calculation

use huginn::shm::{MarketSnapshot, ORDERBOOK_DEPTH};

/// Calculate Volume-Weighted Average Price (VWAP) across depth levels
///
/// Returns VWAP for bid or ask side up to `max_levels` (1-10).
/// Uses fixed-point u64 arithmetic (9 decimal places).
///
/// # Arguments
/// * `snapshot` - Market data with 10 levels of depth
/// * `is_bid` - true for bid side, false for ask side
/// * `max_levels` - How many levels to include (1-10)
///
/// # Returns
/// * `Some(vwap_price)` - Weighted average price in u64 fixed-point
/// * `None` - If no liquidity on that side
///
/// # Performance
/// Target: <5ns (inline, no branches in hot path)
///
/// # Example
/// ```ignore
/// let bid_vwap = calculate_vwap(&snapshot, true, 5);
/// let ask_vwap = calculate_vwap(&snapshot, false, 5);
/// ```
#[inline(always)]
pub fn calculate_vwap(
    snapshot: &MarketSnapshot,
    is_bid: bool,
    max_levels: usize,
) -> Option<u64> {
    let max_levels = max_levels.min(ORDERBOOK_DEPTH); // Clamp to valid range

    let (prices, sizes) = if is_bid {
        (&snapshot.bid_prices, &snapshot.bid_sizes)
    } else {
        (&snapshot.ask_prices, &snapshot.ask_sizes)
    };

    let mut total_value: u128 = 0;
    let mut total_size: u128 = 0;

    // Accumulate price * size for all non-zero levels
    for i in 0..max_levels {
        let size = sizes[i];
        if size == 0 {
            break; // No more liquidity
        }

        let price = prices[i];
        // Both price and size are in 9 decimal places
        // price * size = value with 18 decimal places
        // We keep 18 decimals for precision during accumulation
        total_value += price as u128 * size as u128;
        total_size += size as u128;
    }

    if total_size == 0 {
        return None;
    }

    // VWAP = total_value / total_size
    // total_value has 18 decimals (price * size)
    // total_size has 9 decimals
    // Division gives us 9 decimals (18 - 9 = 9)
    let vwap = (total_value / total_size) as u64;
    Some(vwap)
}

/// Calculate orderbook imbalance ratio
///
/// Measures buy pressure vs sell pressure based on size at depth.
///
/// # Formula
/// ```text
/// imbalance = (bid_volume - ask_volume) / (bid_volume + ask_volume)
/// ```
///
/// # Arguments
/// * `snapshot` - Market data with depth
/// * `max_levels` - How many levels to consider (1-10)
///
/// # Returns
/// * Imbalance ratio as i64 fixed-point (9 decimals)
///   - `+1.0` (1_000_000_000) = 100% bid pressure
///   - `0.0` (0) = Balanced
///   - `-1.0` (-1_000_000_000) = 100% ask pressure
///
/// # Performance
/// Target: <8ns
///
/// # Example
/// ```ignore
/// let imbalance = calculate_imbalance(&snapshot, 5);
/// if imbalance > 200_000_000 { // > 0.2
///     // Strong bid pressure, expect price to rise
/// }
/// ```
#[inline(always)]
pub fn calculate_imbalance(
    snapshot: &MarketSnapshot,
    max_levels: usize,
) -> i64 {
    let max_levels = max_levels.min(ORDERBOOK_DEPTH);

    let mut bid_volume: u128 = 0;
    let mut ask_volume: u128 = 0;

    // Sum bid sizes
    for i in 0..max_levels {
        let size = snapshot.bid_sizes[i];
        if size == 0 {
            break;
        }
        bid_volume += size as u128;
    }

    // Sum ask sizes
    for i in 0..max_levels {
        let size = snapshot.ask_sizes[i];
        if size == 0 {
            break;
        }
        ask_volume += size as u128;
    }

    let total = bid_volume + ask_volume;
    if total == 0 {
        return 0; // No liquidity
    }

    // Calculate (bid - ask) / (bid + ask)
    // Result in range [-1.0, +1.0]
    let numerator = bid_volume as i128 - ask_volume as i128;
    let denominator = total as i128;

    // Scale to 9 decimal places
    // numerator and denominator are both in size units (already 9 decimals)
    // Division gives dimensionless ratio, so multiply by 1e9 to get fixed-point
    const SCALE: i128 = 1_000_000_000;
    let imbalance = (numerator * SCALE) / denominator;

    imbalance as i64
}

/// Calculate total liquidity (volume) at depth
///
/// Returns total size available on bid or ask side up to `max_levels`.
///
/// # Arguments
/// * `snapshot` - Market data
/// * `is_bid` - true for bid side, false for ask
/// * `max_levels` - How many levels to sum (1-10)
///
/// # Returns
/// Total size in u64 fixed-point (9 decimals)
/// If total would overflow u64::MAX, returns u64::MAX
///
/// # Performance
/// Target: <3ns
#[inline(always)]
pub fn calculate_liquidity(
    snapshot: &MarketSnapshot,
    is_bid: bool,
    max_levels: usize,
) -> u64 {
    let max_levels = max_levels.min(ORDERBOOK_DEPTH);

    let sizes = if is_bid {
        &snapshot.bid_sizes
    } else {
        &snapshot.ask_sizes
    };

    // Use u128 internally to detect overflow
    let mut total: u128 = 0;
    for i in 0..max_levels {
        let size = sizes[i];
        if size == 0 {
            break;
        }
        total += size as u128;
    }

    // Clamp to u64::MAX if overflow would occur
    // This is better than silent wrapping with saturating_add
    if total > u64::MAX as u128 {
        u64::MAX
    } else {
        total as u64
    }
}

/// Calculate mid price from best bid/ask
///
/// # Returns
/// Mid price in u64 fixed-point (9 decimals)
#[inline(always)]
pub fn mid_price(snapshot: &MarketSnapshot) -> u64 {
    (snapshot.best_bid_price + snapshot.best_ask_price) / 2
}

/// Calculate spread in basis points
///
/// # Returns
/// Spread as u32 in basis points (1 bp = 0.01%)
#[inline(always)]
pub fn spread_bps(snapshot: &MarketSnapshot) -> u32 {
    spread_bps_from_prices(snapshot.best_bid_price, snapshot.best_ask_price)
}

/// Calculate spread in basis points from raw prices
///
/// Helper function for L2OrderBook that works on raw u64 prices.
#[inline(always)]
pub fn spread_bps_from_prices(bid_price: u64, ask_price: u64) -> u32 {
    if bid_price == 0 {
        return 0;
    }

    let spread = ask_price.saturating_sub(bid_price);
    let spread_bps = (spread as u128 * 10_000) / bid_price as u128;
    spread_bps as u32
}

/// Calculate VWAP from raw price and size arrays
///
/// Helper function for L2OrderBook that works on raw arrays.
#[inline]
pub fn calculate_vwap_u64(
    prices: &[u64],
    sizes: &[u64],
    max_levels: usize,
) -> Option<u64> {
    let max_levels = max_levels.min(prices.len().min(sizes.len()));

    let mut total_value: u128 = 0;
    let mut total_size: u128 = 0;

    for i in 0..max_levels {
        let size = sizes[i];
        if size == 0 {
            break;
        }

        let price = prices[i];
        total_value += price as u128 * size as u128;
        total_size += size as u128;
    }

    if total_size == 0 {
        return None;
    }

    Some((total_value / total_size) as u64)
}

/// Calculate orderbook imbalance from raw arrays (returns i64 -100 to +100)
///
/// Helper function for L2OrderBook that works on raw arrays.
/// Returns imbalance scaled to -100 to +100 range instead of -1.0 to +1.0.
#[inline]
pub fn calculate_imbalance_i64(
    bid_prices: &[u64],
    bid_sizes: &[u64],
    ask_prices: &[u64],
    ask_sizes: &[u64],
    max_levels: usize,
) -> i64 {
    let max_levels = max_levels.min(bid_prices.len().min(ask_prices.len()));

    let mut bid_volume: u128 = 0;
    let mut ask_volume: u128 = 0;

    for i in 0..max_levels {
        if bid_sizes[i] > 0 {
            bid_volume += bid_sizes[i] as u128;
        }
        if ask_sizes[i] > 0 {
            ask_volume += ask_sizes[i] as u128;
        }
    }

    if bid_volume == 0 && ask_volume == 0 {
        return 0; // No liquidity
    }

    let total = bid_volume + ask_volume;
    if total == 0 {
        return 0;
    }

    // Imbalance = (bid - ask) / total * 100
    // Returns -100 to +100
    let bid_i128 = bid_volume as i128;
    let ask_i128 = ask_volume as i128;
    let total_i128 = total as i128;

    let imbalance = ((bid_i128 - ask_i128) * 100) / total_i128;
    imbalance as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_snapshot() -> MarketSnapshot {
        MarketSnapshot {
            market_id: 1,
            sequence: 100,
            exchange_timestamp_ns: 0,
            local_recv_ns: 0,
            local_publish_ns: 0,
            best_bid_price: 50_000_000_000_000, // $50,000
            best_bid_size: 1_000_000_000,       // 1.0 BTC
            best_ask_price: 50_010_000_000_000, // $50,010 (10 bps spread)
            best_ask_size: 1_000_000_000,
            // Level 1-10 bids (prices descending)
            bid_prices: [
                49_990_000_000_000, // -10 from best
                49_980_000_000_000,
                49_970_000_000_000,
                49_960_000_000_000,
                49_950_000_000_000,
                0, 0, 0, 0, 0, // No depth beyond level 5
            ],
            bid_sizes: [
                2_000_000_000, // 2.0 BTC at level 1
                3_000_000_000, // 3.0 BTC
                1_000_000_000, // 1.0 BTC
                500_000_000,   // 0.5 BTC
                500_000_000,   // 0.5 BTC
                0, 0, 0, 0, 0,
            ],
            // Level 1-10 asks (prices ascending)
            ask_prices: [
                50_020_000_000_000, // +10 from best
                50_030_000_000_000,
                50_040_000_000_000,
                0, 0, 0, 0, 0, 0, 0, // Only 3 levels
            ],
            ask_sizes: [
                1_500_000_000, // 1.5 BTC
                1_000_000_000, // 1.0 BTC
                500_000_000,   // 0.5 BTC
                0, 0, 0, 0, 0, 0, 0,
            ],
            dex_type: 1,
            ..Default::default()
        }
    }

    #[test]
    fn test_vwap_calculation() {
        let snapshot = create_test_snapshot();

        // Calculate VWAP for top 3 bid levels
        // Level 1: 49_990 * 2.0 = 99_980
        // Level 2: 49_980 * 3.0 = 149_940
        // Level 3: 49_970 * 1.0 = 49_970
        // Total: 299_890 / 6.0 = 49_981.67
        let bid_vwap = calculate_vwap(&snapshot, true, 3).unwrap();

        // Expected: 49_981.666... ≈ 49_981_666_666_666
        assert!((bid_vwap as i64 - 49_981_666_666_666_i64).abs() < 1_000_000);

        // Calculate VWAP for ask side
        let ask_vwap = calculate_vwap(&snapshot, false, 3).unwrap();

        // Level 1: 50_020 * 1.5 = 75_030
        // Level 2: 50_030 * 1.0 = 50_030
        // Level 3: 50_040 * 0.5 = 25_020
        // Total: 150_080 / 3.0 = 50_026.67
        assert!((ask_vwap as i64 - 50_026_666_666_666_i64).abs() < 1_000_000);
    }

    #[test]
    fn test_imbalance_calculation() {
        let snapshot = create_test_snapshot();

        // Bid volume (5 levels): 2.0 + 3.0 + 1.0 + 0.5 + 0.5 = 7.0 BTC
        // Ask volume (3 levels): 1.5 + 1.0 + 0.5 = 3.0 BTC
        // Imbalance: (7.0 - 3.0) / (7.0 + 3.0) = 4.0 / 10.0 = 0.4
        let imbalance = calculate_imbalance(&snapshot, 10);

        // Expected: 0.4 = 400_000_000
        assert_eq!(imbalance, 400_000_000);
    }

    #[test]
    fn test_liquidity_calculation() {
        let snapshot = create_test_snapshot();

        // Bid liquidity (5 levels): 7.0 BTC = 7_000_000_000
        let bid_liq = calculate_liquidity(&snapshot, true, 10);
        assert_eq!(bid_liq, 7_000_000_000);

        // Ask liquidity (3 levels): 3.0 BTC = 3_000_000_000
        let ask_liq = calculate_liquidity(&snapshot, false, 10);
        assert_eq!(ask_liq, 3_000_000_000);
    }

    #[test]
    fn test_mid_price() {
        let snapshot = create_test_snapshot();

        // Mid = (50_000 + 50_010) / 2 = 50_005
        let mid = mid_price(&snapshot);
        assert_eq!(mid, 50_005_000_000_000);
    }

    #[test]
    fn test_spread_bps() {
        let snapshot = create_test_snapshot();

        // Spread: 50_010 - 50_000 = 10
        // BPS: 10 / 50_000 * 10_000 = 2 bps
        let spread = spread_bps(&snapshot);
        assert_eq!(spread, 2);
    }

    #[test]
    fn test_empty_orderbook() {
        let mut snapshot = create_test_snapshot();
        snapshot.bid_sizes = [0; 10];
        snapshot.ask_sizes = [0; 10];

        // VWAP should return None
        assert_eq!(calculate_vwap(&snapshot, true, 5), None);
        assert_eq!(calculate_vwap(&snapshot, false, 5), None);

        // Imbalance should be 0
        assert_eq!(calculate_imbalance(&snapshot, 5), 0);

        // Liquidity should be 0
        assert_eq!(calculate_liquidity(&snapshot, true, 5), 0);
        assert_eq!(calculate_liquidity(&snapshot, false, 5), 0);
    }
}
