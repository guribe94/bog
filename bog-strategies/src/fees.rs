/// Exchange fee configuration for profitable market making
///
/// Fees are expressed in sub-basis points (1 sub-bps = 0.01 bps = 0.0001% = 0.000001)
/// This allows precise representation of fractional bps fees like Lighter DEX's 0.2 bps.
///
/// Scale factors:
/// - BPS_SCALE = 10,000 (for basis points)
/// - SUB_BPS_SCALE = 1,000,000 (for sub-basis points, 100x more precise)
///
/// # Lighter DEX Fee Structure
/// - Maker fee: 0.2 bps = 20 sub-bps (0.002%)
/// - Taker fee: 2 bps = 200 sub-bps (0.02%)
/// - Round-trip cost: 2.2 bps = 220 sub-bps
///
/// # Configuration via Cargo Features
/// Fees can be overridden at compile time:
/// ```bash
/// cargo build --features maker-fee-5bps,taker-fee-10bps
/// ```

use bog_core::config::BPS_SCALE;

/// Scale factor for sub-basis point calculations
/// 1 sub-bps = 0.01 bps = 0.0001% = 0.000001
pub const SUB_BPS_SCALE: u128 = 1_000_000;

/// Maker fee in sub-basis points (fee paid when posting passive liquidity)
///
/// 1 sub-bps = 0.01 bps = 0.0001%
/// Default: 20 sub-bps = 0.2 bps (Lighter DEX maker fee)
///
/// Available feature flags:
/// - `maker-fee-0bps` (0 sub-bps)
/// - `maker-fee-1bps` (100 sub-bps)
/// - `maker-fee-2bps` (200 sub-bps)
/// - `maker-fee-5bps` (500 sub-bps)
/// - `maker-fee-10bps` (1000 sub-bps)
pub const MAKER_FEE_SUB_BPS: u32 = {
    #[cfg(feature = "maker-fee-10bps")]
    {
        1000 // 10 bps = 1000 sub-bps
    }
    #[cfg(feature = "maker-fee-5bps")]
    {
        500 // 5 bps = 500 sub-bps
    }
    #[cfg(feature = "maker-fee-2bps")]
    {
        200 // 2 bps = 200 sub-bps
    }
    #[cfg(feature = "maker-fee-1bps")]
    {
        100 // 1 bps = 100 sub-bps
    }
    #[cfg(feature = "maker-fee-0bps")]
    {
        0 // 0 bps = 0 sub-bps
    }
    #[cfg(not(any(
        feature = "maker-fee-0bps",
        feature = "maker-fee-1bps",
        feature = "maker-fee-2bps",
        feature = "maker-fee-5bps",
        feature = "maker-fee-10bps"
    )))]
    {
        20 // Lighter default: 0.2 bps = 20 sub-bps
    }
};

/// Taker fee in sub-basis points (fee paid when taking liquidity / getting filled)
///
/// 1 sub-bps = 0.01 bps = 0.0001%
/// Default: 200 sub-bps = 2 bps (Lighter DEX taker fee)
///
/// Available feature flags:
/// - `taker-fee-2bps` (default - Lighter, 200 sub-bps)
/// - `taker-fee-5bps` (500 sub-bps)
/// - `taker-fee-10bps` (1000 sub-bps)
/// - `taker-fee-20bps` (2000 sub-bps)
/// - `taker-fee-30bps` (3000 sub-bps)
pub const TAKER_FEE_SUB_BPS: u32 = {
    #[cfg(feature = "taker-fee-30bps")]
    {
        3000 // 30 bps = 3000 sub-bps
    }
    #[cfg(feature = "taker-fee-20bps")]
    {
        2000 // 20 bps = 2000 sub-bps
    }
    #[cfg(feature = "taker-fee-10bps")]
    {
        1000 // 10 bps = 1000 sub-bps
    }
    #[cfg(feature = "taker-fee-5bps")]
    {
        500 // 5 bps = 500 sub-bps
    }
    #[cfg(not(any(
        feature = "taker-fee-5bps",
        feature = "taker-fee-10bps",
        feature = "taker-fee-20bps",
        feature = "taker-fee-30bps"
    )))]
    {
        200 // Lighter default: 2 bps = 200 sub-bps
    }
};

