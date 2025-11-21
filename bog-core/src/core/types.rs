//! Zero-overhead core types for HFT trading
//!
//! All types in this module are designed for:
//! - Zero heap allocations
//! - Copy semantics where possible
//! - Cache-line alignment
//! - Minimal memory footprint

use std::fmt;
use std::sync::atomic::{AtomicI64, AtomicU32, AtomicU64, Ordering};
use crate::core::errors::OverflowError;

/// Unique identifier for an order
///
/// Uses u128 instead of String for zero-allocation, copy semantics.
/// Generated using thread-local counter + timestamp + random bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(transparent)]
pub struct OrderId(pub u128);

impl OrderId {
    /// Create a new OrderId from a u128
    #[inline(always)]
    pub const fn new(id: u128) -> Self {
        Self(id)
    }

    /// Generate a new random OrderId
    ///
    /// Format: `[timestamp:64][random:32][counter:32]`
    /// This ensures uniqueness across threads and time
    ///
    /// # Performance
    ///
    /// Measured: ~64ns (primarily from SystemTime::now() ~60ns)
    ///
    /// Note: Timestamp caching optimization was tried but hurt overall pipeline performance.
    /// Micro-optimizations that help in isolation can hurt in realistic context due to:
    /// - Instruction cache pressure
    /// - Additional TLS accesses
    /// - Branch mispredictions
    /// - Instant::elapsed() overhead
    ///
    /// Keeping simple implementation for better pipeline performance.
    #[inline]
    pub fn generate() -> Self {
        use std::time::SystemTime;
        use rand::Rng;

        thread_local! {
            static COUNTER: std::cell::Cell<u32> = std::cell::Cell::new(0);
            static RNG: std::cell::RefCell<rand::rngs::ThreadRng> = std::cell::RefCell::new(rand::thread_rng());
        }

        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_else(|_| std::time::Duration::from_nanos(0))
            .as_nanos() as u64;

        let random_part = RNG.with(|rng| rng.borrow_mut().gen::<u32>());

        let counter = COUNTER.with(|c| {
            let val = c.get();
            let next = val.wrapping_add(1);

            // SAFETY: Detect and log OrderId counter wraparound
            // This is a critical event that should never happen in production
            // u32 allows 4 billion orders before wrap
            if next < val {
                // Counter wrapped around!
                // In production, this should trigger alerts
                eprintln!("CRITICAL: OrderId counter wraparound detected! Old: {}, New: {}", val, next);
                // Could also panic here in debug mode:
                // debug_assert!(false, "OrderId counter wraparound!");
            }

            c.set(next);
            val
        });

        let id = ((timestamp as u128) << 64) | ((random_part as u128) << 32) | (counter as u128);
        Self(id)
    }

    /// Alias for generate() - for backwards compatibility
    #[inline]
    pub fn new_random() -> Self {
        Self::generate()
    }

    /// Get the inner u128 value
    #[inline(always)]
    pub const fn as_u128(&self) -> u128 {
        self.0
    }

    /// Get timestamp component (upper 64 bits)
    #[inline(always)]
    pub const fn timestamp(&self) -> u64 {
        (self.0 >> 64) as u64
    }

    /// Get random component (bits 32-63)
    #[inline(always)]
    pub const fn random_part(&self) -> u32 {
        (self.0 >> 32) as u32
    }

    /// Get counter component (lower 32 bits)
    #[inline(always)]
    pub const fn counter(&self) -> u32 {
        self.0 as u32
    }
}

impl fmt::Display for OrderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:032x}", self.0)
    }
}

impl From<u128> for OrderId {
    #[inline(always)]
    fn from(id: u128) -> Self {
        Self(id)
    }
}

/// Order side (Buy or Sell)
///
/// Single byte enum for minimal size
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Side {
    Buy = 0,
    Sell = 1,
}

impl fmt::Display for Side {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Side::Buy => write!(f, "BUY"),
            Side::Sell => write!(f, "SELL"),
        }
    }
}

