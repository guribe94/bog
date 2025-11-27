# bog-core

**Ultra-low-latency HFT trading engine core library**

[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Core primitives and execution engine for high-frequency cryptocurrency trading with sub-microsecond tick-to-trade latency.

## Features

- **Zero-allocation hot path** - Object pools, fixed-size queues, stack-only execution
- **Const generic engine** - Full monomorphization for inline everything
- **Lock-free position tracking** - Atomic operations with `Acquire`/`Release` ordering
- **Type-safe state machines** - Compile-time order lifecycle validation
- **Real-time safe** - No heap allocations, mutexes, or dynamic dispatch in critical path

## Performance

| Metric | Target | Achieved |
|--------|--------|----------|
| **Tick-to-trade** | <1μs | **70.79ns** ✅ |
| **Engine overhead** | <100ns | **2.19ns** ✅ |
| **Signal generation** | <100ns | **15-18ns** ✅ |
| **Position update** | <50ns | **6-7ns** ✅ |

See [LATEST.md](../docs/benchmarks/LATEST.md) for detailed benchmarks.

## Quick Start

### Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
bog-core = { path = "../bog-core", features = ["simulated"] }
bog-strategies = { path = "../bog-strategies", features = ["spread-10bps"] }
```

### Basic Usage

```rust
use bog_core::engine::{Engine, SimulatedExecutor};
use bog_strategies::SimpleSpread;

fn main() -> anyhow::Result<()> {
    // Create strategy (zero-sized type - 0 bytes!)
    let strategy = SimpleSpread;

    // Create executor with instant fills
    let executor = SimulatedExecutor::new_default();

    // Create engine with full monomorphization
    let mut engine = Engine::new(strategy, executor);

    // Process market data
    for snapshot in market_feed {
        engine.process_tick(&snapshot, true)?;
    }

    let stats = engine.stats();
    println!("Processed {} ticks", stats.ticks_processed);

    Ok(())
}
```

## Architecture

### Component Overview

```
┌─────────────────────────────────────────────────────────────┐
│                        bog-core                             │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────┐ │
│  │   Strategy   │───>│    Engine    │───>│   Executor   │ │
│  │   (trait)    │    │ (const gen)  │    │   (trait)    │ │
│  └──────────────┘    └──────────────┘    └──────────────┘ │
│         │                    │                    │         │
│         v                    v                    v         │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────┐ │
│  │   Position   │<───│     Risk     │    │    Fills     │ │
│  │  (atomic)    │    │   Manager    │    │  (pooled)    │ │
│  └──────────────┘    └──────────────┘    └──────────────┘ │
│                                                             │
│  ┌──────────────────────────────────────────────────────┐  │
│  │              Market Data (huginn)                    │  │
│  │    ┌──────────┐  ┌──────────┐  ┌──────────┐        │  │
│  │    │Snapshot  │─>│L2 Book   │─>│Validation│        │  │
│  │    └──────────┘  └──────────┘  └──────────┘        │  │
│  └──────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

### Data Flow

**Tick Processing Pipeline** (~70ns total):

1. **Market Data** (0ns) - Snapshot from Huginn via shared memory
2. **Validation** (~20ns) - Check data integrity, staleness
3. **Strategy** (15-18ns) - Calculate bid/ask quotes
4. **Risk Check** (2ns) - Validate position limits
5. **Execute** (~10ns) - Place orders (simulated)
6. **Position Update** (6-7ns) - Atomic position tracking
7. **Metrics** (~2ns) - Increment counters

### Key Components

- **[Engine](src/engine/)** - Tick processing loop with const generic strategy/executor
- **[Execution](src/execution/)** - Order placement backends (simulated, live)
- **[Risk](src/risk/)** - Position tracking, limit validation, circuit breakers
- **[Data](src/data/)** - Market snapshot handling, gap recovery, validation
- **[Core](src/core/)** - Primitives (OrderId, Signal, Position, types)
- **[OrderBook](src/orderbook/)** - L2 book reconstruction, VWAP calculations
- **[Monitoring](src/monitoring/)** - Alerts, Prometheus metrics
- **[Resilience](src/resilience/)** - Circuit breakers, stale data detection

## Configuration

bog-core uses **compile-time configuration** via Cargo features for zero-overhead:

### Strategy Selection

```toml
bog-core = { features = ["simple-spread"] }  # SimpleSpread strategy
bog-core = { features = ["inventory-based"] }  # Avellaneda-Stoikov
```

### Execution Mode

```toml
bog-core = { features = ["simulated"] }  # Paper trading (instant fills)
bog-core = { features = ["live"] }        # Live trading (Lighter DEX)
```

### Risk Limits

```toml
bog-core = { features = [
    "max-position-one",    # Max 1.0 BTC position
    "max-order-half",      # Max 0.5 BTC per order
    "max-daily-loss-1000", # Max $1000 daily loss
] }
```

See [Cargo.toml features](Cargo.toml#L104-L162) for all options.

## Examples

### Custom Strategy

```rust
use bog_core::engine::Strategy;
use bog_core::core::{Signal, Position};
use bog_core::data::MarketSnapshot;

struct MyStrategy;

impl Strategy for MyStrategy {
    fn calculate(&mut self, snapshot: &MarketSnapshot, position: &Position)
        -> Option<Signal>
    {
        let mid = (snapshot.best_bid_price + snapshot.best_ask_price) / 2;
        let spread = 10_000_000_000; // 10 bps

        Some(Signal::quote_both(
            mid - spread / 2,  // bid
            mid + spread / 2,  // ask
            100_000_000,       // 0.1 BTC size
        ))
    }

    fn name(&self) -> &'static str {
        "MyStrategy"
    }
}
```

### Real-time CPU Pinning

```rust
use bog_core::perf::cpu;

fn main() -> anyhow::Result<()> {
    // Pin to CPU core 3
    cpu::pin_to_core(3)?;

    // Set real-time priority (Linux only)
    #[cfg(target_os = "linux")]
    cpu::set_realtime_priority(50)?;

    // Run trading loop...
    Ok(())
}
```

### Position Tracking

```rust
use bog_core::core::Position;

let position = Position::new();

// Atomic updates from multiple threads
position.update_quantity(100_000_000);  // +0.1 BTC

// Lock-free reads
let qty = position.get_quantity();
let pnl = position.get_realized_pnl();
let trades = position.get_trade_count();
```

## Safety & Testing

- **493 passing tests** across unit, integration, and property-based tests
- **Zero unwrap() in production code** - All error handling via `Result<T>`
- **Type-safe state machines** - Invalid state transitions prevented at compile-time
- **Overflow protection** - Checked arithmetic on all position updates
- **Stale data detection** - Automatic halt on data >5s old

## Performance Tuning

### CPU Affinity

Pin the trading thread to an isolated CPU core:

```bash
# Isolate cores 2-3 (add to kernel params)
isolcpus=2,3

# Run bog on core 3
bog-simple-spread-simulated --cpu-core 3
```

### Real-time Priority

```bash
# Set SCHED_FIFO priority (requires CAP_SYS_NICE)
bog-simple-spread-simulated --realtime
```

### Huge Pages (Linux)

```bash
# Allocate 128MB huge pages
echo 64 > /proc/sys/vm/nr_hugepages

# Mount hugetlbfs
mount -t hugetlbfs none /mnt/huge
```

## Documentation

- **[API Documentation](https://docs.rs/bog-core)** - Full API reference
- **[Architecture Guide](../docs/architecture/system-design.md)** - Zero-overhead design
- **[State Machines](../docs/architecture/STATE_MACHINES.md)** - Typestate patterns
- **[Performance Benchmarks](../docs/benchmarks/LATEST.md)** - Latest results
- **[Production Guide](../docs/deployment/PRODUCTION_READINESS.md)** - Deployment checklist

## Contributing

See [CONTRIBUTING.md](../CONTRIBUTING.md) for guidelines.

## License

MIT - See [LICENSE](../LICENSE) for details.
