use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;

// Re-export Huginn types
pub use huginn::shm::{ConsumerStats, MarketSnapshot};

// Re-export extension trait
pub use conversions::*;

/// Type alias for encoded market IDs
/// Encoded format: (dex_type * 1_000_000) + raw_market_id
/// Example: Lighter (dex_type=1) market 1 = 1,000,001
pub type EncodedMarketId = u64;

/// Type alias for raw market IDs
/// Raw format: DEX-specific market identifier (e.g., 1, 2, 3...)
pub type RawMarketId = u64;

/// Encode a market ID by combining DEX type and raw market ID
///
/// # Arguments
/// - `dex_type`: DEX identifier (1 = Lighter, 2 = Binance, etc.)
/// - `market_id`: Raw DEX-specific market ID (e.g., 1, 2, 3...)
///
/// # Returns
/// Encoded market ID: `(dex_type * 1_000_000) + market_id`
///
/// # Example
/// ```rust
/// # use bog_core::data::types::encode_market_id;
/// let encoded = encode_market_id(1, 1); // Lighter market 1
/// assert_eq!(encoded, 1_000_001);
/// ```
#[inline]
pub fn encode_market_id(dex_type: u8, market_id: u64) -> EncodedMarketId {
    ((dex_type as u64) * 1_000_000) + market_id
}

/// Encode a market ID with validation
///
/// Returns an error if the market_id >= 1_000_000 (would exceed encoding space)
///
/// # Arguments
/// - `dex_type`: DEX identifier
/// - `market_id`: Raw market ID
///
/// # Returns
/// - `Ok(encoded)`: Successfully encoded market ID
/// - `Err`: Market ID >= 1_000_000 (exceeds encoding capacity)
#[inline]
pub fn encode_market_id_checked(dex_type: u8, market_id: u64) -> Result<EncodedMarketId, String> {
    if market_id >= 1_000_000 {
        Err(format!(
            "Market ID {} exceeds maximum (999,999). DEX encoding requires space for 1M markets per DEX.",
            market_id
        ))
    } else {
        Ok(encode_market_id(dex_type, market_id))
    }
}

/// Decode a market ID into DEX type and raw market ID
///
/// # Arguments
/// - `encoded_id`: Encoded market ID (from `encode_market_id()`)
///
/// # Returns
/// Tuple of `(dex_type, raw_market_id)`
///
/// # Example
/// ```rust
/// # use bog_core::data::types::decode_market_id;
/// let (dex, market) = decode_market_id(1_000_001);
/// assert_eq!(dex, 1);    // Lighter
/// assert_eq!(market, 1); // market 1
/// ```
#[inline]
pub fn decode_market_id(encoded_id: EncodedMarketId) -> (u8, RawMarketId) {
    let dex_type = (encoded_id / 1_000_000) as u8;
    let market_id = encoded_id % 1_000_000;
    (dex_type, market_id)
}

/// Helper functions for price conversions
pub mod conversions {
    use super::*;

    /// Convert u64 fixed-point price to f64
    /// Huginn uses 9 decimal places
    #[inline]
    pub fn u64_to_f64(price: u64) -> f64 {
        price as f64 / 1_000_000_000.0
    }

    /// Convert u64 fixed-point price to Decimal
    #[inline]
    pub fn u64_to_decimal(price: u64) -> Decimal {
        Decimal::from(price) / Decimal::from(1_000_000_000)
    }

    /// Convert f64 to u64 fixed-point
    #[inline]
    pub fn f64_to_u64(price: f64) -> u64 {
        (price * 1_000_000_000.0) as u64
    }

    /// Convert Decimal to u64 fixed-point
    #[inline]
    pub fn decimal_to_u64(price: Decimal) -> u64 {
        (price * Decimal::from(1_000_000_000)).to_u64().unwrap_or(0)
    }
}

/// Extension trait for MarketSnapshot with convenient methods
pub trait MarketSnapshotExt {
    /// Get best bid price as f64
    fn best_bid_f64(&self) -> f64;

    /// Get best ask price as f64
    fn best_ask_f64(&self) -> f64;

    /// Get best bid price as Decimal
    fn best_bid_decimal(&self) -> Decimal;

    /// Get best ask price as Decimal
    fn best_ask_decimal(&self) -> Decimal;

    /// Calculate mid price as f64
    fn mid_price_f64(&self) -> f64;

    /// Calculate mid price as Decimal
    fn mid_price_decimal(&self) -> Decimal;

    /// Calculate spread in basis points
    fn spread_bps(&self) -> f64;

    /// Calculate total bid size (best + next levels)
    fn total_bid_size(&self) -> u64;