/// Total round-trip cost in sub-basis points
///
/// When market making, we pay:
/// - Maker fee when our quote is posted
/// - Taker fee when we exit the position
///
/// For profitability: `spread > ROUND_TRIP_COST_SUB_BPS`
///
/// Lighter DEX: 20 + 200 = 220 sub-bps = 2.2 bps
pub const ROUND_TRIP_COST_SUB_BPS: u32 = MAKER_FEE_SUB_BPS + TAKER_FEE_SUB_BPS;

/// Minimum profitable spread in sub-basis points
///
/// This is the minimum spread needed to break even after fees.
/// Strategies should target spreads wider than this.
///
/// # Example
/// ```
/// use bog_strategies::fees::MIN_PROFITABLE_SPREAD_SUB_BPS;
///
/// // For Lighter (20 + 200 = 220 sub-bps = 2.2 bps round-trip):
/// assert_eq!(MIN_PROFITABLE_SPREAD_SUB_BPS, 220);
///
/// // Strategy should target wider spreads:
/// let target_spread_sub_bps = MIN_PROFITABLE_SPREAD_SUB_BPS + 300; // 520 sub-bps = 5.2 bps
/// ```
pub const MIN_PROFITABLE_SPREAD_SUB_BPS: u32 = ROUND_TRIP_COST_SUB_BPS;

/// Calculate fee amount in fixed-point u64 using sub-basis points
///
/// Given a price and fee in sub-basis points, calculates the fee amount.
///
/// # Arguments
/// * `price` - Price in u64 fixed-point (9 decimals)
/// * `fee_sub_bps` - Fee in sub-basis points (1 sub-bps = 0.01 bps = 0.0001%)
///
/// # Returns
/// Fee amount in u64 fixed-point (9 decimals)
///
/// # Example
/// ```ignore
/// let price = 50_000_000_000_000; // $50,000
/// let fee = calculate_fee_sub_bps(price, 200); // 200 sub-bps = 2 bps
/// // fee = 50_000 * 0.0002 = $10
/// assert_eq!(fee, 10_000_000_000);
///
/// // For maker fee (0.2 bps = 20 sub-bps):
/// let maker_fee = calculate_fee_sub_bps(price, 20);
/// // fee = 50_000 * 0.00002 = $1
/// assert_eq!(maker_fee, 1_000_000_000);
/// ```
#[inline(always)]
pub fn calculate_fee_sub_bps(price: u64, fee_sub_bps: u32) -> u64 {
    // price * fee_sub_bps / SUB_BPS_SCALE
    // Use u128 to prevent overflow
    let fee = (price as u128 * fee_sub_bps as u128) / SUB_BPS_SCALE;
    fee as u64
}

/// Calculate fee amount in fixed-point u64 using basis points (legacy)
///
/// Given a price and fee in basis points, calculates the fee amount.
/// For new code, prefer `calculate_fee_sub_bps` for fractional bps precision.
///
/// # Arguments
/// * `price` - Price in u64 fixed-point (9 decimals)
/// * `fee_bps` - Fee in basis points (1 bp = 0.01%)
///
/// # Returns
/// Fee amount in u64 fixed-point (9 decimals)
#[inline(always)]
pub fn calculate_fee(price: u64, fee_bps: u32) -> u64 {
    // price * fee_bps / BPS_SCALE
    // Use u128 to prevent overflow
    let fee = (price as u128 * fee_bps as u128) / BPS_SCALE as u128;
    fee as u64
}

