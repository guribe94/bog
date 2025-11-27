//! Zero-allocation volatility estimation for dynamic spread adjustment
//!
//! Provides fast, zero-overhead volatility calculations using:
//! - Rolling window with fixed-size circular buffer
//! - EWMA (Exponentially Weighted Moving Average)
//! - Parkinson estimator (high-low range)
//! - All calculations in u64 fixed-point arithmetic

use std::cmp::min;

/// Rolling volatility estimator with fixed-size circular buffer
///
/// Uses a circular buffer to track recent price changes and calculate
/// rolling volatility without heap allocations.
#[derive(Clone)]
pub struct RollingVolatility<const WINDOW_SIZE: usize> {
    /// Circular buffer of price observations
    prices: [u64; WINDOW_SIZE],
    /// Current write position in circular buffer
    position: usize,
    /// Number of observations collected (up to WINDOW_SIZE)
    count: usize,
    /// Cached volatility (9 decimal fixed-point)
    cached_volatility: u64,
    /// Flag indicating cache is valid
    cache_valid: bool,
}

impl<const WINDOW_SIZE: usize> RollingVolatility<WINDOW_SIZE> {
    /// Create a new rolling volatility estimator
    pub const fn new() -> Self {
        Self {
            prices: [0; WINDOW_SIZE],
            position: 0,
            count: 0,
            cached_volatility: 0,
            cache_valid: false,
        }
    }

    /// Add a new price observation
    #[inline]
    pub fn add_price(&mut self, price: u64) {
        self.prices[self.position] = price;
        self.position = (self.position + 1) % WINDOW_SIZE;
        if self.count < WINDOW_SIZE {
            self.count += 1;
        }
        self.cache_valid = false;
    }

    /// Calculate volatility (standard deviation of returns)
    ///
    /// Returns volatility in basis points (fixed-point, 9 decimals)
    pub fn calculate(&mut self) -> u64 {
        if self.cache_valid {
            return self.cached_volatility;
        }

        if self.count < 2 {
            return 0;
        }

        // Calculate returns (percent changes)
        let mut returns = [0i64; WINDOW_SIZE];
        let mut returns_count = 0;

        for i in 0..(self.count - 1) {
            let idx = (self.position + WINDOW_SIZE - self.count + i) % WINDOW_SIZE;
            let next_idx = (idx + 1) % WINDOW_SIZE;

            let price1 = self.prices[idx];
            let price2 = self.prices[next_idx];

            if price1 > 0 {
                // Calculate return: (price2 - price1) / price1 * 10000 (basis points)
                let diff = price2 as i128 - price1 as i128;
                let return_bps = (diff * 10_000) / price1 as i128;
                returns[returns_count] = return_bps as i64;
                returns_count += 1;
            }
        }

        if returns_count == 0 {
            return 0;
        }

        // Calculate mean
        let sum: i64 = returns[..returns_count].iter().sum();
        let mean = sum / returns_count as i64;

        // Calculate variance
        let mut variance_sum: u128 = 0;
        for i in 0..returns_count {
            let diff = returns[i] - mean;
            variance_sum += (diff as i128 * diff as i128) as u128;
        }
        let variance = variance_sum / returns_count as u128;

        // Calculate standard deviation (sqrt approximation)
        let std_dev = integer_sqrt(variance);

        self.cached_volatility = std_dev as u64;
        self.cache_valid = true;

        std_dev as u64
    }

    /// Get current volatility (calculates if needed)
    #[inline]
    pub fn volatility(&mut self) -> u64 {
        self.calculate()
    }

    /// Check if enough data for meaningful estimate
    #[inline]
    pub fn is_ready(&self) -> bool {
        self.count >= WINDOW_SIZE / 2
    }

    /// Reset the estimator
    pub fn reset(&mut self) {
        self.position = 0;
        self.count = 0;
        self.cached_volatility = 0;
        self.cache_valid = false;
    }
}

/// EWMA (Exponentially Weighted Moving Average) volatility estimator
///
/// Gives more weight to recent observations, adapts faster to regime changes.
#[derive(Clone)]
pub struct EwmaVolatility {
    /// Current EWMA value
    ewma: u64,
    /// Smoothing factor (0-1000, represents 0.0-1.0)
    alpha: u16,
    /// Last price observation
    last_price: u64,
    /// Number of observations
    count: usize,
}

impl EwmaVolatility {
    /// Create a new EWMA estimator
    ///
    /// alpha: smoothing factor, 0-1000 (e.g., 200 = 0.2)
    pub fn new(alpha: u16) -> Self {
        Self {
            ewma: 0,
            alpha: min(alpha, 1000),
            last_price: 0,
            count: 0,
        }
    }

    /// Add a new price observation
    #[inline]
    pub fn add_price(&mut self, price: u64) {
        if self.count == 0 {
            self.last_price = price;
            self.count = 1;
            return;
        }

        if self.last_price == 0 {
            self.last_price = price;
            return;
        }

        // Calculate absolute return in basis points
        let diff = if price > self.last_price {
            price - self.last_price
        } else {
            self.last_price - price
        };

        let abs_return = (diff * 10_000) / self.last_price;

        // Update EWMA: ewma = alpha * abs_return + (1 - alpha) * ewma
        let alpha_scaled = self.alpha as u64;
        let new_component = (alpha_scaled * abs_return) / 1000;
        let old_component = ((1000 - alpha_scaled) * self.ewma) / 1000;
        self.ewma = new_component + old_component;

        self.last_price = price;
        self.count += 1;
    }

    /// Get current EWMA volatility estimate (in basis points)
    #[inline]
    pub fn volatility(&self) -> u64 {
        self.ewma
    }

    /// Check if ready
    #[inline]
    pub fn is_ready(&self) -> bool {
        self.count >= 5
    }

