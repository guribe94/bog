# System Design Architecture

## Overview

bog is a high-frequency trading (HFT) engine designed for sub-microsecond latency (<1μs tick-to-trade). The architecture prioritizes **zero-overhead abstractions**, **compile-time optimization**, and **cache efficiency** to achieve institutional-grade performance.

## Design Principles

### 1. Zero-Cost Abstractions

**Principle**: Abstraction should have zero runtime cost.

Every abstraction in bog compiles down to optimal machine code:
- Generic traits → Monomorphization (no vtables)
- Zero-sized types → Compile-time constants (0 bytes)
- Inline functions → Direct code insertion
- Const generics → Compile-time evaluation

**Anti-patterns avoided**:
-  `Box<dyn Trait>` - Virtual dispatch adds 50-200ns per call
-  `Arc<Mutex<T>>` - Lock contention adds microseconds
-  Heap allocations - malloc/free adds hundreds of nanoseconds
-  Runtime configuration - TOML parsing adds milliseconds

### 2. Compile-Time Configuration

**Principle**: All configuration happens at compile time.

```
Runtime Config (OLD)              Compile-Time Config (NEW)
              
spread = 10bps (TOML)              cargo build --features spread-10bps
→ Parse TOML (1ms)                 → Const evaluation (0ns)
→ Validate (100μs)                 → Type checking at build time
→ Store in struct (40 bytes)       → Zero-sized type (0 bytes)
→ Load on hot path (2ns)           → Inlined constant (0ns)
```

**Trade-off**: Flexibility for performance. Need different configs? Build different binaries.

### 3. Cache-First Design

**Principle**: Optimize for L1 cache hits (4 cycles = ~1ns).

```
L1 cache:    64KB,  4 cycles  (~1ns)
L2 cache:   512KB, 12 cycles  (~3ns)
L3 cache:    8MB,  40 cycles  (~10ns)
RAM:       32GB+, 200 cycles  (~50ns)
```

**Strategies**:
- **Cache-line alignment**: Position is 64 bytes (exactly 1 cache line)
- **Hot data locality**: Most-accessed fields at start of struct
- **False sharing prevention**: Each thread owns its cache lines
- **Minimized working set**: Total hot path < 32KB (fits in L1)

## Core Architecture

### High-Level Flow

```

   Huginn       Market data producer
 (Shared Mem)   /dev/shm/hg_m{market_id}

        POSIX shared memory (IPC)
       ↓

         Engine<S, E>                      
    
   1. Read market tick (Huginn SHM)     <-- ~5ns
   2. Generate signal (Strategy S)      <-- ~10ns
   3. Execute order (Executor E)        <-- ~10ns
   4. Update position (Position)        <-- ~2ns
    
                                           
  Hot data (64 bytes, 1 cache line):      
   position: Position                   
   last_market: MarketState             
   tick_count: u64                      
   signal_count: u64                    

       
       ↓

  Fills         Trade confirmations
 (ArrayQueue)   Lock-free bounded queue

```

### Component Architecture

```
bog workspace
 bog-core          Core zero-overhead types
    types.rs      Position, OrderId, Signal (cache-aligned)
    fixed_point.rs   u64 fixed-point arithmetic (9 decimals)
    huginn_shm.rs    POSIX shared memory reader
    engine/
        generic.rs   Engine<S: Strategy, E: Executor>

 bog-strategies    Zero-sized strategy implementations
    simple_spread.rs    SimpleSpread (0 bytes)
    inventory_based.rs  InventoryBased (0 bytes, stub)

 bog-bins          Binary combinations
     simple-spread-simulated.rs
     simple-spread-production.rs
```

## Zero-Overhead Type System

### Strategy Pattern

```rust
// Trait (compiles to static dispatch)
pub trait Strategy {
    fn calculate(
        &mut self,
        snapshot: &MarketSnapshot,
    ) -> Option<Signal>;
}

// Implementation (zero-sized type)
pub struct SimpleSpread;

impl SimpleSpread {
    const SPREAD_BPS: u64 = 10;  // Compile-time constant
    const SIZE: i64 = 10_000_000; // 0.01 BTC in fixed-point
}

impl Strategy for SimpleSpread {
    #[inline(always)]  // Force inlining
    fn generate_signal(&self, market: &MarketState, position: &Position)
        -> Option<Signal>
    {
        // All branches const-folded by compiler
        let spread = Self::SPREAD_BPS;
        let size = Self::SIZE;
        // ... logic here
    }
}
```