    /// Calculate total ask size (best + next levels)
    fn total_ask_size(&self) -> u64;

    /// Get decoded market ID (dex_type, original_market_id)
    fn decoded_market_id(&self) -> (u8, u64);

    /// Calculate end-to-end latency in microseconds
    fn latency_us(&self) -> u64;

    /// Calculate Huginn processing latency in nanoseconds
    fn huginn_latency_ns(&self) -> u64;

    /// Get the best bid price
    ///
    /// Returns the pre-computed best bid price from Huginn. Huginn guarantees
    /// that best_bid_price contains the highest bid price (correct ordering).
    ///
    /// Note: Previously this function scanned bid_prices[] to find the max,
    /// but Huginn now handles sorting before publishing to shared memory.
    fn actual_best_bid(&self) -> u64;

    /// Get the best ask price
    ///
    /// Returns the pre-computed best ask price from Huginn. Huginn guarantees
    /// that best_ask_price contains the lowest ask price (correct ordering).
    ///
    /// Note: Previously this function scanned ask_prices[] to find the min,
    /// but Huginn now handles sorting before publishing to shared memory.
    fn actual_best_ask(&self) -> u64;

    /// Calculate spread in basis points
    ///
    /// Uses best_bid_price and best_ask_price which are guaranteed to be
    /// correctly ordered by Huginn. Returns spread as integer basis points.
    fn corrected_spread_bps(&self) -> u64;
}

impl MarketSnapshotExt for MarketSnapshot {
    #[inline]
    fn best_bid_f64(&self) -> f64 {
        conversions::u64_to_f64(self.best_bid_price)
    }

    #[inline]
    fn best_ask_f64(&self) -> f64 {
        conversions::u64_to_f64(self.best_ask_price)
    }

    #[inline]
    fn best_bid_decimal(&self) -> Decimal {
        conversions::u64_to_decimal(self.best_bid_price)
    }

    #[inline]
    fn best_ask_decimal(&self) -> Decimal {
        conversions::u64_to_decimal(self.best_ask_price)
    }

    #[inline]
    fn mid_price_f64(&self) -> f64 {
        (self.best_bid_f64() + self.best_ask_f64()) / 2.0
    }

    #[inline]
    fn mid_price_decimal(&self) -> Decimal {
        (self.best_bid_decimal() + self.best_ask_decimal()) / Decimal::from(2)
    }

    #[inline]
    fn spread_bps(&self) -> f64 {
        let bid = self.best_bid_f64();
        let ask = self.best_ask_f64();
        if bid > 0.0 {
            ((ask - bid) / bid) * 10_000.0
        } else {
            0.0
        }
    }

    #[inline]
    fn total_bid_size(&self) -> u64 {
        self.best_bid_size
        // Note: Add bid_sizes[] when we need depth
    }

    #[inline]
    fn total_ask_size(&self) -> u64 {
        self.best_ask_size
        // Note: Add ask_sizes[] when we need depth
    }

    #[inline]
    fn decoded_market_id(&self) -> (u8, u64) {
        let dex_number = (self.market_id / 1_000_000) as u8;
        let original_id = self.market_id % 1_000_000;
        (dex_number, original_id)
    }

    #[inline]
    fn latency_us(&self) -> u64 {
        if self.exchange_timestamp_ns > 0 && self.local_publish_ns >= self.exchange_timestamp_ns {
            (self.local_publish_ns - self.exchange_timestamp_ns) / 1_000
        } else {
            0
        }
    }

    #[inline]
    fn huginn_latency_ns(&self) -> u64 {
        if self.local_publish_ns >= self.local_recv_ns {
            self.local_publish_ns - self.local_recv_ns
        } else {
            0
        }
    }

    #[inline]
    fn actual_best_bid(&self) -> u64 {
        // Huginn now computes the correct best bid (max price) before publishing
        // Just return the pre-computed value
        self.best_bid_price
    }

    #[inline]
    fn actual_best_ask(&self) -> u64 {
        // Huginn now computes the correct best ask (min non-zero price) before publishing
        // Just return the pre-computed value
        self.best_ask_price
    }

