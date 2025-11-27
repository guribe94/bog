# bog-strategies

**Zero-overhead HFT trading strategies**

Ultra-low-latency trading strategy implementations using zero-sized types (ZSTs) and compile-time configuration for sub-100ns signal generation.

## Overview

This crate provides market-making strategies optimized for high-frequency trading:

- **[SimpleSpread](src/simple_spread.rs)** - Basic market making with inventory management (✅ Production Ready)
- **[InventoryBased](src/inventory_based.rs)** - Avellaneda-Stoikov with volatility adjustment (🚧 Stub)

## Performance

| Strategy | Target | Achieved | Status |
|----------|--------|----------|--------|
| **SimpleSpread** | <100ns | **~5ns** | ✅ |
| **InventoryBased** | <100ns | TBD | 🚧 |

**Key Insight**: Zero-sized types (ZSTs) mean strategies occupy **0 bytes** of memory and compile to pure code with full LLVM optimization.

## Quick Start

### Installation

```toml
[dependencies]
bog-strategies = {
    path = "../bog-strategies",
    features = ["spread-10bps", "size-medium"]
}
```

### Basic Usage

```rust
use bog_strategies::SimpleSpread;
use bog_core::engine::Engine;
use bog_core::execution::SimulatedExecutor;

fn main() -> anyhow::Result<()> {
    // Create strategy (0 bytes!)
    let strategy = SimpleSpread;
    assert_eq!(std::mem::size_of_val(&strategy), 0);

    // Create executor
    let executor = SimulatedExecutor::new_default();

    // Create engine with full monomorphization
    let mut engine = Engine::new(strategy, executor);

    // Process market data
    engine.process_tick(&snapshot, true)?;

    Ok(())
}
```

## Strategies

### SimpleSpread - Basic Market Making

**Features:**
- Symmetric quotes around mid-price
- Configurable spread (5, 10, 20 bps)
- Configurable order size (0.01, 0.1, 1.0 BTC)
- Minimum spread filter
- Inventory-based skew adjustment (Avellaneda-Stoikov)
- Volatility-aware spread widening (stub - returns 1.0x currently)

**Performance**: ~5ns per signal generation

**Configuration**:

```toml
bog-strategies = { features = [
    "spread-10bps",      # 10 basis point spread
    "size-medium",       # 0.1 BTC orders
    "min-spread-1bps",   # Trade if market spread >= 1bp
] }
```

**Example Output**:

```
Mid Price: $50,000
Spread: 10 bps
Position: +0.5 BTC (long)

Output:
  Bid: $49,995 (adjusted down due to long position)
  Ask: $49,997.50 (adjusted down to encourage sells)
```

### InventoryBased - Avellaneda-Stoikov (Stub)

**Planned Features:**
- Dynamic spread based on volatility
- Inventory risk aversion
- Target inventory management
- Time-to-expiry consideration

**Status**: Stub implementation in Phase 4

## Configuration

All configuration is **compile-time** via Cargo features for zero-overhead.

### Spread Configuration

```toml
# Choose ONE spread configuration
features = ["spread-5bps"]   # 5 basis points (aggressive)
features = ["spread-10bps"]  # 10 basis points (balanced) - DEFAULT
features = ["spread-20bps"]  # 20 basis points (conservative)
```

**Profitability**: All spreads are profitable after 2 bps taker fees:
- 5 bps spread → 3 bps net profit ✅
- 10 bps spread → 8 bps net profit ✅
- 20 bps spread → 18 bps net profit ✅

### Order Size Configuration

```toml
# Choose ONE size configuration
features = ["size-small"]   # 0.01 BTC per order
features = ["size-medium"]  # 0.1 BTC per order - DEFAULT
features = ["size-large"]   # 1.0 BTC per order
```

### Min Spread Filter

```toml
# Choose ONE minimum spread threshold
features = ["min-spread-1bps"]   # Trade if market >= 1bp - DEFAULT
features = ["min-spread-5bps"]   # Trade if market >= 5bp
features = ["min-spread-10bps"]  # Trade if market >= 10bp
```

Filters out tight markets where profitability is marginal.

## Architecture

### Zero-Sized Types (ZSTs)

Strategies are implemented as **zero-sized types**:

```rust
pub struct SimpleSpread;  // 0 bytes!

impl Strategy for SimpleSpread {
    #[inline(always)]
    fn calculate(&mut self, snapshot: &MarketSnapshot, position: &Position)
        -> Option<Signal>
    {
        // All logic compiled inline
        // No dynamic dispatch
        // No heap allocations
    }
}
```

**Benefits**:
- **0 bytes memory** - Strategy state is compile-time only
- **Full inline** - LLVM optimizes across call boundaries
- **No allocation** - Stack-only execution
- **Cache-friendly** - No pointer dereferencing