**Size verification**:
```rust
assert_eq!(std::mem::size_of::<SimpleSpread>(), 0);
//  Passes: 0 bytes
```

**Optimization**: LLVM inlines the entire function and const-folds the spread/size constants.

### Executor Pattern

```rust
pub trait Executor {
    fn submit_order(&mut self, signal: &Signal) -> OrderId;
    fn poll_fills(&mut self) -> Vec<Fill>;
}

// Simulated executor (for backtesting)
pub struct SimulatedExecutor {
    orders: HashMap<OrderId, Order>,
    pending_fills: ArrayQueue<Fill>,  // Bounded to 1024
    mode: ExecutionMode,
}

// Production executor (for live trading)
pub struct ProductionExecutor {
    exchange_client: ExchangeClient,
    order_cache: HashMap<OrderId, Order>,
}
```

**Polymorphism**: Compile-time via generics, not runtime via trait objects.

### Engine Monomorphization

```rust
pub struct Engine<S: Strategy, E: Executor> {
    strategy: S,        // 0 bytes (ZST)
    executor: E,        // Varies (SimulatedExecutor ~200 bytes)
    position: Position, // 64 bytes (cache-aligned)
    // ... other fields
}

impl<S: Strategy, E: Executor> Engine<S, E> {
    #[inline(always)]
    pub fn process_tick(&mut self, market: MarketState) {
        self.tick_count += 1;

        // 1. Generate signal (inlined, const-folded)
        if let Some(signal) = self.strategy.generate_signal(&market, &self.position) {
            self.signal_count += 1;

            // 2. Submit order (inlined)
            let order_id = self.executor.submit_order(&signal);

            // 3. Poll for fills (inlined)
            let fills = self.executor.poll_fills();

            // 4. Update position (atomic, lock-free)
            for fill in fills {
                self.position.update_quantity_checked(fill.quantity).ok();
                self.position.update_realized_pnl_checked(fill.pnl).ok();
            }
        }
    }
}
```

**Binary generation**:
```rust
// bog-bins/src/simple-spread-simulated.rs
type MyEngine = Engine<SimpleSpread, SimulatedExecutor>;

fn main() {
    let mut engine = MyEngine::new(SimpleSpread, SimulatedExecutor::new());
    // ... run loop
}
```

**Result**: Each binary is monomorphized for its specific Strategy+Executor combination. No runtime dispatch.

## Shared Memory IPC

### Huginn Integration

```
Producer: Huginn (market data aggregator)
 Writes to POSIX shared memory
 Path: /dev/shm/hg_m{market_id}
 Size: 4KB (single page)
 Update rate: ~1000 Hz (every 1ms)

Consumer: bog Engine
 Memory-maps SHM region
 Reads MarketState struct (lockless)
 No system calls (zero-copy)
 Latency: ~5ns (memory read)
```

### Memory Layout

```
/dev/shm/hg_m1 (4096 bytes)

 MarketSnapshot (512 bytes)              
  market_id: u64                      
  sequence: u64                       
  exchange_timestamp_ns: u64          
  best_bid_price: u64 (fixed-point)  
  best_bid_size: u64                  
  best_ask_price: u64                 
  best_ask_size: u64                  
  bid_prices: [u64; 10] (depth)      
  bid_sizes: [u64; 10]               
  ask_prices: [u64; 10]              
  ask_sizes: [u64; 10]               

 Padding (3584 bytes)                    

```

**Synchronization**: Lock-free reads via atomic sequence number protocol:

```rust
loop {
    let seq_before = market.seq.load(Ordering::Acquire);
    if seq_before % 2 != 0 {
        // Writer is updating, spin
        continue;
    }

    // Read data
    let data = market.clone();

    let seq_after = market.seq.load(Ordering::Acquire);
    if seq_before == seq_after {
        // Consistent read
        return data;
    }
    // Retry
}
```

**Advantage over pipes/sockets**:
- No system calls (mmap already done)
- No context switches
- No memory copies
- Latency: ~5ns vs ~1μs (pipe) or ~10μs (TCP)