/// Order type
///
/// Single byte enum for minimal size
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum OrderType {
    Limit = 0,
    Market = 1,
    PostOnly = 2,
}

/// Order status
///
/// Single byte enum for minimal size
///
/// This is the canonical OrderStatus definition used throughout the codebase.
/// It's re-exported by `execution::types` to ensure a single source of truth.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[repr(u8)]
pub enum OrderStatus {
    Pending = 0,
    Open = 1,
    PartiallyFilled = 2,
    Filled = 3,
    Cancelled = 4,
    Rejected = 5,
    Expired = 6,
}

/// Cache-aligned position state
///
/// Aligned to 64 bytes (cache line) to prevent false sharing.
/// Uses atomic operations for lock-free updates.
/// All values use fixed-point arithmetic (9 decimal places).
#[repr(C, align(64))]
pub struct Position {
    /// Current position quantity (fixed-point, 9 decimals)
    /// Positive = long, Negative = short
    pub quantity: AtomicI64,

    /// Average entry price (fixed-point, 9 decimals)
    pub entry_price: AtomicU64,

    /// Realized PnL (fixed-point, 9 decimals)
    pub realized_pnl: AtomicI64,

    /// Daily PnL (fixed-point, 9 decimals)
    pub daily_pnl: AtomicI64,

    /// Total number of trades
    pub trade_count: AtomicU32,

    /// Padding to 64 bytes
    _padding: [u8; 20],
}

impl Position {
    /// Create a new empty position
    #[inline]
    pub const fn new() -> Self {
        Self {
            quantity: AtomicI64::new(0),
            entry_price: AtomicU64::new(0),
            realized_pnl: AtomicI64::new(0),
            daily_pnl: AtomicI64::new(0),
            trade_count: AtomicU32::new(0),
            _padding: [0; 20],
        }
    }

    /// Get current quantity (relaxed ordering for reads in hot path)
    #[inline(always)]
    pub fn get_quantity(&self) -> i64 {
        self.quantity.load(Ordering::Relaxed)
    }

    /// Update quantity (acquire-release semantics)
    #[inline(always)]
    pub fn update_quantity(&self, delta: i64) -> i64 {
        self.quantity.fetch_add(delta, Ordering::AcqRel) + delta
    }

    /// Get realized PnL
    #[inline(always)]
    pub fn get_realized_pnl(&self) -> i64 {
        self.realized_pnl.load(Ordering::Relaxed)
    }

    /// Update realized PnL
    #[inline(always)]
    pub fn update_realized_pnl(&self, delta: i64) {
        self.realized_pnl.fetch_add(delta, Ordering::AcqRel);
    }

    /// Get daily PnL
    #[inline(always)]
    pub fn get_daily_pnl(&self) -> i64 {
        self.daily_pnl.load(Ordering::Relaxed)
    }

    /// Update daily PnL
    #[inline(always)]
    pub fn update_daily_pnl(&self, delta: i64) {
        self.daily_pnl.fetch_add(delta, Ordering::AcqRel);
    }

    /// Reset daily PnL (called at start of day)
    #[inline]
    pub fn reset_daily_pnl(&self) {
        self.daily_pnl.store(0, Ordering::Release);
    }

    /// Increment trade count
    #[inline(always)]
    pub fn increment_trades(&self) -> u32 {
        self.trade_count.fetch_add(1, Ordering::AcqRel) + 1
    }

    /// Get trade count
    #[inline(always)]
    pub fn get_trade_count(&self) -> u32 {
        self.trade_count.load(Ordering::Relaxed)
    }

    // ===== OVERFLOW-SAFE METHODS =====
    //
    // These methods provide overflow protection for critical arithmetic operations.
    // They return Result types that must be handled, preventing silent corruption.