### Fixed-Point Arithmetic

All calculations use **u64 fixed-point with 9 decimals**:

```rust
// Price: $50,000.123456789
let price: u64 = 50_000_123_456_789;

// Size: 0.1 BTC
let size: u64 = 100_000_000;

// Calculation (no Decimal allocations!)
let notional = (price as u128 * size as u128) / 1_000_000_000;
```

**Why u64 instead of Decimal?**
- Decimal allocates on heap
- u64 is Copy and stack-allocated
- ~10x faster for simple arithmetic
- Sufficient precision (9 decimals)

## Inventory Management

SimpleSpread implements **Avellaneda-Stoikov inventory skew**:

### Theory

When holding inventory, market makers face **inventory risk**. The optimal solution is to:

1. **Incentivize** trades that REDUCE inventory
2. **Disincentivize** trades that INCREASE inventory

### Implementation

```rust
// Long position (+0.5 BTC): Shift quotes DOWN to encourage sells
if inventory_ratio > 0.01 {
    our_ask = our_ask - ask_adjustment;        // Lower ask (incentive)
    our_bid = our_bid - (bid_adjustment / 2);  // Lower bid (disincentive)
}

// Short position (-0.5 BTC): Shift quotes UP to encourage buys
else if inventory_ratio < -0.01 {
    our_bid = our_bid + bid_adjustment;        // Raise bid (incentive)
    our_ask = our_ask + (ask_adjustment / 2);  // Raise ask (disincentive)
}
```

### Example with Numbers

**Base quotes**: bid=$50,000, ask=$50,010 (10 bps spread)
**Position**: +0.5 BTC (50% of max)
**Skew**: 5 bps adjustment

**Result**:
- **Ask**: $50,010 - $25 = $49,985 (INCENTIVE: sell to us!)
- **Bid**: $50,000 - $12.50 = $49,987.50 (DISINCENTIVE: don't sell to us)
- **New spread**: $2.50 (0.5 bps) - much tighter, favoring sells

This creates economic incentive for market takers to reduce our long position.

See [simple_spread.rs lines 657-712](src/simple_spread.rs#L657) for full documentation.

## Examples

### Custom Spread Strategy

```rust
use bog_core::engine::Strategy;
use bog_core::core::{Signal, Position};
use huginn::MarketSnapshot;

pub struct WideSpread;  // 50 bps spread

impl Strategy for WideSpread {
    fn calculate(&mut self, snapshot: &MarketSnapshot, _position: &Position)
        -> Option<Signal>
    {
        let mid = (snapshot.best_bid_price + snapshot.best_ask_price) / 2;
        let spread = 50_000_000_000; // 50 bps in fixed-point

        Some(Signal::quote_both(
            mid - spread / 2,  // bid
            mid + spread / 2,  // ask
            50_000_000,        // 0.05 BTC
        ))
    }

    fn name(&self) -> &'static str {
        "WideSpread"
    }
}
```

### Strategy with State (Non-ZST)

```rust
pub struct MomentumStrategy {
    last_price: u64,
    trend: i8,  // -1, 0, +1
}

impl Strategy for MomentumStrategy {
    fn calculate(&mut self, snapshot: &MarketSnapshot, position: &Position)
        -> Option<Signal>
    {
        let mid = (snapshot.best_bid_price + snapshot.best_ask_price) / 2;

        // Update trend
        if mid > self.last_price { self.trend = 1; }
        else if mid < self.last_price { self.trend = -1; }
        self.last_price = mid;

        // Skew quotes based on trend
        if self.trend > 0 {
            Some(Signal::quote_ask(mid + 5_000_000_000, 100_000_000))
        } else {
            Some(Signal::quote_bid(mid - 5_000_000_000, 100_000_000))
        }
    }

    fn name(&self) -> &'static str {
        "Momentum"
    }
}
```

## Testing

```bash
# Run strategy tests
cargo test -p bog-strategies

# Run with specific features
cargo test -p bog-strategies --features spread-5bps,size-small

# Benchmark strategy performance
cargo bench -p bog-strategies
```

## Feature Matrix

| Feature | Values | Default |
|---------|--------|---------|
| **spread** | 5bps, 10bps, 20bps | 10bps |
| **size** | small (0.01), medium (0.1), large (1.0) | medium |
| **min-spread** | 1bps, 5bps, 10bps | 1bps |

## Documentation

- **[API Docs](https://docs.rs/bog-strategies)** - Full API reference
- **[SimpleSpread Source](src/simple_spread.rs)** - Implementation with diagrams
- **[Fee Calculation](src/fees.rs)** - Profitability analysis

## Contributing

See [CONTRIBUTING.md](../CONTRIBUTING.md) for guidelines.

## License

MIT - See [LICENSE](../LICENSE) for details.