/// Calculate required spread for target profit (in sub-basis points)
///
/// Given a desired profit margin in sub-basis points, calculates the total
/// spread needed to achieve that profit after fees.
///
/// # Formula
/// ```text
/// required_spread = round_trip_fees + target_profit
/// ```
///
/// # Arguments
/// * `target_profit_sub_bps` - Desired profit in sub-basis points
///
/// # Returns
/// Required spread in sub-basis points
///
/// # Example
/// ```
/// use bog_strategies::fees::calculate_required_spread_sub_bps;
///
/// // Want 300 sub-bps (3 bps) profit after fees
/// let spread = calculate_required_spread_sub_bps(300);
/// // For Lighter: 220 (fees) + 300 (profit) = 520 sub-bps = 5.2 bps
/// assert_eq!(spread, 520);
/// ```
#[inline(always)]
pub const fn calculate_required_spread_sub_bps(target_profit_sub_bps: u32) -> u32 {
    ROUND_TRIP_COST_SUB_BPS + target_profit_sub_bps
}

/// Calculate bid/ask prices with fee-aware spread (in sub-basis points)
///
/// Given a mid price and target spread in sub-basis points, calculates bid/ask prices that
/// ensure profitability after fees.
///
/// # Arguments
/// * `mid_price` - Mid market price in u64 fixed-point
/// * `target_spread_sub_bps` - Desired total spread in sub-basis points
///
/// # Returns
/// `(bid_price, ask_price)` tuple in u64 fixed-point
///
/// # Panic
/// Panics if `target_spread_sub_bps < MIN_PROFITABLE_SPREAD_SUB_BPS`
///
/// # Example
/// ```ignore
/// let mid = 50_000_000_000_000; // $50,000
/// let (bid, ask) = calculate_quotes_sub_bps(mid, 500); // 500 sub-bps = 5 bps spread
///
/// // bid = mid * (1 - 0.00025) = $49,987.50
/// // ask = mid * (1 + 0.00025) = $50,012.50
/// ```
#[inline(always)]
pub fn calculate_quotes_sub_bps(mid_price: u64, target_spread_sub_bps: u32) -> (u64, u64) {
    debug_assert!(
        target_spread_sub_bps >= MIN_PROFITABLE_SPREAD_SUB_BPS,
        "Spread too narrow: {} < {} (min profitable)",
        target_spread_sub_bps,
        MIN_PROFITABLE_SPREAD_SUB_BPS
    );

    // Half-spread in sub-bps
    let half_spread_sub_bps = target_spread_sub_bps / 2;

    // Calculate bid: mid * (1 - half_spread / SUB_BPS_SCALE)
    let bid_adjustment = (mid_price as u128 * half_spread_sub_bps as u128) / SUB_BPS_SCALE;
    let bid_price = mid_price.saturating_sub(bid_adjustment as u64);

    // Calculate ask: mid * (1 + half_spread / SUB_BPS_SCALE)
    let ask_adjustment = (mid_price as u128 * half_spread_sub_bps as u128) / SUB_BPS_SCALE;
    let ask_price = mid_price.saturating_add(ask_adjustment as u64);

    (bid_price, ask_price)
}

/// Calculate bid/ask prices with fee-aware spread (in basis points, legacy)
///
/// Given a mid price and target spread, calculates bid/ask prices that
/// ensure profitability after fees.
/// For new code, prefer `calculate_quotes_sub_bps` for fractional bps precision.
///
/// # Arguments
/// * `mid_price` - Mid market price in u64 fixed-point
/// * `target_spread_bps` - Desired total spread in basis points
///
/// # Returns
/// `(bid_price, ask_price)` tuple in u64 fixed-point
#[inline(always)]
pub fn calculate_quotes(mid_price: u64, target_spread_bps: u32) -> (u64, u64) {
    // Convert bps to sub-bps and delegate
    calculate_quotes_sub_bps(mid_price, target_spread_bps * 100)
}

// ===== BACKWARD COMPATIBILITY ALIASES =====
// These aliases allow existing code to compile without changes.
// New code should use the _SUB_BPS variants for fractional precision.