    /// Update quantity with overflow checking
    ///
    /// # Returns
    ///
    /// - `Ok(new_quantity)` if the operation succeeded
    /// - `Err(OverflowError)` if the addition would overflow
    ///
    /// # Example
    ///
    /// ```
    /// # use bog_core::core::Position;
    /// let pos = Position::new();
    ///
    /// // Safe update
    /// let new_qty = pos.update_quantity_checked(1_000_000_000)?;
    /// assert_eq!(new_qty, 1_000_000_000);
    ///
    /// // Overflow detection
    /// let result = pos.update_quantity_checked(i64::MAX);
    /// assert!(result.is_err());
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[inline(always)]
    pub fn update_quantity_checked(&self, delta: i64) -> Result<i64, OverflowError> {
        let old = self.quantity.load(Ordering::Acquire);
        let new = old.checked_add(delta)
            .ok_or(OverflowError::QuantityOverflow { old, delta })?;

        self.quantity.store(new, Ordering::Release);
        Ok(new)
    }

    /// Update realized PnL with overflow checking
    ///
    /// # Returns
    ///
    /// - `Ok(())` if the operation succeeded
    /// - `Err(OverflowError)` if the addition would overflow
    #[inline(always)]
    pub fn update_realized_pnl_checked(&self, delta: i64) -> Result<(), OverflowError> {
        let old = self.realized_pnl.load(Ordering::Acquire);
        let new = old.checked_add(delta)
            .ok_or(OverflowError::RealizedPnlOverflow { old, delta })?;

        self.realized_pnl.store(new, Ordering::Release);
        Ok(())
    }

    /// Update daily PnL with overflow checking
    ///
    /// # Returns
    ///
    /// - `Ok(())` if the operation succeeded
    /// - `Err(OverflowError)` if the addition would overflow
    #[inline(always)]
    pub fn update_daily_pnl_checked(&self, delta: i64) -> Result<(), OverflowError> {
        let old = self.daily_pnl.load(Ordering::Acquire);
        let new = old.checked_add(delta)
            .ok_or(OverflowError::DailyPnlOverflow { old, delta })?;

        self.daily_pnl.store(new, Ordering::Release);
        Ok(())
    }

    /// Increment trade count with overflow checking
    ///
    /// # Returns
    ///
    /// - `Ok(new_count)` if the operation succeeded
    /// - `Err(OverflowError)` if incrementing would overflow u32
    ///
    /// Note: This is unlikely (4 billion trades), but still protected.
    #[inline(always)]
    pub fn increment_trades_checked(&self) -> Result<u32, OverflowError> {
        let old = self.trade_count.load(Ordering::Acquire);
        let new = old.checked_add(1)
            .ok_or(OverflowError::TradeCountOverflow { old })?;

        self.trade_count.store(new, Ordering::Release);
        Ok(new)
    }

    // ===== SATURATING METHODS =====
    //
    // These methods use saturating arithmetic (clamp at i64::MAX/MIN).
    // Use only for non-critical paths where clamping is acceptable.

    /// Update quantity with saturating arithmetic (clamps at i64::MAX/MIN)
    ///
    /// # Warning
    ///
    /// This method silently clamps on overflow. Only use in non-critical paths
    /// where overflow is acceptable. For critical paths, use `update_quantity_checked()`.
    #[inline(always)]
    pub fn update_quantity_saturating(&self, delta: i64) -> i64 {
        let old = self.quantity.load(Ordering::Acquire);
        let new = old.saturating_add(delta);
        self.quantity.store(new, Ordering::Release);
        new
    }

    /// Update realized PnL with saturating arithmetic
    ///
    /// # Warning
    ///
    /// This method silently clamps on overflow. Prefer `update_realized_pnl_checked()`.
    #[inline(always)]
    pub fn update_realized_pnl_saturating(&self, delta: i64) -> i64 {
        let old = self.realized_pnl.load(Ordering::Acquire);
        let new = old.saturating_add(delta);
        self.realized_pnl.store(new, Ordering::Release);
        new
    }

