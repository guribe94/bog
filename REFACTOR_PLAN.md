# Bog Trading Bot - HFT Refactor Plan

## Target: Sub-Microsecond Tick-to-Trade Latency

### Design Principles

1. **Zero Runtime Overhead**: Everything possible at compile time
2. **Zero Allocations**: No heap allocations in hot path
3. **Zero Dynamic Dispatch**: Static dispatch everywhere
4. **Lock-Free**: Atomic operations only, no mutexes
5. **Cache-Optimized**: 64-byte alignment, false sharing prevention
6. **Separate Binaries**: Each strategy is a separate optimized binary

### Architecture Overview

```
bog/
├── Cargo.toml                 # Workspace with binary targets
├── bin/
│   ├── bog-simple-spread.rs   # Separate optimized binary
│   ├── bog-inventory.rs       # Separate optimized binary
│   └── bog-benchmark.rs       # Performance testing binary
├── src/
│   ├── lib.rs                 # Minimal library exports
│   ├── config.rs              # Compile-time constants via features
│   ├── core/                  # Zero-overhead core types
│   │   ├── mod.rs
│   │   ├── types.rs           # Copy types, no allocations
│   │   ├── order.rs           # u128-based OrderId
│   │   └── state.rs           # Cache-aligned state
│   ├── feed/                  # Huginn integration
│   │   ├── mod.rs
│   │   └── consumer.rs        # Zero-copy wrapper
│   ├── book/                  # OrderBook (stub)
│   │   ├── mod.rs
│   │   └── stub.rs            # Placeholder
│   ├── strategies/            # Strategy implementations
│   │   ├── mod.rs
│   │   ├── simple_spread.rs   # Static strategy
│   │   └── inventory.rs       # Static strategy
│   ├── execution/             # Execution engines
│   │   ├── mod.rs
│   │   ├── simulated.rs       # Object pool based
│   │   └── lighter.rs         # Stub
│   ├── risk/                  # Risk management
│   │   ├── mod.rs
│   │   └── limits.rs          # Const-based validation
│   ├── engine/                # Core engine
│   │   └── mod.rs             # Generic monomorphized engine
│   └── perf/                  # Performance utilities
│       ├── mod.rs
│       ├── cpu.rs             # CPU pinning
│       ├── alloc.rs           # Object pools
│       └── metrics.rs         # Lock-free counters
├── benches/
│   ├── hot_path.rs
│   ├── strategy.rs
│   └── full_loop.rs
└── tests/
    ├── integration/
    └── property/

```

### Phase 1: Core Types (Week 1, Days 1-2)

**Objectives:**
- Zero-allocation types
- Cache-aligned structures
- Copy semantics everywhere possible

**Key Changes:**

1. **OrderId: String → u128**
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct OrderId(pub u128);
```

2. **Signal: Enum → Inline Struct**
```rust
#[derive(Clone, Copy)]
#[repr(C, align(64))]
pub struct Signal {
    pub action: SignalAction,  // 1 byte enum
    pub bid_price: u64,         // Fixed-point
    pub ask_price: u64,
    pub size: u64,
    _padding: [u8; 39],         // Pad to 64 bytes
}
```

3. **Position: Cache-Aligned**
```rust
#[repr(C, align(64))]
pub struct Position {
    pub quantity: i64,          // Fixed-point, signed
    pub cost_basis: i64,
    pub realized_pnl: i64,
    pub daily_pnl: i64,
    pub trade_count: u32,
    _padding: [u8; 28],
}
```

### Phase 2: Configuration (Week 1, Days 3-4)

**Objectives:**
- Cargo features for all configuration
- Const generics for strategy parameters
- Zero runtime parsing

**Implementation:**

```toml
# Cargo.toml
[features]
default = ["simulated", "simple-spread"]

# Execution modes (mutually exclusive)
simulated = []
live = []

# Strategies (mutually exclusive)
simple-spread = []
inventory = []

# Market configuration
market-1 = []
market-2 = []

# Risk profiles
conservative = []
aggressive = []
```

```rust
// src/config.rs
#[cfg(feature = "simple-spread")]
pub const SPREAD_BPS: f64 = 10.0;

#[cfg(feature = "market-1")]
pub const MARKET_ID: u64 = 1_000_001;

#[cfg(feature = "conservative")]
pub const MAX_POSITION: i64 = 1_000_000_000; // 1.0 BTC in fixed-point
```

### Phase 3: Engine Redesign (Week 1, Days 5-7)

**Objectives:**
- Generic monomorphized engine
- Zero allocations
- Lock-free everywhere

**Key Design:**

```rust
pub struct Engine<S: Strategy> {
    strategy: S,                           // Zero-sized if const generic
    position: CacheAligned<Position>,
    metrics: CacheAligned<Metrics>,
}