## Cache-Line Alignment

### Position Struct

```rust
#[repr(C, align(64))]  // Force 64-byte alignment
pub struct Position {
    // HOT FIELDS (frequently accessed in tight loops)
    pub quantity: AtomicI64,      // Offset 0-7   (most accessed)
    pub realized_pnl: AtomicI64,  // Offset 8-15
    pub daily_pnl: AtomicI64,     // Offset 16-23
    pub trade_count: AtomicU32,   // Offset 24-27
    _pad1: [u8; 4],               // Offset 28-31 (padding)

    // COLD FIELDS (statistics, less critical)
    pub last_update_ns: AtomicI64,   // Offset 32-39
    pub max_quantity: AtomicI64,     // Offset 40-47
    pub min_quantity: AtomicI64,     // Offset 48-55
    _pad2: [u8; 8],                  // Offset 56-63 (padding)
}
```

**Size verification**:
```rust
assert_eq!(std::mem::size_of::<Position>(), 64);
assert_eq!(std::mem::align_of::<Position>(), 64);
```

**Why 64 bytes?**
- x86_64 cache line size: 64 bytes
- ARM64 cache line size: 64 bytes (M-series, Graviton)
- Exactly fits one cache line
- No false sharing between threads

### False Sharing Prevention

```
BAD: Two threads updating adjacent fields

 Cache Line (64 bytes)              
             
 Thread A  Thread B              
 position_aposition_b            
             

     ↓            ↓
  Store A      Store B
     → Cache line invalidation (50ns penalty)
```

```
GOOD: Separate cache lines

 Cache Line 0                       
                        
 Thread A                         
 position_a                       
                        



 Cache Line 1                       
                        
 Thread B                         
 position_b                       
                        

     ↓            ↓
  Store A      Store B
     → No invalidation (independent lines)
```

## Fixed-Point Arithmetic

### Rationale

**Why not floating-point (f64)?**
- Non-deterministic (rounding varies by CPU)
- Precision loss at large magnitudes
- NaN/Infinity edge cases
- Slower (SSE3 required for performance)

**Why not Decimal crate?**
- Heap allocations (malloc on overflow)
- 128-bit representation (oversized)
- Arithmetic overhead (~10x slower than u64)

**Why u64 fixed-point?**
- Deterministic (bitwise identical results)
- Fast (single CPU instruction)
- Exact (no rounding within 9 decimals)
- Small (8 bytes vs 16 bytes for Decimal)

### Implementation

```rust
pub mod fixed_point {
    pub const SCALE: i64 = 1_000_000_000;  // 9 decimal places

    #[inline(always)]
    pub fn from_f64_checked(value: f64) -> Result<i64, ConversionError> {
        if value.is_nan() {
            return Err(ConversionError::NotANumber);
        }
        if value.is_infinite() {
            return Err(ConversionError::Infinite { positive: value > 0.0 });
        }

        const MAX_SAFE: f64 = (i64::MAX / SCALE) as f64;
        const MIN_SAFE: f64 = (i64::MIN / SCALE) as f64;

        if value > MAX_SAFE || value < MIN_SAFE {
            return Err(ConversionError::OutOfRange { value });
        }

        Ok((value * SCALE as f64) as i64)
    }

    #[inline(always)]
    pub fn to_f64(fixed: i64) -> f64 {
        (fixed as f64) / (SCALE as f64)
    }
}
```

**Precision**:
```
9 decimal places
 0.000000001 (smallest unit)
 Range: ±9.2 billion (i64::MIN to i64::MAX)

Examples:
 Price: $50,123.456789123 → 50123456789123 (exact)
 Size:  0.123456789 BTC   → 123456789      (exact)
 PnL:   $-1,234.567890    → -1234567890000 (exact)
```

### Arithmetic Operations

```rust
// Addition (checked)
let a = fixed_point::from_f64_checked(50_000.0)?;  // $50k
let b = fixed_point::from_f64_checked(100.0)?;     // $100
let sum = a.checked_add(b).ok_or(OverflowError)?;  // $50,100

// Multiplication (requires scaling)
let price = fixed_point::from_f64_checked(50_000.0)?;  // $50k per BTC
let size = fixed_point::from_f64_checked(0.1)?;        // 0.1 BTC

// Naive multiply would be wrong:
// price * size = (50000 * 1e9) * (0.1 * 1e9) = 5e21 (overflow!)

// Correct: scale down after multiply
let notional = ((price as i128) * (size as i128) / (SCALE as i128)) as i64;
// = 5000 * 1e9 = $5,000 (correct)
```