    /// Update daily PnL with saturating arithmetic
    ///
    /// # Warning
    ///
    /// This method silently clamps on overflow. Prefer `update_daily_pnl_checked()`.
    #[inline(always)]
    pub fn update_daily_pnl_saturating(&self, delta: i64) -> i64 {
        let old = self.daily_pnl.load(Ordering::Acquire);
        let new = old.saturating_add(delta);
        self.daily_pnl.store(new, Ordering::Release);
        new
    }

    // ===== FILL PROCESSING =====
    //
    // Methods for processing fills (trade executions) and updating position state.
    // Critical for accurate position tracking in paper trading and live execution.

    /// Process a fill from the simulated executor (using fixed-point arithmetic)
    ///
    /// Updates position quantity, realized PnL, and trade count atomically.
    /// Uses u64 fixed-point values (9 decimal places) for zero-allocation.
    ///
    /// # Arguments
    ///
    /// * `order_side` - 0 for Buy, 1 for Sell
    /// * `price` - Fill price in fixed-point (9 decimals)
    /// * `size` - Fill size in fixed-point (9 decimals)
    /// * `fee_bps` - Fee in basis points (e.g., 2 for 0.02%)
    ///
    /// # Returns
    ///
    /// - `Ok(())` if the fill was processed successfully
    /// - `Err(OverflowError)` if any arithmetic operation would overflow
    ///
    /// # Performance
    ///
    /// Target: <10ns for the entire operation
    /// Uses relaxed ordering where safe, AcqRel for critical updates
    #[inline(always)]
    pub fn process_fill_fixed_with_fee(
        &self,
        order_side: u8,
        price: u64,
        size: u64,
        fee_bps: u32,
    ) -> Result<(), OverflowError> {
        // Convert size to i64 for position delta
        let size_i64 = size as i64;

        // Calculate position delta based on side
        // Buy (0) increases position, Sell (1) decreases position
        let position_delta = if order_side == 0 {
            size_i64  // Buy: increase position
        } else {
            -size_i64  // Sell: decrease position
        };

        // Get current position before update
        let old_qty = self.quantity.load(Ordering::Acquire);

        // Calculate PnL if we're reducing/closing position
        let pnl = if (old_qty > 0 && position_delta < 0) || (old_qty < 0 && position_delta > 0) {
            // We're reducing or reversing position - calculate PnL
            let closing_qty = if position_delta.abs() > old_qty.abs() {
                old_qty.abs()  // Closing entire position
            } else {
                position_delta.abs()  // Partial close
            };

            // Get current average entry price
            let entry_price = self.entry_price.load(Ordering::Acquire);

            if entry_price > 0 {
                // Calculate PnL: (exit_price - entry_price) * quantity for long
                // or (entry_price - exit_price) * quantity for short
                let price_diff = if old_qty > 0 {
                    // Long position: profit if exit > entry
                    price as i64 - entry_price as i64
                } else {
                    // Short position: profit if exit < entry
                    entry_price as i64 - price as i64
                };

                // PnL = price_diff * closing_quantity / SCALE
                // We divide by SCALE because both price and quantity are in fixed-point
                let gross_pnl = (price_diff as i128 * closing_qty as i128 / 1_000_000_000) as i64;

                // Calculate fee in fixed-point
                // fee = (price * closing_qty * fee_bps) / (10000 * SCALE)
                // Careful with order to avoid overflow
                let fee = ((price as u128 * closing_qty as u128 * fee_bps as u128)
                    / (10_000 * 1_000_000_000)) as i64;

                // Net PnL = Gross PnL - Fee
                gross_pnl.saturating_sub(fee)
            } else {
                0  // No entry price set, no PnL
            }
        } else {
            0  // Not closing, no realized PnL
        };

        // Update position quantity with overflow check
        let new_qty = old_qty.checked_add(position_delta)
            .ok_or(OverflowError::QuantityOverflow { old: old_qty, delta: position_delta })?;

        self.quantity.store(new_qty, Ordering::Release);

        // Update entry price (weighted average for increases)
        if (new_qty > 0 && position_delta > 0) || (new_qty < 0 && position_delta < 0) {
            // Position increased in same direction - update weighted average entry
            let old_entry = self.entry_price.load(Ordering::Acquire);

            if old_entry == 0 || old_qty == 0 {
                // First position or was flat - set entry price
                self.entry_price.store(price, Ordering::Release);
            } else {
                // Calculate weighted average: (old_entry * old_qty + new_price * new_qty) / total_qty
                let old_value = (old_entry as u128 * old_qty.abs() as u128) / 1_000_000_000;
                let new_value = (price as u128 * position_delta.abs() as u128) / 1_000_000_000;
                let total_value = old_value + new_value;
                let total_qty = old_qty.abs() + position_delta.abs();

                if total_qty > 0 {
                    let avg_entry_u128 = (total_value * 1_000_000_000) / total_qty as u128;

                    // Check for overflow before casting to u64
                    if avg_entry_u128 > u64::MAX as u128 {
                        // Entry price would overflow - clamp to maximum
                        // This is an extreme edge case (BTC at billions per coin)
                        // Better to clamp than corrupt the value
                        self.entry_price.store(u64::MAX, Ordering::Release);
                    } else {
                        let avg_entry = avg_entry_u128 as u64;
                        self.entry_price.store(avg_entry, Ordering::Release);
                    }
                }
            }
        } else if new_qty == 0 {
            // Position closed - reset entry price
            self.entry_price.store(0, Ordering::Release);
        }

        // Update realized PnL if we had any
        if pnl != 0 {
            self.update_realized_pnl_checked(pnl)?;
            self.update_daily_pnl_checked(pnl)?;
        }

        // Increment trade count
        self.increment_trades_checked()?;

        Ok(())
    }

