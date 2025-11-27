# Trait Implementation Guide

Complete guide to implementing custom `Strategy` and `Executor` traits for the bog trading engine.

## Table of Contents

- [Strategy Trait](#strategy-trait)
  - [Complete Example: EMA Crossover](#complete-example-ema-crossover)
  - [Fixed-Point Arithmetic Patterns](#fixed-point-arithmetic-patterns)
  - [Performance Optimization](#performance-optimization)
  - [Testing Your Strategy](#testing-your-strategy)
- [Executor Trait](#executor-trait)
  - [Complete Example: Paper Trading Executor](#complete-example-paper-trading-executor)
  - [Object Pool Pattern](#object-pool-pattern)
  - [Fill Generation](#fill-generation)
- [Common Patterns](#common-patterns)
- [Troubleshooting](#troubleshooting)

---

## Strategy Trait

The `Strategy` trait is the core interface for implementing trading algorithms:

```rust
pub trait Strategy {
    /// Calculate trading signal from market snapshot and current position
    fn calculate(&mut self, snapshot: &MarketSnapshot, position: &Position) -> Option<Signal>;

    /// Strategy name for logging
    fn name(&self) -> &'static str;

    /// Reset strategy state (optional)
    fn reset(&mut self) {}
}
```

### Complete Example: EMA Crossover

Here's a complete implementation of an EMA crossover strategy using zero-sized types and fixed-point arithmetic:

```rust
//! EMA Crossover Strategy - Buy when fast EMA > slow EMA
//!
//! This strategy demonstrates:
//! - Zero-sized type pattern for maximum performance
//! - Fixed-point arithmetic (9 decimal places)
//! - Compile-time configuration via Cargo features
//! - Inline everything for zero overhead

use bog_core::core::{Position, Signal, Side};
use bog_core::data::MarketSnapshot;
use bog_core::engine::Strategy;

// Configuration from Cargo features
#[cfg(not(any(feature = "ema-fast-10", feature = "ema-fast-20")))]
pub const EMA_FAST_PERIODS: u32 = 10; // Default: 10 periods

#[cfg(feature = "ema-fast-10")]
pub const EMA_FAST_PERIODS: u32 = 10;

#[cfg(feature = "ema-fast-20")]
pub const EMA_FAST_PERIODS: u32 = 20;

#[cfg(not(any(feature = "ema-slow-20", feature = "ema-slow-50")))]
pub const EMA_SLOW_PERIODS: u32 = 50; // Default: 50 periods

#[cfg(feature = "ema-slow-20")]
pub const EMA_SLOW_PERIODS: u32 = 20;

#[cfg(feature = "ema-slow-50")]
pub const EMA_SLOW_PERIODS: u32 = 50;

// Order size configuration
pub const ORDER_SIZE: u64 = 100_000_000; // 0.1 BTC in fixed-point

/// EMA Crossover Strategy - Zero-Sized Type
///
/// Memory footprint: 0 bytes at runtime (parameters are const)
/// Performance: ~30ns per calculation (all inline, no allocations)
pub struct EmaCrossover {
    // EMA state (mutable state stored here)
    fast_ema: u64,
    slow_ema: u64,
    initialized: bool,
}

impl EmaCrossover {
    /// Create new EMA crossover strategy
    pub const fn new() -> Self {
        Self {
            fast_ema: 0,
            slow_ema: 0,
            initialized: false,
        }
    }

    /// Calculate EMA using fixed-point arithmetic
    ///
    /// Formula: EMA = (price * alpha) + (previous_ema * (1 - alpha))
    /// Where alpha = 2 / (periods + 1)
    ///
    /// Fixed-point math:
    /// - All prices are u64 with 9 decimals
    /// - Alpha is scaled by 1_000_000 for precision
    #[inline(always)]
    fn update_ema(&self, current_ema: u64, new_price: u64, periods: u32) -> u64 {
        if current_ema == 0 {
            // First update: EMA = price
            return new_price;
        }

        // Calculate alpha = 2 / (periods + 1)
        // Scale by 1_000_000 to keep precision
        let alpha = (2_000_000) / (periods + 1);

        // EMA = price * alpha + prev_ema * (1 - alpha)
        // = price * alpha + prev_ema - prev_ema * alpha
        // = prev_ema + alpha * (price - prev_ema)

        let diff = if new_price > current_ema {
            new_price - current_ema
        } else {
            current_ema - new_price
        };

        let adjustment = (diff as u128 * alpha as u128) / 1_000_000;

        if new_price > current_ema {
            current_ema + adjustment as u64
        } else {
            current_ema - adjustment as u64
        }
    }
}

impl Strategy for EmaCrossover {
    #[inline(always)]
    fn calculate(&mut self, snapshot: &MarketSnapshot, position: &Position) -> Option<Signal> {
        // Calculate mid price
        let mid = (snapshot.best_bid_price + snapshot.best_ask_price) / 2;

        // Update EMAs
        self.fast_ema = self.update_ema(self.fast_ema, mid, EMA_FAST_PERIODS);
        self.slow_ema = self.update_ema(self.slow_ema, mid, EMA_SLOW_PERIODS);

        // Need at least 2 updates before trading
        if !self.initialized {
            self.initialized = true;
            return Some(Signal::no_action());
        }

        // Get current position
        let qty = position.get_quantity();

        // Crossover logic
        if self.fast_ema > self.slow_ema {
            // Fast > Slow: Bullish - go long
            if qty <= 0 {
                // Not long: buy
                return Some(Signal::quote_bid(snapshot.best_bid_price, ORDER_SIZE));
            }
        } else {
            // Fast < Slow: Bearish - go flat/short
            if qty > 0 {
                // Currently long: sell
                return Some(Signal::quote_ask(snapshot.best_ask_price, ORDER_SIZE));
            }
        }

        // No change needed
        Some(Signal::no_action())
    }

    fn name(&self) -> &'static str {
        "EMA-Crossover"
    }

    fn reset(&mut self) {
        self.fast_ema = 0;
        self.slow_ema = 0;
        self.initialized = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bog_core::testing::helpers::{create_test_snapshot, create_test_position};

    #[test]
    fn test_ema_initialization() {
        let mut strategy = EmaCrossover::new();
        let snapshot = create_test_snapshot(
            1, 1,
            50_000_000_000_000, // $50,000 bid
            50_005_000_000_000, // $50,005 ask
            1_000_000_000,      // 1.0 BTC
            1_000_000_000,
        );
        let position = create_test_position(0);

        // First call initializes
        let signal = strategy.calculate(&snapshot, &position);
        assert!(signal.is_some());
        assert!(strategy.fast_ema > 0);
        assert!(strategy.slow_ema > 0);
    }

    #[test]
    fn test_ema_update() {
        let mut strategy = EmaCrossover::new();

        // Test EMA calculation
        let ema = strategy.update_ema(0, 50_000_000_000_000, 10);
        assert_eq!(ema, 50_000_000_000_000); // First update = price

        let ema2 = strategy.update_ema(ema, 51_000_000_000_000, 10);
        assert!(ema2 > ema); // Price increased, EMA should increase
        assert!(ema2 < 51_000_000_000_000); // But not all the way to new price
    }

    #[test]
    fn test_crossover_signal() {
        let mut strategy = EmaCrossover::new();
        let position = create_test_position(0);

        // Simulate uptrend: fast EMA will cross above slow EMA
        for i in 0..100 {
            let price = 50_000_000_000_000 + (i * 10_000_000_000); // Price rising
            let snapshot = create_test_snapshot(
                1, i, price, price + 5_000_000_000, 1_000_000_000, 1_000_000_000
            );
            strategy.calculate(&snapshot, &position);
        }

        // Fast should be above slow after uptrend
        assert!(strategy.fast_ema > strategy.slow_ema);
    }
}
```

### Fixed-Point Arithmetic Patterns

All prices and sizes use u64 fixed-point with 9 decimal places:

```rust
// Constants
const DECIMALS: u64 = 1_000_000_000; // 10^9

// Price conversion
let dollars = 50_000.50;
let fixed_price = (dollars * 1_000_000_000.0) as u64; // 50_000_500_000_000

// BTC amount conversion
let btc = 0.1;
let fixed_size = (btc * 1_000_000_000.0) as u64; // 100_000_000

// Calculate percentage (basis points)
let price = 50_000_000_000_000_u64; // $50,000
let bps = 10; // 10 basis points = 0.1%
let adjustment = (price as u128 * bps as u128) / 1_000_000;
let new_price = price + adjustment as u64;

// Multiplication (watch for overflow!)
let price = 50_000_000_000_000_u64;
let quantity = 1_000_000_000_u64; // 1.0 BTC
let value = (price as u128 * quantity as u128) / DECIMALS as u128; // $50,000 in fixed-point

// Division
let total_value = 100_000_000_000_000_u64; // $100,000
let total_size = 2_000_000_000_u64; // 2.0 BTC
let avg_price = (total_value as u128 * DECIMALS as u128) / total_size as u128;
```

**Key Rules:**
1. Always use u128 for intermediate calculations to prevent overflow
2. When multiplying two fixed-point numbers, divide by DECIMALS once
3. When dividing, multiply numerator by DECIMALS before dividing
4. Use saturating operations for safety: `saturating_add`, `saturating_sub`

### Performance Optimization

**1. Use #[inline(always)] for hot path methods:**

```rust
impl Strategy for MyStrategy {
    #[inline(always)]  // ✅ Critical for performance
    fn calculate(&mut self, snapshot: &MarketSnapshot, position: &Position) -> Option<Signal> {
        // Implementation
    }
}
```

**2. Avoid heap allocations:**

```rust
// ❌ BAD: Heap allocation
fn calculate_average(&self, prices: Vec<u64>) -> u64 {
    prices.iter().sum::<u64>() / prices.len() as u64
}

// ✅ GOOD: Stack-allocated array
fn calculate_average(&self, prices: &[u64; 10]) -> u64 {
    prices.iter().sum::<u64>() / 10
}
```

**3. Use const for configuration:**

```rust
// ✅ GOOD: Compile-time constant
const THRESHOLD: u64 = 100_000_000;

// ❌ BAD: Runtime field
struct Strategy {
    threshold: u64,  // Prevents const-folding
}
```

**4. Minimize state:**

```rust
// ✅ GOOD: Minimal state
pub struct SimpleStrategy {
    last_price: u64,  // Only what's needed
}

// ❌ BAD: Unnecessary state
pub struct BloatedStrategy {
    last_100_prices: Vec<u64>,  // Heap allocation
    history: HashMap<u64, u64>, // Heap allocation
    // ... more state
}
```

### Testing Your Strategy

**Unit tests:**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use bog_core::testing::helpers::*;

    #[test]
    fn test_strategy_zero_sized() {
        use std::mem::size_of;
        assert_eq!(size_of::<MyStrategy>(), EXPECTED_SIZE);
    }

    #[test]
    fn test_signal_generation() {
        let mut strategy = MyStrategy::new();
        let snapshot = create_simple_snapshot(1);
        let position = create_test_position(0);

        let signal = strategy.calculate(&snapshot, &position);
        assert!(signal.is_some());
    }

    #[test]
    fn test_performance() {
        use std::time::Duration;

        let mut strategy = MyStrategy::new();
        let snapshot = create_simple_snapshot(1);
        let position = create_test_position(0);

        assert_within_latency(Duration::from_nanos(100), || {
            strategy.calculate(&snapshot, &position);
        }, "calculate");
    }
}
```

**Integration tests:**

```rust
#[test]
fn test_with_engine() {
    use bog_core::engine::{Engine, SimulatedExecutor};

    let strategy = MyStrategy::new();
    let executor = SimulatedExecutor::new();
    let mut engine = Engine::new(strategy, executor);

    let snapshot = create_simple_snapshot(1);
    engine.process_tick(&snapshot, false).unwrap();

    let stats = engine.stats();
    assert!(stats.ticks > 0);
}
```

---

## Executor Trait

The `Executor` trait handles order placement and fill generation:

```rust
pub trait Executor {
    /// Execute a trading signal
    fn execute(&mut self, signal: Signal, position: &Position) -> Result<()>;

    /// Cancel all outstanding orders
    fn cancel_all(&mut self) -> Result<()>;

    /// Get pending fills
    fn get_fills(&mut self) -> Vec<Fill>;

    /// Get dropped fill count (for overflow detection)
    fn dropped_fill_count(&self) -> u64;

    /// Executor name for logging
    fn name(&self) -> &'static str;
}
```

### Complete Example: Paper Trading Executor

```rust
//! Paper Trading Executor - Simulates instant fills with realistic fees
//!
//! This executor demonstrates:
//! - Object pool pattern for zero-allocation fills
//! - Lock-free fill queue using crossbeam
//! - Proper fee accounting
//! - Fill tracking and metrics

use bog_core::core::{Position, Signal, SignalAction, Side};
use bog_core::execution::{Fill, OrderId};
use bog_core::engine::Executor;
use anyhow::Result;
use crossbeam::queue::ArrayQueue;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicU128, Ordering};

/// Paper trading executor with realistic fill simulation
pub struct PaperExecutor {
    /// Fill queue (lock-free, bounded capacity)
    fill_queue: Arc<ArrayQueue<Fill>>,

    /// Order ID counter
    next_order_id: AtomicU128,

    /// Dropped fill counter (overflow tracking)
    dropped_fills: AtomicU64,

    /// Fee rate in basis points (default: 2 bps for Lighter taker)
    fee_bps: u32,
}

impl PaperExecutor {
    pub fn new() -> Self {
        Self::new_with_fee_bps(2) // 2 bps = 0.02% taker fee
    }

    pub fn new_with_fee_bps(fee_bps: u32) -> Self {
        Self {
            fill_queue: Arc::new(ArrayQueue::new(1024)), // 1024 fill capacity
            next_order_id: AtomicU128::new(1),
            dropped_fills: AtomicU64::new(0),
            fee_bps,
        }
    }

    /// Generate next order ID
    #[inline(always)]
    fn next_id(&self) -> OrderId {
        OrderId::from(self.next_order_id.fetch_add(1, Ordering::Relaxed))
    }

    /// Calculate fee for a fill
    #[inline(always)]
    fn calculate_fee(&self, price: u64, size: u64) -> i64 {
        // Fee = (price * size * fee_bps) / 1_000_000_000 / 10_000
        // Note: price * size gives value in 18 decimals, divide by 1e9 to get 9 decimals
        let value = (price as u128 * size as u128) / 1_000_000_000;
        let fee = (value * self.fee_bps as u128) / 10_000;
        fee as i64
    }

    /// Create and enqueue a fill
    fn create_fill(&self, order_id: OrderId, side: Side, price: u64, size: u64) {
        let fee = self.calculate_fee(price, size);

        let fill = Fill {
            order_id,
            side,
            price,
            size,
            fee,
            timestamp_ns: 0, // Would use actual timestamp in production
        };

        if self.fill_queue.push(fill).is_err() {
            // Queue full - increment dropped counter
            self.dropped_fills.fetch_add(1, Ordering::Relaxed);
            tracing::warn!("Fill queue overflow - fill dropped!");
        }
    }
}

impl Executor for PaperExecutor {
    #[inline(always)]
    fn execute(&mut self, signal: Signal, _position: &Position) -> Result<()> {
        match signal.action {
            SignalAction::QuoteBoth => {
                // Place both bid and ask
                let bid_id = self.next_id();
                let ask_id = self.next_id();

                // Simulate instant fills at our quoted prices
                self.create_fill(bid_id, Side::Buy, signal.bid_price, signal.size);
                self.create_fill(ask_id, Side::Sell, signal.ask_price, signal.size);
            }
            SignalAction::QuoteBid => {
                let id = self.next_id();
                self.create_fill(id, Side::Buy, signal.bid_price, signal.size);
            }
            SignalAction::QuoteAsk => {
                let id = self.next_id();
                self.create_fill(id, Side::Sell, signal.ask_price, signal.size);
            }
            SignalAction::CancelAll => {
                // Paper trading: no orders to cancel
            }
            SignalAction::NoAction => {
                // Nothing to do
            }
        }

        Ok(())
    }

    fn cancel_all(&mut self) -> Result<()> {
        // Paper trading: instant fills mean no outstanding orders
        Ok(())
    }

    fn get_fills(&mut self) -> Vec<Fill> {
        let mut fills = Vec::new();

        // Drain fill queue
        while let Some(fill) = self.fill_queue.pop() {
            fills.push(fill);
        }

        fills
    }

    fn dropped_fill_count(&self) -> u64 {
        self.dropped_fills.load(Ordering::Relaxed)
    }

    fn name(&self) -> &'static str {
        "Paper-Executor"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bog_core::core::Signal;

    #[test]
    fn test_executor_creation() {
        let executor = PaperExecutor::new();
        assert_eq!(executor.name(), "Paper-Executor");
        assert_eq!(executor.dropped_fill_count(), 0);
    }

    #[test]
    fn test_execute_quote_both() {
        let mut executor = PaperExecutor::new();
        let position = Position::new();

        let signal = Signal::quote_both(
            50_000_000_000_000, // $50,000 bid
            50_010_000_000_000, // $50,010 ask
            100_000_000,        // 0.1 BTC
        );

        executor.execute(signal, &position).unwrap();

        let fills = executor.get_fills();
        assert_eq!(fills.len(), 2); // Both bid and ask filled

        // Check fees
        for fill in fills {
            assert!(fill.fee > 0); // Fee should be charged
        }
    }

    #[test]
    fn test_fee_calculation() {
        let executor = PaperExecutor::new_with_fee_bps(2); // 2 bps

        let fee = executor.calculate_fee(
            50_000_000_000_000, // $50,000
            1_000_000_000,      // 1.0 BTC
        );

        // Fee = $50,000 * 1.0 BTC * 0.0002 = $10 = 10_000_000_000
        assert_eq!(fee, 10_000_000_000);
    }

    #[test]
    fn test_fill_queue_overflow() {
        let mut executor = PaperExecutor::new();
        let position = Position::new();

        // Fill queue capacity is 1024, so create 2000 fills
        for _ in 0..2000 {
            let signal = Signal::quote_bid(50_000_000_000_000, 100_000_000);
            executor.execute(signal, &position).unwrap();
        }

        // Should have dropped some fills
        assert!(executor.dropped_fill_count() > 0);
    }
}
```

### Object Pool Pattern

For production executors, use object pools to avoid allocations:

```rust
use crossbeam::queue::ArrayQueue;
use std::sync::Arc;

pub struct PooledExecutor {
    // Order pool (reusable order objects)
    order_pool: Arc<ArrayQueue<Order>>,

    // Fill queue (bounded)
    fill_queue: Arc<ArrayQueue<Fill>>,
}

impl PooledExecutor {
    pub fn new() -> Self {
        let order_pool = Arc::new(ArrayQueue::new(256));

        // Pre-populate pool
        for _ in 0..256 {
            let _ = order_pool.push(Order::default());
        }

        Self {
            order_pool,
            fill_queue: Arc::new(ArrayQueue::new(1024)),
        }
    }

    fn acquire_order(&self) -> Option<Order> {
        self.order_pool.pop()
    }

    fn release_order(&self, order: Order) {
        let _ = self.order_pool.push(order);
    }
}
```

### Fill Generation

**Realistic fill simulation patterns:**

```rust
// 1. Instant fills (paper trading)
fn execute_instant(&mut self, price: u64, size: u64, side: Side) {
    let fill = Fill {
        price,
        size,
        side,
        fee: self.calculate_fee(price, size),
        timestamp_ns: now(),
    };
    self.fill_queue.push(fill);
}

// 2. Probabilistic fills (more realistic)
fn execute_probabilistic(&mut self, price: u64, size: u64, side: Side) {
    use rand::Rng;
    let mut rng = rand::thread_rng();

    if rng.gen::<f64>() < 0.8 { // 80% fill rate
        let filled_size = if rng.gen::<f64>() < 0.2 {
            // 20% partial fills
            (size as f64 * rng.gen_range(0.3..0.9)) as u64
        } else {
            size
        };

        let fill = Fill {
            price,
            size: filled_size,
            side,
            fee: self.calculate_fee(price, filled_size),
            timestamp_ns: now(),
        };
        self.fill_queue.push(fill);
    }
}

// 3. Latency simulation
fn execute_with_latency(&mut self, price: u64, size: u64, side: Side) {
    use std::thread;
    use std::time::Duration;

    // Simulate 50-150μs latency
    let latency_us = rand::thread_rng().gen_range(50..150);
    thread::sleep(Duration::from_micros(latency_us));

    self.execute_instant(price, size, side);
}
```

---

## Common Patterns

### 1. State Management

```rust
// ✅ GOOD: Minimal mutable state
pub struct Strategy {
    ema: u64,           // Only essential state
    initialized: bool,
}

// ❌ BAD: Excessive state
pub struct Strategy {
    history: Vec<u64>,  // Heap allocation
    cache: HashMap<u64, Signal>, // Heap allocation
}
```

### 2. Error Handling

```rust
// ✅ GOOD: Propagate errors
fn execute(&mut self, signal: Signal) -> Result<()> {
    self.validate_signal(&signal)?;
    self.place_orders(signal)?;
    Ok(())
}

// ❌ BAD: Swallow errors
fn execute(&mut self, signal: Signal) {
    let _ = self.place_orders(signal); // Error ignored!
}
```

### 3. Signal Validation

```rust
impl Strategy {
    fn calculate(&mut self, snapshot: &MarketSnapshot, position: &Position) -> Option<Signal> {
        // Always check market validity
        if snapshot.best_bid_price == 0 || snapshot.best_ask_price == 0 {
            return Some(Signal::no_action());
        }

        // Check for crossed market
        if snapshot.best_bid_price >= snapshot.best_ask_price {
            tracing::warn!("Crossed market detected");
            return Some(Signal::no_action());
        }

        // Your logic here
        // ...
    }
}
```

---

## Troubleshooting

### Issue: Strategy is slow (>100ns)

**Diagnosis:**
```rust
use std::time::Instant;

let start = Instant::now();
strategy.calculate(&snapshot, &position);
let elapsed = start.elapsed();
println!("Strategy took: {:?}", elapsed);
```

**Solutions:**
1. Add `#[inline(always)]` to `calculate()`
2. Use const for configuration instead of fields
3. Avoid heap allocations (Vec, HashMap, etc.)
4. Use fixed-point instead of Decimal
5. Profile with `cargo bench`

### Issue: Fill queue overflow

**Diagnosis:**
```rust
let dropped = executor.dropped_fill_count();
if dropped > 0 {
    eprintln!("WARNING: {} fills dropped!", dropped);
}
```

**Solutions:**
1. Increase fill queue capacity (default: 1024)
2. Process fills more frequently
3. Check if fills are being consumed by engine

### Issue: Position tracking incorrect

**Diagnosis:**
```rust
// Check position reconciliation
let expected = /* calculate from fills */;
let actual = position.get_quantity();
if expected != actual {
    eprintln!("Position mismatch: expected {} got {}", expected, actual);
}
```

**Solutions:**
1. Verify all fills are being applied
2. Check for dropped fills
3. Ensure fills have correct sign (buy=+, sell=-)
4. Use position reconciliation module

### Issue: Strategy generates no signals

**Diagnosis:**
```rust
impl Strategy {
    fn calculate(&mut self, snapshot: &MarketSnapshot, position: &Position) -> Option<Signal> {
        eprintln!("Mid: {}, Fast EMA: {}, Slow EMA: {}",
            self.mid, self.fast_ema, self.slow_ema);
        // ...
    }
}
```

**Solutions:**
1. Check if strategy needs initialization period
2. Verify market data is valid
3. Check if thresholds are too strict
4. Review strategy logic with debug prints

---

## Further Reading

- [Market Making Guide](./market-making-guide.md) - Strategy design patterns
- [Benchmark Guide](./benchmark-guide.md) - Performance testing
- [bog-core API docs](../../bog-core/README.md) - Core types reference
- [bog-strategies source](../../bog-strategies/src/) - Example implementations

---

**Questions?** Check the [GitHub issues](https://github.com/your-repo/bog/issues) or create a new one.
