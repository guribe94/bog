/// Exchange fee configuration for profitable market making
///
/// Fees are expressed in basis points (bps) where 1 bp = 0.01% = 0.0001
/// All calculations use u64 fixed-point with scale factor 10,000 for bps.
///
/// # Lighter DEX Fee Structure (Premium Accounts)
/// - Maker fee: 0.2 bps (essentially free)
/// - Taker fee: 2 bps
/// - Round-trip cost: 2.2 bps
///
/// # Configuration via Cargo Features
/// Fees can be overridden at compile time:
/// ```bash
/// cargo build --features maker-fee-5bps,taker-fee-10bps
/// ```

/// Maker fee in basis points (fee paid when posting passive liquidity)
///
/// Default: 0 bps (Lighter's 0.2 bps rounds to 0 for our calculations)
///
/// Available feature flags:
/// - `maker-fee-0bps` (default)
/// - `maker-fee-1bps`
/// - `maker-fee-2bps`
/// - `maker-fee-5bps`
/// - `maker-fee-10bps`
pub const MAKER_FEE_BPS: u32 = {
    #[cfg(feature = "maker-fee-10bps")]
    { 10 }
    #[cfg(feature = "maker-fee-5bps")]
    { 5 }
    #[cfg(feature = "maker-fee-2bps")]
    { 2 }
    #[cfg(feature = "maker-fee-1bps")]
    { 1 }
    #[cfg(not(any(
        feature = "maker-fee-1bps",
        feature = "maker-fee-2bps",
        feature = "maker-fee-5bps",
        feature = "maker-fee-10bps"
    )))]
    { 0 } // Lighter default: 0.2 bps rounds to 0
};

/// Taker fee in basis points (fee paid when taking liquidity / getting filled)
///
/// Default: 2 bps (Lighter DEX standard)
///
/// Available feature flags:
/// - `taker-fee-2bps` (default - Lighter)
/// - `taker-fee-5bps`
/// - `taker-fee-10bps`
/// - `taker-fee-20bps`
/// - `taker-fee-30bps`
pub const TAKER_FEE_BPS: u32 = {
    #[cfg(feature = "taker-fee-30bps")]
    { 30 }
    #[cfg(feature = "taker-fee-20bps")]
    { 20 }
    #[cfg(feature = "taker-fee-10bps")]
    { 10 }
    #[cfg(feature = "taker-fee-5bps")]
    { 5 }
    #[cfg(not(any(
        feature = "taker-fee-5bps",
        feature = "taker-fee-10bps",
        feature = "taker-fee-20bps",
        feature = "taker-fee-30bps"
    )))]
    { 2 } // Lighter default
};

/// Total round-trip cost in basis points
///
/// When market making, we pay:
/// - Maker fee when our quote is posted
/// - Taker fee when we exit the position
///
/// For profitability: `spread > ROUND_TRIP_COST_BPS`
pub const ROUND_TRIP_COST_BPS: u32 = MAKER_FEE_BPS + TAKER_FEE_BPS;

/// Minimum profitable spread in basis points
///
/// This is the minimum spread needed to break even after fees.
/// Strategies should target spreads wider than this.
///
/// # Example
/// ```
/// use bog_strategies::fees::MIN_PROFITABLE_SPREAD_BPS;
///
/// // For Lighter (0 + 2 = 2 bps round-trip):
/// assert_eq!(MIN_PROFITABLE_SPREAD_BPS, 2);
///
/// // Strategy should target wider spreads:
/// let target_spread_bps = MIN_PROFITABLE_SPREAD_BPS + 3; // 5 bps total
/// ```
pub const MIN_PROFITABLE_SPREAD_BPS: u32 = ROUND_TRIP_COST_BPS;

/// Calculate fee amount in fixed-point u64
///
/// Given a price and fee in basis points, calculates the fee amount.
///
/// # Arguments
/// * `price` - Price in u64 fixed-point (9 decimals)
/// * `fee_bps` - Fee in basis points (1 bp = 0.01%)
///
/// # Returns
/// Fee amount in u64 fixed-point (9 decimals)
///
/// # Example
/// ```ignore
/// let price = 50_000_000_000_000; // $50,000
/// let fee = calculate_fee(price, 2); // 2 bps
/// // fee = 50_000 * 0.0002 = $10
/// assert_eq!(fee, 10_000_000_000);
/// ```
#[inline(always)]
pub fn calculate_fee(price: u64, fee_bps: u32) -> u64 {
    // price * fee_bps / 10_000
    // Use u128 to prevent overflow
    let fee = (price as u128 * fee_bps as u128) / 10_000;
    fee as u64
}

/// Calculate required spread for target profit
///
/// Given a desired profit margin in basis points, calculates the total
/// spread needed to achieve that profit after fees.
///
/// # Formula
/// ```text
/// required_spread = round_trip_fees + target_profit
/// ```
///
/// # Arguments
/// * `target_profit_bps` - Desired profit in basis points
///
/// # Returns
/// Required spread in basis points
///
/// # Example
/// ```
/// use bog_strategies::fees::calculate_required_spread;
///
/// // Want 3 bps profit after fees
/// let spread = calculate_required_spread(3);
/// // For Lighter: 2 (fees) + 3 (profit) = 5 bps
/// assert_eq!(spread, 5);
/// ```
#[inline(always)]
pub const fn calculate_required_spread(target_profit_bps: u32) -> u32 {
    ROUND_TRIP_COST_BPS + target_profit_bps
}