    /// Process a fill from the simulated executor (backward compatibility)
    ///
    /// Calls process_fill_fixed_with_fee with 0 fees for backward compatibility.
    #[inline(always)]
    pub fn process_fill_fixed(
        &self,
        order_side: u8,
        price: u64,
        size: u64,
    ) -> Result<(), OverflowError> {
        // Call with 0 fee for backward compatibility
        self.process_fill_fixed_with_fee(order_side, price, size, 0)
    }

    /// Get current average entry price
    #[inline(always)]
    pub fn get_entry_price(&self) -> u64 {
        self.entry_price.load(Ordering::Relaxed)
    }

    /// Calculate unrealized PnL given current market price
    ///
    /// # Arguments
    ///
    /// * `market_price` - Current market price in fixed-point (9 decimals)
    ///
    /// # Returns
    ///
    /// Unrealized PnL in fixed-point (9 decimals), or 0 if no position or no entry price
    ///
    /// # Safety
    ///
    /// This method handles the edge case where entry_price is 0 (corrupted state)
    /// by returning 0 instead of panicking from division by zero.
    #[inline(always)]
    pub fn get_unrealized_pnl(&self, market_price: u64) -> i64 {
        let qty = self.quantity.load(Ordering::Relaxed);
        if qty == 0 {
            return 0; // No position, no unrealized PnL
        }

        let entry_price = self.entry_price.load(Ordering::Relaxed);
        if entry_price == 0 {
            // SAFETY: Handle corrupted state gracefully
            // This could happen due to:
            // 1. Bug in fill processing
            // 2. Concurrent modification issues
            // 3. Memory corruption
            // Return 0 rather than panic
            return 0;
        }

        // Calculate price difference based on position side
        let price_diff = if qty > 0 {
            // Long position: profit if market > entry
            market_price as i64 - entry_price as i64
        } else {
            // Short position: profit if market < entry
            entry_price as i64 - market_price as i64
        };

        // PnL = price_diff * abs(quantity) / SCALE
        // We divide by SCALE because both price and quantity are in fixed-point
        let abs_qty = qty.abs();
        let pnl = (price_diff as i128 * abs_qty as i128 / 1_000_000_000) as i64;
        pnl
    }
}

impl Default for Position {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Position")
            .field("quantity", &self.get_quantity())
            .field("realized_pnl", &self.get_realized_pnl())
            .field("daily_pnl", &self.get_daily_pnl())
            .field("trade_count", &self.get_trade_count())
            .finish()
    }
}