impl<S: Strategy> Engine<S> {
    #[inline(always)]
    pub fn process_snapshot(&mut self, snapshot: &MarketSnapshot) -> Option<Signal> {
        // Zero allocations
        // ~100-200ns target
    }
}
```

### Phase 4: Strategy Implementations (Week 2, Days 1-3)

**Objectives:**
- Const generic parameters
- All calculations inline
- SIMD where applicable

**Example:**

```rust
pub struct SimpleSpread<const SPREAD_BPS: u32, const ORDER_SIZE: u64>;

impl<const SPREAD_BPS: u32, const ORDER_SIZE: u64> Strategy for SimpleSpread<SPREAD_BPS, ORDER_SIZE> {
    #[inline(always)]
    fn on_update(&mut self, mid_price: u64) -> Option<Signal> {
        // Everything const-folded at compile time
        let half_spread = (mid_price * SPREAD_BPS as u64) / 20_000;
        Some(Signal {
            action: SignalAction::QuoteBoth,
            bid_price: mid_price - half_spread,
            ask_price: mid_price + half_spread,
            size: ORDER_SIZE,
            _padding: [0; 39],
        })
    }
}
```

### Phase 5: Execution Layer (Week 2, Days 4-5)

**Objectives:**
- Object pools for Fill/Order
- Lock-free queues
- Zero-copy where possible

**Implementation:**

```rust
use crossbeam::queue::ArrayQueue;

pub struct SimulatedExecutor {
    fill_pool: ArrayQueue<Fill>,           // Lock-free, pre-allocated
    order_pool: ArrayQueue<OrderSlot>,
}

// Pre-allocate on startup
impl SimulatedExecutor {
    pub fn new() -> Self {
        let mut fill_pool = ArrayQueue::new(1024);
        // Pre-allocate fills
        for _ in 0..1024 {
            fill_pool.push(Fill::default()).ok();
        }
        Self { fill_pool, order_pool: ArrayQueue::new(256) }
    }
}
```

### Phase 6: Risk Management (Week 2, Days 6-7)

**Objectives:**
- Const-based limits
- Branch-free validation where possible
- Atomic counters

**Implementation:**

```rust
#[repr(C, align(64))]
pub struct RiskState {
    position: AtomicI64,
    daily_pnl: AtomicI64,
    order_count: AtomicU32,
}

impl RiskState {
    #[inline(always)]
    pub fn can_buy(&self, size: u64) -> bool {
        let current_pos = self.position.load(Ordering::Relaxed);
        let new_pos = current_pos + size as i64;
        new_pos <= MAX_POSITION  // Const from config
    }
}
```

### Phase 7: Performance Infrastructure (Week 3, Days 1-2)

**Objectives:**
- CPU pinning
- Huge pages
- Lock-free metrics

**Implementation:**

```rust
// src/perf/cpu.rs
pub fn pin_to_core(core: usize) {
    use core_affinity::CoreId;
    core_affinity::set_for_current(CoreId { id: core });
}

pub fn set_thread_priority() {
    use libc::{sched_param, sched_setscheduler, SCHED_FIFO};
    unsafe {
        let param = sched_param { sched_priority: 99 };
        sched_setscheduler(0, SCHED_FIFO, &param);
    }
}

// src/perf/metrics.rs (lock-free)
#[repr(C, align(64))]
pub struct Metrics {
    updates_processed: AtomicU64,
    signals_generated: AtomicU64,
    orders_placed: AtomicU64,
    fills_received: AtomicU64,
    total_latency_ns: AtomicU64,
}
```

### Phase 8: Separate Binaries (Week 3, Days 3-4)

**Implementation:**

```rust
// bin/bog-simple-spread.rs
use bog::prelude::*;

#[global_allocator]
static ALLOC: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Pin to core 2
    bog::perf::pin_to_core(2);
    bog::perf::set_thread_priority();

    // Create engine with compile-time strategy
    let mut engine = Engine::<SimpleSpread<10, 100_000_000>>::new();

    // Connect to Huginn
    let mut feed = MarketFeed::connect(MARKET_ID)?;

    // Run (everything inlined and optimized)
    engine.run(&mut feed)?;

    Ok(())
}
```

### Phase 9: Testing & Benchmarking (Week 3, Days 5-7)

**Benchmarks:**

```rust
// benches/hot_path.rs
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn benchmark_full_tick_to_trade(c: &mut Criterion) {
    let mut engine = Engine::<SimpleSpread<10, 100_000_000>>::new();
    let snapshot = create_test_snapshot();

    c.bench_function("full_tick_to_trade", |b| {
        b.iter(|| {
            black_box(engine.process_snapshot(black_box(&snapshot)))
        });
    });
}