/// Calculate bid/ask prices with fee-aware spread
///
/// Given a mid price and target spread, calculates bid/ask prices that
/// ensure profitability after fees.
///
/// # Arguments
/// * `mid_price` - Mid market price in u64 fixed-point
/// * `target_spread_bps` - Desired total spread in basis points
///
/// # Returns
/// `(bid_price, ask_price)` tuple in u64 fixed-point
///
/// # Panic
/// Panics if `target_spread_bps < MIN_PROFITABLE_SPREAD_BPS`
///
/// # Example
/// ```ignore
/// let mid = 50_000_000_000_000; // $50,000
/// let (bid, ask) = calculate_quotes(mid, 5); // 5 bps spread
///
/// // bid = mid * (1 - 0.00025) = $49,987.50
/// // ask = mid * (1 + 0.00025) = $50,012.50
/// ```
#[inline(always)]
pub fn calculate_quotes(mid_price: u64, target_spread_bps: u32) -> (u64, u64) {
    debug_assert!(
        target_spread_bps >= MIN_PROFITABLE_SPREAD_BPS,
        "Spread too narrow: {} < {} (min profitable)",
        target_spread_bps,
        MIN_PROFITABLE_SPREAD_BPS
    );

    // Half-spread in bps
    let half_spread_bps = target_spread_bps / 2;

    // Calculate bid: mid * (1 - half_spread / 10_000)
    let bid_adjustment = (mid_price as u128 * half_spread_bps as u128) / 10_000;
    let bid_price = mid_price.saturating_sub(bid_adjustment as u64);

    // Calculate ask: mid * (1 + half_spread / 10_000)
    let ask_adjustment = (mid_price as u128 * half_spread_bps as u128) / 10_000;
    let ask_price = mid_price.saturating_add(ask_adjustment as u64);

    (bid_price, ask_price)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bog_core::config::DEFAULT_FEE_BPS;

    #[test]
    fn test_fee_constants() {
        // Lighter default configuration
        #[cfg(not(any(
            feature = "maker-fee-1bps",
            feature = "maker-fee-2bps",
            feature = "maker-fee-5bps",
            feature = "maker-fee-10bps"
        )))]
        {
            assert_eq!(MAKER_FEE_BPS, 0);
        }

        #[cfg(not(any(
            feature = "taker-fee-5bps",
            feature = "taker-fee-10bps",
            feature = "taker-fee-20bps",
            feature = "taker-fee-30bps"
        )))]
        {
            assert_eq!(TAKER_FEE_BPS, 2);
        }

        // Round-trip cost
        assert_eq!(ROUND_TRIP_COST_BPS, MAKER_FEE_BPS + TAKER_FEE_BPS);
        assert_eq!(MIN_PROFITABLE_SPREAD_BPS, ROUND_TRIP_COST_BPS);

        // Cross-crate consistency: under default features, the taker fee
        // should match the core DEFAULT_FEE_BPS used by the engine.
        #[cfg(not(any(
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
            assert_eq!(TAKER_FEE_BPS, DEFAULT_FEE_BPS);
        }
    }

    #[test]
    fn test_calculate_fee() {
        let price = 50_000_000_000_000; // $50,000

        // 2 bps fee
        let fee = calculate_fee(price, 2);
        // Expected: 50_000 * 0.0002 = $10 = 10_000_000_000
        assert_eq!(fee, 10_000_000_000);

        // 10 bps fee
        let fee = calculate_fee(price, 10);
        // Expected: 50_000 * 0.001 = $50 = 50_000_000_000
        assert_eq!(fee, 50_000_000_000);

        // 1 bp fee
        let fee = calculate_fee(price, 1);
        // Expected: 50_000 * 0.0001 = $5 = 5_000_000_000
        assert_eq!(fee, 5_000_000_000);
    }

    #[test]
    fn test_calculate_required_spread() {
        // Want 3 bps profit
        let spread = calculate_required_spread(3);
        // For Lighter default (2 bps fees): 2 + 3 = 5 bps
        assert_eq!(spread, ROUND_TRIP_COST_BPS + 3);

        // Want 10 bps profit
        let spread = calculate_required_spread(10);
        assert_eq!(spread, ROUND_TRIP_COST_BPS + 10);
    }

    #[test]
    fn test_calculate_quotes() {
        let mid = 50_000_000_000_000; // $50,000

        // 10 bps spread (5 bps each side)
        let (bid, ask) = calculate_quotes(mid, 10);

        // bid = 50_000 * (1 - 0.0005) = 49_975
        // ask = 50_000 * (1 + 0.0005) = 50_025
        assert_eq!(bid, 49_975_000_000_000);
        assert_eq!(ask, 50_025_000_000_000);

        // Verify spread
        let actual_spread = ask - bid;
        let actual_spread_bps = (actual_spread as u128 * 10_000) / mid as u128;
        assert_eq!(actual_spread_bps, 10);
    }

    #[test]
    fn test_quotes_profitability() {
        let mid = 50_000_000_000_000; // $50,000

        // Use minimum profitable spread
        let (bid, ask) = calculate_quotes(mid, MIN_PROFITABLE_SPREAD_BPS);

        // Calculate expected profit
        let spread = ask - bid;
        let spread_bps = (spread as u128 * 10_000) / mid as u128;

        // Profit after fees should be >= 0
        assert!(spread_bps >= ROUND_TRIP_COST_BPS as u128);
    }

    #[test]
    fn test_fee_calculations_precision() {
        // Test with very small price
        let small_price = 1_000_000; // $0.001
        let fee = calculate_fee(small_price, 10);
        // Should not underflow
        assert!(fee < small_price);

        // Test with very large price
        let large_price = 100_000_000_000_000_000; // $100,000,000
        let fee = calculate_fee(large_price, 10);
        // Should not overflow (using u128 internally)
        assert!(fee > 0);
    }
}