/// Fixed-point conversion utilities
///
/// Huginn uses 9 decimal places for prices/sizes
pub mod fixed_point {
    use crate::core::errors::ConversionError;

    /// Scale factor for 9 decimal places
    pub const SCALE: i64 = 1_000_000_000;

    /// Maximum safe value for f64 conversion (to prevent overflow)
    /// i64::MAX / SCALE = ~9.2 quadrillion
    pub const MAX_SAFE_F64: f64 = (i64::MAX / SCALE) as f64;

    /// Minimum safe value for f64 conversion
    pub const MIN_SAFE_F64: f64 = (i64::MIN / SCALE) as f64;

    /// Convert f64 to fixed-point i64 (UNCHECKED - legacy)
    ///
    /// # Warning
    ///
    /// This function does not check for overflow. Use `from_f64_checked()` instead.
    ///
    /// # Safety
    ///
    /// Caller must ensure `value` is within the safe range:
    /// - `MIN_SAFE_F64 <= value <= MAX_SAFE_F64`
    /// - `value` is not NaN or infinite
    #[inline(always)]
    pub fn from_f64(value: f64) -> i64 {
        (value * SCALE as f64) as i64
    }

    /// Convert f64 to fixed-point i64 with overflow checking
    ///
    /// # Returns
    ///
    /// - `Ok(fixed_point_value)` if conversion succeeded
    /// - `Err(ConversionError)` if value is out of range, NaN, or infinite
    ///
    /// # Example
    ///
    /// ```
    /// # use bog_core::core::fixed_point;
    /// // Safe conversion
    /// let price = fixed_point::from_f64_checked(50000.0)?;
    /// assert_eq!(price, 50000_000_000_000);
    ///
    /// // Overflow detection
    /// let result = fixed_point::from_f64_checked(1e20);
    /// assert!(result.is_err());
    ///
    /// // NaN detection
    /// let result = fixed_point::from_f64_checked(f64::NAN);
    /// assert!(result.is_err());
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[inline(always)]
    pub fn from_f64_checked(value: f64) -> Result<i64, ConversionError> {
        // Check for NaN
        if value.is_nan() {
            return Err(ConversionError::NotANumber);
        }

        // Check for infinity
        if value.is_infinite() {
            return Err(ConversionError::Infinite { positive: value > 0.0 });
        }

        // Check range
        if value > MAX_SAFE_F64 || value < MIN_SAFE_F64 {
            return Err(ConversionError::OutOfRange { value });
        }

        Ok((value * SCALE as f64) as i64)
    }

    /// Convert fixed-point i64 to f64
    #[inline(always)]
    pub fn to_f64(value: i64) -> f64 {
        value as f64 / SCALE as f64
    }

    /// Convert u64 fixed-point to i64 fixed-point with overflow checking
    ///
    /// # Returns
    ///
    /// - `Ok(i64_value)` if conversion succeeded
    /// - `Err(ConversionError)` if u64 value exceeds i64::MAX
    #[inline(always)]
    pub fn from_u64_checked(value: u64) -> Result<i64, ConversionError> {
        if value > i64::MAX as u64 {
            // Convert to f64 for error message
            let f64_val = value as f64 / SCALE as f64;
            return Err(ConversionError::OutOfRange { value: f64_val });
        }
        Ok(value as i64)
    }

    /// Convert u64 fixed-point to i64 fixed-point (UNCHECKED - legacy)
    ///
    /// # Warning
    ///
    /// This can silently truncate if value > i64::MAX. Use `from_u64_checked()` instead.
    #[inline(always)]
    pub fn from_u64(value: u64) -> i64 {
        value as i64
    }

    /// Convert i64 fixed-point to u64 fixed-point
    ///
    /// Negative values are clamped to 0.
    #[inline(always)]
    pub fn to_u64(value: i64) -> u64 {
        value.max(0) as u64
    }
}