**Note**: For complex math, use 128-bit intermediates to prevent overflow.

## Atomic Operations

### Lock-Free Position Updates

```rust
impl Position {
    pub fn update_quantity_checked(&self, delta: i64) -> Result<i64, OverflowError> {
        let old = self.quantity.load(Ordering::Acquire);
        let new = old.checked_add(delta)
            .ok_or(OverflowError::QuantityOverflow { old, delta })?;
        self.quantity.store(new, Ordering::Release);
        Ok(new)
    }
}
```

**Memory ordering**:
- `Acquire`: Prevents reordering of subsequent reads before this load
- `Release`: Prevents reordering of previous writes after this store
- Ensures visibility across threads
- No locks, no contention

**Compare-and-swap** (for concurrent updates):
```rust
// Example: Multiple threads updating same position (not recommended)
loop {
    let old = self.quantity.load(Ordering::Acquire);
    let new = old.checked_add(delta)?;

    if self.quantity.compare_exchange_weak(
        old,
        new,
        Ordering::AcqRel,
        Ordering::Acquire,
    ).is_ok() {
        return Ok(new);
    }
    // Retry on contention
}
```

**In practice**: bog is single-threaded per market, so CAS is unnecessary. Simple load/store suffices.

## Bounded Collections

### ArrayQueue for Backpressure

```rust
use crossbeam::queue::ArrayQueue;

pub struct SimulatedExecutor {
    pending_fills: Arc<ArrayQueue<Fill>>,  // MAX_PENDING_FILLS = 1024
    dropped_fills: u64,
}

impl SimulatedExecutor {
    pub fn add_fill(&mut self, fill: Fill) {
        if let Err(returned_fill) = self.pending_fills.push(fill) {
            self.dropped_fills += 1;
            warn!("Fill queue full, dropping oldest fill");

            // Drop oldest to make room
            if let Some(_oldest) = self.pending_fills.pop() {
                self.pending_fills.push(returned_fill).ok();
            }
        }
    }
}
```

**Why bounded?**
- Prevents unbounded memory growth (OOM risk)
- Early warning of processing lag
- Graceful degradation under load

**Why ArrayQueue?**
- Lock-free MPMC queue
- Constant-time operations
- No heap allocations (pre-allocated array)
- Cache-friendly (contiguous memory)

## Performance Budget

### Latency Breakdown

Target: <1μs (1000ns) tick-to-trade

| Component | Budget | Measured | Status |
|-----------|--------|----------|--------|
| Huginn SHM read | 10ns | ~5ns |  50% under |
| Signal generation | 100ns | ~17.28ns |  83% under |
| Order execution | 500ns | ~10ns |  98% under |
| Position update | 20ns | ~2ns |  90% under |
| Overflow checks | 10ns | ~2ns |  80% under |
| **Total hot path** | **640ns** | **~70.79ns** |  **89% under** |

**Slack**: 973ns remaining for:
- Exchange network latency (~100μs)
- Queueing at exchange (~50μs)
- Future features

### Microbenchmark Results

From `bog-core/benches/engine_bench.rs`:

```
engine/tick_processing  time: [68.234 ns 70.791 ns 72.012 ns]
engine/signal_gen       time: [16.123 ns  17.284 ns  18.456 ns]
engine/order_submit     time: [8.456 ns  8.891 ns  9.012 ns]
position/update         time: [1.789 ns  1.823 ns  1.901 ns]
```

**Comparison to alternatives**:

| Architecture | Latency | Notes |
|--------------|---------|-------|
| bog (const generic) | ~70.79ns | This implementation |
| Dynamic dispatch | ~150ns | Box<dyn Strategy> |
| Python (NumPy) | ~1μs | Interpreted overhead |
| Java (HotSpot) | ~500ns | JIT + GC pauses |

## Scalability

### Single-Threaded Design

**Rationale**: Each market runs in its own process.

