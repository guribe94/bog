//! Zero-overhead core types for HFT trading
//!
//! All types in this module are designed for:
//! - Zero heap allocations
//! - Copy semantics where possible
//! - Cache-line alignment
//! - Minimal memory footprint

use std::fmt;
use std::sync::atomic::{AtomicI64, AtomicU32, AtomicU64, Ordering};

/// Unique identifier for an order
///
/// Uses u128 instead of String for zero-allocation, copy semantics.
/// Generated using thread-local counter + timestamp + random bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
    /// Format: [timestamp:64][random:32][counter:32]
    /// This ensures uniqueness across threads and time
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
            .unwrap()
            .as_nanos() as u64;

        let random_part = RNG.with(|rng| rng.borrow_mut().gen::<u32>());

        let counter = COUNTER.with(|c| {
            let val = c.get();
            c.set(val.wrapping_add(1));
            val
        });

        let id = ((timestamp as u128) << 64) | ((random_part as u128) << 32) | (counter as u128);
        Self(id)
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    /// Scale factor for 9 decimal places
    pub const SCALE: i64 = 1_000_000_000;

    /// Convert f64 to fixed-point i64
    #[inline(always)]
    pub fn from_f64(value: f64) -> i64 {
        (value * SCALE as f64) as i64
    }

    /// Convert fixed-point i64 to f64
    #[inline(always)]
    pub fn to_f64(value: i64) -> f64 {
        value as f64 / SCALE as f64
    }

    /// Convert u64 fixed-point to i64 fixed-point
    #[inline(always)]
    pub fn from_u64(value: u64) -> i64 {
        value as i64
    }

    /// Convert i64 fixed-point to u64 fixed-point
    #[inline(always)]
    pub fn to_u64(value: i64) -> u64 {
        value.max(0) as u64
    }
}

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
}