criterion_group!(benches, benchmark_full_tick_to_trade);
criterion_main!(benches);
```

**Target Metrics:**
- `process_snapshot`: <100ns
- `validate_risk`: <50ns
- `place_order`: <200ns
- **Total: <500ns** (leaves 500ns margin for network/exchange)

### Phase 10: CLI Design (Week 4, Day 1)

**Minimal CLI:**

```rust
// Only essential runtime parameters
#[derive(clap::Parser)]
struct Args {
    /// CPU core to pin to
    #[arg(long, default_value = "2")]
    cpu_core: usize,

    /// Enable metrics logging (adds ~10ns overhead)
    #[arg(long)]
    metrics: bool,
}
```

All other configuration via cargo features:
```bash
# Build for market 1, simple spread, simulated
cargo build --release \
    --features "market-1 simple-spread simulated" \
    --bin bog-simple-spread

# Run
./target/release/bog-simple-spread --cpu-core 2
```

### Performance Budget Breakdown

**Target: <1μs tick-to-trade**

| Component | Budget | Notes |
|-----------|--------|-------|
| Huginn read | 50-150ns | Already optimized |
| Parse snapshot | 20ns | Inline copy |
| Update orderbook stub | 30ns | Minimal state update |
| Strategy calculation | 100ns | All const-folded |
| Risk validation | 50ns | Branchless checks |
| Signal creation | 20ns | Stack allocation |
| Order placement | 200ns | Queue push |
| **Total** | **470-570ns** | ✅ Under 1μs |

### Memory Layout Strategy

**Cache Lines (64 bytes each):**

```
Line 0: Position state (hot, frequently accessed)
Line 1: Order state (medium frequency)
Line 2: Metrics (cold, async updates)
Line 3: Strategy state (read-only after init)
```

**False Sharing Prevention:**
- Each thread gets own cache-aligned state
- Atomic counters on separate cache lines
- No shared mutable state

### Zero-Allocation Guarantee

**Hot Path (main loop):**
- ✅ No Vec allocations
- ✅ No String allocations
- ✅ No HashMap allocations
- ✅ No Box allocations
- ✅ Signal on stack (64 bytes)
- ✅ Orders from pre-allocated pool

**Cold Path (initialization):**
- Allocate pools once at startup
- Initialize all data structures
- Never reallocate in main loop

### Build Configuration Examples

```bash
# Ultra-low latency, simple spread, market 1
cargo build --release \
    --features "simulated simple-spread market-1 conservative" \
    --bin bog-simple-spread

# Inventory strategy, aggressive risk, market 2
cargo build --release \
    --features "simulated inventory market-2 aggressive" \
    --bin bog-inventory

# Live trading (when ready)
cargo build --release \
    --features "live simple-spread market-1 conservative" \
    --bin bog-simple-spread
```

### Success Criteria

**Performance:**
- [ ] Main loop: <100ns per iteration
- [ ] Strategy: <100ns signal generation
- [ ] Risk: <50ns validation
- [ ] Total: <500ns tick-to-trade
- [ ] Zero allocations in hot path
- [ ] Zero dynamic dispatch

**Quality:**
- [ ] All tests passing
- [ ] Benchmarks showing <1μs
- [ ] No clippy warnings
- [ ] Full documentation
- [ ] CI/CD pipeline

**Functionality:**
- [ ] Simple spread strategy working
- [ ] Inventory strategy working
- [ ] Risk limits enforced
- [ ] Metrics collection (optional)
- [ ] Graceful shutdown

### Risk Mitigation

**Complexity Risk:**
- Start with simplest strategy (simple spread)
- Incremental feature addition
- Extensive benchmarking at each step

**Performance Risk:**
- Profile with perf/flamegraph at each stage
- Validate cache behavior with cachegrind
- Verify branch prediction with perf stat

**Correctness Risk:**
- Property-based testing with proptest
- Fuzz testing for edge cases
- Integration tests with real Huginn data

### Timeline

**Week 1:** Core types + Config + Engine (Days 1-7)
**Week 2:** Strategies + Execution + Risk (Days 8-14)
**Week 3:** Perf infrastructure + Binaries + Testing (Days 15-21)
**Week 4:** Polish + Benchmarks + Documentation (Days 22-28)

**Total: 4 weeks to production-ready HFT bot**

### Next Steps

1. Get approval on this plan
2. Back up current code
3. Start fresh in a new branch
4. Implement Phase 1 (Core Types)
5. Benchmark at every step