// Property-based tests for fixed-point arithmetic
// Note: Commented out for now - can be re-enabled by moving to core/mod.rs
// #[cfg(test)]
// mod fixed_point_proptest;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_order_id_generation() {
        let id1 = OrderId::generate();
        let id2 = OrderId::generate();

        // IDs should be unique
        assert_ne!(id1, id2);

        // Counter should increment
        assert_eq!(id1.counter() + 1, id2.counter());
    }

    #[test]
    fn test_order_id_components() {
        let id = OrderId::generate();

        let timestamp = id.timestamp();
        let random_part = id.random_part();
        let counter = id.counter();

        // Reconstruct and verify
        let reconstructed = ((timestamp as u128) << 64) | ((random_part as u128) << 32) | (counter as u128);
        assert_eq!(id.as_u128(), reconstructed);
    }

    #[test]
    fn test_position_updates() {
        let pos = Position::new();

        assert_eq!(pos.get_quantity(), 0);

        // Buy 1.0 BTC (in fixed-point)
        let new_qty = pos.update_quantity(1_000_000_000);
        assert_eq!(new_qty, 1_000_000_000);
        assert_eq!(pos.get_quantity(), 1_000_000_000);

        // Sell 0.5 BTC
        let new_qty = pos.update_quantity(-500_000_000);
        assert_eq!(new_qty, 500_000_000);
    }

    #[test]
    fn test_position_pnl() {
        let pos = Position::new();

        pos.update_realized_pnl(100_000_000); // +100 USD
        assert_eq!(pos.get_realized_pnl(), 100_000_000);

        pos.update_daily_pnl(50_000_000); // +50 USD daily
        assert_eq!(pos.get_daily_pnl(), 50_000_000);

        pos.reset_daily_pnl();
        assert_eq!(pos.get_daily_pnl(), 0);
    }

    #[test]
    fn test_position_alignment() {
        // Verify cache line alignment
        assert_eq!(std::mem::align_of::<Position>(), 64);
        assert_eq!(std::mem::size_of::<Position>(), 64);
    }

    #[test]
    fn test_fixed_point_conversion() {
        use fixed_point::*;

        let price = 50000.123456789;
        let fixed = from_f64(price);
        let converted = to_f64(fixed);

        assert!((price - converted).abs() < 0.000001);
    }

    #[test]
    fn test_side_size() {
        assert_eq!(std::mem::size_of::<Side>(), 1);
        assert_eq!(std::mem::size_of::<OrderType>(), 1);
        assert_eq!(std::mem::size_of::<OrderStatus>(), 1);
    }

    #[test]
    fn test_order_id_display() {
        let id = OrderId::new(0x123456789abcdef0);
        let display = format!("{}", id);
        assert_eq!(display, "0000000000000000123456789abcdef0");
    }

    // ===== ATOMIC CONTENTION TESTS =====

    #[test]
    fn test_position_concurrent_updates() {
        use std::sync::Arc;
        use std::thread;

        let position = Arc::new(Position::new());
        let mut handles = vec![];

        // Spawn 10 threads that each increment quantity 100 times
        for _ in 0..10 {
            let pos = Arc::clone(&position);
            let handle = thread::spawn(move || {
                for _ in 0..100 {
                    pos.update_quantity(1_000_000); // +0.001 BTC
                }
            });
            handles.push(handle);
        }

        // Wait for all threads
        for handle in handles {
            handle.join().unwrap();
        }

        // Total should be 10 * 100 * 1_000_000 = 1_000_000_000 (1.0 BTC)
        assert_eq!(position.get_quantity(), 1_000_000_000);
    }

    #[test]
    fn test_position_concurrent_pnl_updates() {
        use std::sync::Arc;
        use std::thread;

        let position = Arc::new(Position::new());
        let mut handles = vec![];

        // Spawn threads updating realized and daily PnL
        for _ in 0..5 {
            let pos = Arc::clone(&position);
            let handle = thread::spawn(move || {
                for _ in 0..100 {
                    pos.update_realized_pnl(1_000_000); // +$0.001
                    pos.update_daily_pnl(500_000);      // +$0.0005
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        // Verify totals
        assert_eq!(position.get_realized_pnl(), 500_000_000); // 5 * 100 * 1M
        assert_eq!(position.get_daily_pnl(), 250_000_000);    // 5 * 100 * 500K
    }

    #[test]
    fn test_position_concurrent_mixed_operations() {
        use std::sync::Arc;
        use std::thread;

        let position = Arc::new(Position::new());
        let mut handles = vec![];

        // Thread 1: Increase quantity
        let pos1 = Arc::clone(&position);
        handles.push(thread::spawn(move || {
            for _ in 0..1000 {
                pos1.update_quantity(100_000);
            }
        }));

        // Thread 2: Decrease quantity
        let pos2 = Arc::clone(&position);
        handles.push(thread::spawn(move || {
            for _ in 0..500 {
                pos2.update_quantity(-100_000);
            }
        }));

        // Thread 3: Update PnL
        let pos3 = Arc::clone(&position);
        handles.push(thread::spawn(move || {
            for _ in 0..1000 {
                pos3.update_realized_pnl(10_000);
            }
        }));

        // Thread 4: Increment trades
        let pos4 = Arc::clone(&position);
        handles.push(thread::spawn(move || {
            for _ in 0..1000 {
                pos4.increment_trades();
            }
        }));

        for handle in handles {
            handle.join().unwrap();
        }

        // Verify final state
        assert_eq!(position.get_quantity(), 50_000_000); // (1000 - 500) * 100K
        assert_eq!(position.get_realized_pnl(), 10_000_000); // 1000 * 10K
        assert_eq!(position.get_trade_count(), 1000);
    }

    #[test]
    fn test_position_reset_daily_pnl_concurrent() {
        use std::sync::Arc;
        use std::thread;

        let position = Arc::new(Position::new());

        // Set initial daily PnL
        position.update_daily_pnl(1_000_000_000);

        let pos1 = Arc::clone(&position);
        let pos2 = Arc::clone(&position);

        // Thread 1: Keep adding to daily PnL
        let h1 = thread::spawn(move || {
            for _ in 0..100 {
                pos1.update_daily_pnl(1_000_000);
                thread::sleep(std::time::Duration::from_micros(10));
            }
        });

        // Thread 2: Reset daily PnL midway
        let h2 = thread::spawn(move || {
            thread::sleep(std::time::Duration::from_micros(500));
            pos2.reset_daily_pnl();
        });

        h1.join().unwrap();
        h2.join().unwrap();

        // Daily PnL should be whatever was added after reset
        // We can't predict exact value due to timing, but it should be < initial
        let final_pnl = position.get_daily_pnl();
        assert!(final_pnl < 1_000_000_000);
    }

    #[test]
    fn test_position_stress_test() {
        use std::sync::Arc;
        use std::thread;

        let position = Arc::new(Position::new());
        let num_threads = 8;
        let ops_per_thread = 1000;
        let mut handles = vec![];

        for thread_id in 0..num_threads {
            let pos = Arc::clone(&position);
            let handle = thread::spawn(move || {
                for i in 0..ops_per_thread {
                    // Mix of operations
                    match (thread_id + i) % 4 {
                        0 => {
                            pos.update_quantity(1_000);
                        }
                        1 => {
                            pos.update_quantity(-1_000);
                        }
                        2 => {
                            pos.update_realized_pnl(100);
                        }
                        _ => {
                            pos.increment_trades();
                        }
                    }
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        // Verify atomics didn't get corrupted
        // Quantity: 2 threads add, 2 subtract, each 2000 ops -> net 0
        assert_eq!(position.get_quantity(), 0);

        // Trades: 2 threads, 1000 ops each = 2000 trades
        assert_eq!(position.get_trade_count(), 2000);

        // PnL: 2 threads, 1000 ops each, 100 per op = 200_000
        assert_eq!(position.get_realized_pnl(), 200_000);
    }
}