/// Backward-compatible alias: use MAKER_FEE_SUB_BPS for new code
/// This converts sub-bps to bps (divides by 100, rounds down)
/// Warning: Loses fractional precision (0.2 bps becomes 0 bps)
pub const MAKER_FEE_BPS: u32 = MAKER_FEE_SUB_BPS / 100;

/// Backward-compatible alias: use TAKER_FEE_SUB_BPS for new code
/// This converts sub-bps to bps (divides by 100)
pub const TAKER_FEE_BPS: u32 = TAKER_FEE_SUB_BPS / 100;

/// Backward-compatible alias: use ROUND_TRIP_COST_SUB_BPS for new code
/// This converts sub-bps to bps (divides by 100, rounds down)
pub const ROUND_TRIP_COST_BPS: u32 = ROUND_TRIP_COST_SUB_BPS / 100;

/// Backward-compatible alias: use MIN_PROFITABLE_SPREAD_SUB_BPS for new code
/// This converts sub-bps to bps (divides by 100, rounds down)
pub const MIN_PROFITABLE_SPREAD_BPS: u32 = MIN_PROFITABLE_SPREAD_SUB_BPS / 100;

/// Backward-compatible alias: use calculate_required_spread_sub_bps for new code
#[inline(always)]
pub const fn calculate_required_spread(target_profit_bps: u32) -> u32 {
    (ROUND_TRIP_COST_SUB_BPS + target_profit_bps * 100) / 100
}

#[cfg(test)]
mod tests {
    use super::*;
    use bog_core::config::DEFAULT_FEE_SUB_BPS;