    #[inline]
    fn corrected_spread_bps(&self) -> u64 {
        let bid = self.actual_best_bid();
        let ask = self.actual_best_ask();
        if bid > 0 && ask > bid {
            ((ask - bid) * 10_000) / bid
        } else {
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conversions() {
        let price_f64 = 50000.123456789;
        let price_u64 = conversions::f64_to_u64(price_f64);
        let converted_back = conversions::u64_to_f64(price_u64);

        // Should be very close (within floating point precision)
        assert!((price_f64 - converted_back).abs() < 0.000001);
    }

    #[test]
    fn test_spread_bps() {
        let snapshot = MarketSnapshot {
            generation_start: 1,
            generation_end: 1,
            market_id: 1,
            sequence: 1,
            exchange_timestamp_ns: 0,
            local_recv_ns: 0,
            local_publish_ns: 0,
            best_bid_price: conversions::f64_to_u64(50000.0),
            best_bid_size: 1000000000,
            best_ask_price: conversions::f64_to_u64(50005.0), // 5 dollar spread
            best_ask_size: 1000000000,
            dex_type: 1,
            ..Default::default()
        };

        let spread = MarketSnapshotExt::spread_bps(&snapshot);
        // 5 / 50000 * 10000 = 1 bp
        assert!((spread - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_actual_best_bid_correctly_sorted() {
        // When data is correctly sorted, actual_best_bid should return best_bid_price
        let snapshot = MarketSnapshot {
            market_id: 1,
            sequence: 1,
            best_bid_price: conversions::f64_to_u64(100.0), // Highest (best)
            best_ask_price: conversions::f64_to_u64(101.0),
            bid_prices: {
                let mut arr = [0u64; 10];
                arr[0] = conversions::f64_to_u64(99.0);  // Second best
                arr[1] = conversions::f64_to_u64(98.0);  // Third best
                arr
            },
            ..Default::default()
        };

        // Should return the best_bid_price since it's the highest
        assert_eq!(snapshot.actual_best_bid(), conversions::f64_to_u64(100.0));
    }

    // Note: test_actual_best_bid_incorrectly_sorted was removed because Huginn
    // now guarantees correct ordering. The actual_best_bid() function trusts
    // Huginn's pre-computed best_bid_price value.

    #[test]
    fn test_actual_best_ask_correctly_sorted() {
        // When data is correctly sorted, actual_best_ask should return best_ask_price
        let snapshot = MarketSnapshot {
            market_id: 1,
            sequence: 1,
            best_bid_price: conversions::f64_to_u64(100.0),
            best_ask_price: conversions::f64_to_u64(101.0), // Lowest (best)
            ask_prices: {
                let mut arr = [0u64; 10];
                arr[0] = conversions::f64_to_u64(102.0); // Second best
                arr[1] = conversions::f64_to_u64(103.0); // Third best
                arr
            },
            ..Default::default()
        };

        // Should return the best_ask_price since it's the lowest
        assert_eq!(snapshot.actual_best_ask(), conversions::f64_to_u64(101.0));
    }

    // Note: test_actual_best_ask_incorrectly_sorted was removed because Huginn
    // now guarantees correct ordering. The actual_best_ask() function trusts
    // Huginn's pre-computed best_ask_price value.

    #[test]
    fn test_corrected_spread_bps() {
        // Test spread calculation in basis points (integer)
        // Huginn guarantees correct ordering, so bid/ask in snapshot are correct
        let snapshot = MarketSnapshot {
            market_id: 1,
            sequence: 1,
            best_bid_price: conversions::f64_to_u64(100.0),
            best_ask_price: conversions::f64_to_u64(100.1), // 0.1% spread = 10 bps
            ..Default::default()
        };

        // Spread: (100.1 - 100.0) / 100.0 * 10000 = 10 bps
        let spread = snapshot.corrected_spread_bps();
        assert_eq!(spread, 10, "Expected 10 bps spread, got {}", spread);

        // Compare with floating point spread_bps
        let spread_f64: f64 = MarketSnapshotExt::spread_bps(&snapshot);
        assert!((spread_f64 - 10.0).abs() < 0.01, "Expected ~10.0 bps, got {}", spread_f64);
    }

    // Note: test_corrected_spread_bps_with_misordered_data was removed because
    // Huginn now guarantees correct ordering. The "correction" in corrected_spread_bps()
    // is no longer needed - it's equivalent to spread_bps() but returns integer bps.

    #[test]
    fn test_actual_best_ask_ignores_zeros() {
        // Zeros in ask_prices should be ignored (empty levels)
        let snapshot = MarketSnapshot {
            market_id: 1,
            sequence: 1,
            best_bid_price: conversions::f64_to_u64(100.0),
            best_ask_price: conversions::f64_to_u64(101.0),
            ask_prices: [0u64; 10], // All zeros should be ignored
            ..Default::default()
        };

        // Should return best_ask_price since all ask_prices are zero
        assert_eq!(snapshot.actual_best_ask(), conversions::f64_to_u64(101.0));
    }
}