```
CPU Pinning
 Market 1 (BTC-USD)  → Core 0
 Market 2 (ETH-USD)  → Core 1
 Market 3 (SOL-USD)  → Core 2
 Metrics server      → Core 3
```

**Why not multi-threaded?**
- No contention (each process is independent)
- No synchronization overhead
- Better CPU cache utilization
- Easier to reason about correctness

### Horizontal Scaling

```
                     
                        Huginn    
                      (market data)
                     
                            
           
                                           
           ↓                ↓                ↓
      
       bog-1          bog-2          bog-3      
      BTC-USD        ETH-USD        SOL-USD     
      (Core 0)       (Core 1)       (Core 2)    
      
                                           
           
                            ↓
                    
                      Aggregator 
                      (positions) 
                    
```

**Capacity**: Scales linearly with CPU cores (e.g., 64-core Threadripper can run 60+ markets).

## Monitoring Architecture

### Prometheus Metrics

```rust
use prometheus::{IntCounter, IntGauge, register_int_counter, register_int_gauge};

pub struct EngineMetrics {
    pub ticks_processed: IntCounter,
    pub signals_generated: IntCounter,
    pub orders_submitted: IntCounter,
    pub fills_received: IntCounter,
    pub overflow_errors: IntCounter,     // Added in Phase 1
    pub saturated_ops: IntCounter,       // Added in Phase 1
    pub queue_depth: IntGauge,
    pub position_quantity: IntGauge,
}
```

**Endpoint**: `http://localhost:9090/metrics`

**Scrape interval**: 1 second

### Alerting

```yaml
groups:
  - name: bog
    interval: 30s
    rules:
      - alert: OverflowDetected
        expr: rate(bog_overflow_errors_total[5m]) > 0
        severity: critical

      - alert: HighQueueDepth
        expr: bog_queue_depth > 100
        severity: warning
```

## Deployment Model

### Binary Structure

```
bog-bins/
 simple-spread-simulated     # Backtesting
    Strategy: SimpleSpread
    Executor: SimulatedExecutor

 simple-spread-production    # Live trading
     Strategy: SimpleSpread
     Executor: ProductionExecutor
```

### Build Profiles

```toml
# Cargo.toml
[profile.release]
opt-level = 3        # Maximum optimization
lto = "fat"          # Link-time optimization (cross-crate inlining)
codegen-units = 1    # Single codegen unit (better optimization)
panic = "abort"      # No unwinding (smaller binary, faster)
strip = false        # Keep symbols for profiling
```

**Build command**:
```bash
cargo build --release --features spread-10bps,size-medium,min-spread-1bps
```

### Runtime Requirements

**Dependencies**:
- Huginn running (for market data)
- POSIX shared memory at `/dev/shm/hg_m*`
- CPU pinning available (Linux: `taskset`, macOS: `thread_policy_set`)
- Real-time priority (Linux: `SCHED_FIFO`, macOS: `THREAD_TIME_CONSTRAINT_POLICY`)

**Resource usage** (per market):
- Memory: ~10MB (mostly mmap for SHM)
- CPU: 100% of one core (pinned)
- Disk: None (runs entirely in memory)

## Testing Strategy

### Unit Tests

Standard Rust unit tests for individual components:
```bash
cargo test --package bog-core
```

### Property-Based Tests

Verifies mathematical invariants with random inputs:
```bash
cargo test --package bog-core fixed_point_proptest
```

17 property tests × 100 iterations = 1700+ test cases

### Fuzz Tests

Finds edge cases with arbitrary inputs:
```bash
cargo +nightly fuzz run fuzz_fixed_point_conversion -- -max_total_time=300
```

Expected: 100k-500k executions/second

### Benchmarks

Measures performance of hot paths:
```bash
cargo bench --package bog-core
```

Generates HTML reports in `target/criterion/`

### Integration Tests

End-to-end testing with simulated market data:
```bash
cargo run --bin simple-spread-simulated
```

## References

- [Overflow Handling Architecture](./overflow-handling.md)
- [Latency Budget](../benchmarks/latency-budget.md)
- [Failure Modes](../deployment/failure-modes.md)
- [Huginn Integration](../HUGINN_INTEGRATION_GUIDE.md)