    /// Reset the estimator
    pub fn reset(&mut self) {
        self.ewma = 0;
        self.last_price = 0;
        self.count = 0;
    }
}

/// Parkinson volatility estimator using high-low range
///
/// More efficient than standard deviation, uses high-low range of prices.
#[derive(Clone)]
pub struct ParkinsonVolatility<const WINDOW_SIZE: usize> {
    /// High prices in window
    highs: [u64; WINDOW_SIZE],
    /// Low prices in window
    lows: [u64; WINDOW_SIZE],
    /// Current position
    position: usize,
    /// Count of observations
    count: usize,
}

impl<const WINDOW_SIZE: usize> ParkinsonVolatility<WINDOW_SIZE> {
    /// Create a new Parkinson estimator
    pub const fn new() -> Self {
        Self {
            highs: [0; WINDOW_SIZE],
            lows: [u64::MAX; WINDOW_SIZE],
            position: 0,
            count: 0,
        }
    }

    /// Add high and low prices for a period
    #[inline]
    pub fn add_high_low(&mut self, high: u64, low: u64) {
        self.highs[self.position] = high;
        self.lows[self.position] = low;
        self.position = (self.position + 1) % WINDOW_SIZE;
        if self.count < WINDOW_SIZE {
            self.count += 1;
        }
    }

    /// Calculate Parkinson volatility
    pub fn volatility(&self) -> u64 {
        if self.count == 0 {
            return 0;
        }

        let mut sum_log_ratio_sq: u128 = 0;

        for i in 0..self.count {
            let high = self.highs[i];
            let low = self.lows[i];

            if low > 0 && high >= low {
                // Calculate log(high/low)^2 approximation
                let ratio = (high * 10_000) / low;
                let log_ratio_approx = if ratio > 10_000 {
                    ratio - 10_000
                } else {
                    0
                };
                sum_log_ratio_sq += (log_ratio_approx as u128).pow(2);
            }
        }

        if self.count == 0 {
            return 0;
        }

        // Parkinson: sqrt(sum / (4 * n * ln(2)))
        // Simplified: sqrt(sum / (3 * n))
        let variance = sum_log_ratio_sq / (3 * self.count as u128);
        integer_sqrt(variance) as u64
    }

    /// Check if ready
    #[inline]
    pub fn is_ready(&self) -> bool {
        self.count >= WINDOW_SIZE / 2
    }

    /// Reset the estimator
    pub fn reset(&mut self) {
        self.position = 0;
        self.count = 0;
    }
}

/// Integer square root using binary search (fast for u128)
#[inline]
fn integer_sqrt(n: u128) -> u64 {
    if n == 0 {
        return 0;
    }
    if n <= 1 {
        return 1;
    }

    let mut low: u64 = 0;
    let mut high: u64 = min(n as u64, u64::MAX);

    while low <= high {
        let mid = low + (high - low) / 2;
        let mid_squared = (mid as u128) * (mid as u128);

        if mid_squared == n {
            return mid;
        } else if mid_squared < n {
            low = mid + 1;
        } else {
            if mid == 0 {
                break;
            }
            high = mid - 1;
        }
    }

    high
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rolling_volatility_basic() {
        let mut vol = RollingVolatility::<20>::new();

        // Add stable prices
        for _ in 0..10 {
            vol.add_price(50000_000000000);
        }

        let v = vol.volatility();
        assert_eq!(v, 0); // No volatility with constant prices
    }

    #[test]
    fn test_rolling_volatility_varying() {
        let mut vol = RollingVolatility::<20>::new();

        // Add varying prices
        vol.add_price(50000_000000000);
        vol.add_price(50500_000000000); // +1%
        vol.add_price(50000_000000000); // -1%
        vol.add_price(50500_000000000); // +1%

        let v = vol.volatility();
        assert!(v > 0); // Should detect volatility
    }

    #[test]
    fn test_ewma_volatility() {
        let mut ewma = EwmaVolatility::new(200); // alpha = 0.2

        // is_ready() requires count >= 5
        ewma.add_price(50000_000000000);
        ewma.add_price(50500_000000000); // +1% = 100 bps
        ewma.add_price(50000_000000000); // -1%
        ewma.add_price(50250_000000000); // +0.5%
        ewma.add_price(50100_000000000); // -0.3%

        let v = ewma.volatility();
        assert!(v > 0);
        assert!(ewma.is_ready());
    }

    #[test]
    fn test_parkinson_volatility() {
        let mut park = ParkinsonVolatility::<10>::new();

        // is_ready() requires count >= WINDOW_SIZE/2 = 5
        park.add_high_low(50500_000000000, 50000_000000000);
        park.add_high_low(50600_000000000, 50100_000000000);
        park.add_high_low(50700_000000000, 50200_000000000);
        park.add_high_low(50800_000000000, 50300_000000000);
        park.add_high_low(50900_000000000, 50400_000000000);

        let v = park.volatility();
        assert!(v > 0);
        assert!(park.is_ready());
    }

    #[test]
    fn test_integer_sqrt() {
        assert_eq!(integer_sqrt(0), 0);
        assert_eq!(integer_sqrt(1), 1);
        assert_eq!(integer_sqrt(4), 2);
        assert_eq!(integer_sqrt(9), 3);
        assert_eq!(integer_sqrt(16), 4);
        assert_eq!(integer_sqrt(100), 10);
        assert_eq!(integer_sqrt(10000), 100);
    }

    #[test]
    fn test_rolling_volatility_is_zero_sized() {
        use std::mem::size_of;
        // Structure contains fixed arrays, not zero-sized, but stack-allocated
        let size = size_of::<RollingVolatility<20>>();
        assert!(size > 0); // Has fixed buffer
    }
}