    #[test]
    fn test_fee_constants() {
        // Lighter default configuration
        #[cfg(not(any(
            feature = "maker-fee-0bps",
            feature = "maker-fee-1bps",
            feature = "maker-fee-2bps",
            feature = "maker-fee-5bps",
            feature = "maker-fee-10bps"
        )))]
        {
            // Default maker fee: 0.2 bps = 20 sub-bps
            assert_eq!(MAKER_FEE_SUB_BPS, 20);
        }

        #[cfg(not(any(
            feature = "taker-fee-5bps",
            feature = "taker-fee-10bps",
            feature = "taker-fee-20bps",
            feature = "taker-fee-30bps"
        )))]
        {
            // Default taker fee: 2 bps = 200 sub-bps
            assert_eq!(TAKER_FEE_SUB_BPS, 200);
        }

        // Round-trip cost
        assert_eq!(
            ROUND_TRIP_COST_SUB_BPS,
            MAKER_FEE_SUB_BPS + TAKER_FEE_SUB_BPS
        );
        assert_eq!(MIN_PROFITABLE_SPREAD_SUB_BPS, ROUND_TRIP_COST_SUB_BPS);

        // Cross-crate consistency: under default features, the maker fee
        // should match the core DEFAULT_FEE_SUB_BPS used by the engine.
        #[cfg(not(any(
            feature = "maker-fee-0bps",
            feature = "maker-fee-1bps",
            feature = "maker-fee-2bps",
            feature = "maker-fee-5bps",
            feature = "maker-fee-10bps",
            feature = "taker-fee-5bps",
            feature = "taker-fee-10bps",
            feature = "taker-fee-20bps",
            feature = "taker-fee-30bps"
        )))]
        {
            // Core uses maker fee for market making (20 sub-bps)
            assert_eq!(MAKER_FEE_SUB_BPS, DEFAULT_FEE_SUB_BPS);
        }
    }

    #[test]
    fn test_calculate_fee_sub_bps() {
        let price = 50_000_000_000_000; // $50,000

        // 200 sub-bps = 2 bps fee
        let fee = calculate_fee_sub_bps(price, 200);
        // Expected: 50_000 * 0.0002 = $10 = 10_000_000_000
        assert_eq!(fee, 10_000_000_000);

        // 20 sub-bps = 0.2 bps fee (Lighter maker fee)
        let fee = calculate_fee_sub_bps(price, 20);
        // Expected: 50_000 * 0.00002 = $1 = 1_000_000_000
        assert_eq!(fee, 1_000_000_000);

        // 1000 sub-bps = 10 bps fee
        let fee = calculate_fee_sub_bps(price, 1000);
        // Expected: 50_000 * 0.001 = $50 = 50_000_000_000
        assert_eq!(fee, 50_000_000_000);
    }

    #[test]
    fn test_calculate_fee_legacy() {
        let price = 50_000_000_000_000; // $50,000

        // 2 bps fee (legacy function)
        let fee = calculate_fee(price, 2);
        // Expected: 50_000 * 0.0002 = $10 = 10_000_000_000
        assert_eq!(fee, 10_000_000_000);

        // 10 bps fee
        let fee = calculate_fee(price, 10);
        // Expected: 50_000 * 0.001 = $50 = 50_000_000_000
        assert_eq!(fee, 50_000_000_000);
    }

    #[test]
    fn test_calculate_required_spread() {
        // Want 300 sub-bps (3 bps) profit
        let spread = calculate_required_spread_sub_bps(300);
        // For Lighter default (220 sub-bps fees): 220 + 300 = 520 sub-bps
        assert_eq!(spread, ROUND_TRIP_COST_SUB_BPS + 300);

        // Want 1000 sub-bps (10 bps) profit
        let spread = calculate_required_spread_sub_bps(1000);
        assert_eq!(spread, ROUND_TRIP_COST_SUB_BPS + 1000);
    }

    #[test]
    fn test_calculate_quotes_sub_bps() {
        let mid = 50_000_000_000_000; // $50,000

        // 1000 sub-bps = 10 bps spread (500 sub-bps = 5 bps each side)
        let (bid, ask) = calculate_quotes_sub_bps(mid, 1000);

        // bid = 50_000 * (1 - 0.0005) = 49_975
        // ask = 50_000 * (1 + 0.0005) = 50_025
        assert_eq!(bid, 49_975_000_000_000);
        assert_eq!(ask, 50_025_000_000_000);

        // Verify spread
        let actual_spread = ask - bid;
        let actual_spread_sub_bps = (actual_spread as u128 * 1_000_000) / mid as u128;
        assert_eq!(actual_spread_sub_bps, 1000);
    }

    #[test]
    fn test_calculate_quotes_legacy() {
        let mid = 50_000_000_000_000; // $50,000

        // 10 bps spread (legacy function)
        let (bid, ask) = calculate_quotes(mid, 10);

        // bid = 50_000 * (1 - 0.0005) = 49_975
        // ask = 50_000 * (1 + 0.0005) = 50_025
        assert_eq!(bid, 49_975_000_000_000);
        assert_eq!(ask, 50_025_000_000_000);
    }

    #[test]
    fn test_quotes_profitability() {
        let mid = 50_000_000_000_000; // $50,000

        // Use minimum profitable spread
        let (bid, ask) = calculate_quotes_sub_bps(mid, MIN_PROFITABLE_SPREAD_SUB_BPS);

        // Calculate expected profit
        let spread = ask - bid;
        let spread_sub_bps = (spread as u128 * 1_000_000) / mid as u128;

        // Profit after fees should be >= 0
        assert!(spread_sub_bps >= ROUND_TRIP_COST_SUB_BPS as u128);
    }

    #[test]
    fn test_fee_calculations_precision() {
        // Test with very small price
        let small_price = 1_000_000; // $0.001
        let fee = calculate_fee_sub_bps(small_price, 1000);
        // Should not underflow
        assert!(fee < small_price);

        // Test with very large price
        let large_price = 100_000_000_000_000_000; // $100,000,000
        let fee = calculate_fee(large_price, 10);
        // Should not overflow (using u128 internally)
        assert!(fee > 0);
    }
}
