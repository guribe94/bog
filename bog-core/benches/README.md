# Bog Benchmark Suite

Comprehensive performance benchmark suite for the bog HFT trading engine.

## Overview

**Framework**: Criterion.rs 0.5 with HTML reports
**Sample Size**: 10,000 iterations (100 for contention tests)
**Statistical Rigor**: 99% confidence (p < 0.01)
**Target**: <1μs tick-to-trade latency (measured: 70.79ns)

## Benchmark Files

### Core Benchmarks (Original 6)

#### engine_bench.rs
**Purpose**: Core engine and strategy performance + error paths
**Target**: <1μs complete tick-to-trade pipeline

**Benchmarks**:
- `tick_processing` - Engine.process_tick() overhead (target: <100ns)
- `strategy_calculation` - SimpleSpread.calculate() (target: <50ns)
- `risk_validation` - Signal validation against position (target: <50ns)
- `executor` - SimulatedExecutor.execute() (target: <200ns)
- `signal_creation` - Signal::quote_both() construction (target: <10ns)
- `position_operations` - Position getters (target: <5ns)
- `market_change_detection` - Early-exit optimization (target: <5ns)
- `varying_order_sizes` - Different order sizes (10M, 100M, 500M satoshis)
- `tick_to_trade_pipeline` - Complete end-to-end (target: <1000ns)
- **`stale_data_skip`** - Error path: stale data handling (NEW)
- **`invalid_market`** - Error path: crossed book handling (NEW)

**Why this matters**: This is the hot path - every nanosecond here multiplies across millions of ticks.

### depth_bench.rs
**Purpose**: Orderbook depth calculations
**Target**: <50ns per calculation

**Benchmarks**:
- `vwap_calculation` - Volume-weighted average price (3-5 levels)
- `imbalance_calculation` - Order book imbalance (5 and 10 levels)
- `liquidity_calculation` - Aggregate liquidity
- `mid_and_spread` - Mid price and spread computation

**Why this matters**: Orderbook analysis runs on every market update for strategy input.

### conversion_bench.rs
**Purpose**: Fixed-point type conversions
**Target**: <20ns per conversion

**Benchmarks**:
- `decimal_to_u64` - Decimal to fixed-point u64 (4 value ranges)
- `u64_to_decimal` - Fixed-point to Decimal (4 value ranges)
- `f64_to_u64` - Float to fixed-point
- `u64_to_f64` - Fixed-point to float
- `decimal_roundtrip` - Precision loss testing

**Why this matters**: Conversions happen on every order and fill. Heap allocations here kill performance.

### atomic_bench.rs
**Purpose**: Concurrent position operations
**Target**: <5ns per atomic operation

**Benchmarks**:
- `position_reads` - Atomic get operations (quantity, PnL, trade count)
- `position_updates_unchecked` - Unchecked atomic increment
- `position_updates_checked` - Overflow-checking variants (measure overhead)
- `orderid_generation` - Single, random, and batch (100) ID generation
- `position_contention` - 1, 2, and 4 thread contention testing

**Why this matters**: Position is shared state - contention here causes latency spikes.

### tls_overhead_bench.rs
**Purpose**: Thread-local storage overhead analysis
**Target**: <10ns TLS access

**Benchmarks**:
- `orderid/original` - Original timestamp implementation
- `orderid/optimized` - Timestamp-cached variant
- `orderid/tls_overhead_only` - Isolate TLS cost
- `orderid/instant_elapsed` - Instant::elapsed() cost
- `orderid/cell_operations` - Cell get/set overhead

**Why this matters**: Helps identify regression sources and optimize thread-local access patterns.

### resilience_bench.rs
**Purpose**: Market data resilience and gap handling
**Target**: <10ns per check

**Benchmarks**:
- `gap_detection` - Sequential checks, small gaps (9 messages), large gaps (1099 messages)
- `stale_data_breaker` - Freshness checks, mark_fresh(), mark_empty_poll()
- `health_monitoring` - Message reporting, empty polls, status checks
- `high_frequency_processing` - 1000, 10000, 100000 sequential ticks
- `gap_recovery_stress` - 100 gap/recovery cycles
- `wraparound_handling` - u64::MAX wraparound scenarios
- `stale_state_machine` - 1000 empty polls transition to offline

**Why this matters**: Resilience logic runs on every market update - must have zero overhead.

## Running Benchmarks

### Run All Benchmarks
```bash
cargo bench --package bog-core
```

### Run Specific Benchmark
```bash
cargo bench --package bog-core --bench engine_bench
cargo bench --package bog-core --bench depth_bench
cargo bench --package bog-core --bench conversion_bench
cargo bench --package bog-core --bench atomic_bench
cargo bench --package bog-core --bench tls_overhead_bench
cargo bench --package bog-core --bench resilience_bench
```

### Run Specific Test Within Benchmark
```bash
cargo bench --package bog-core --bench engine_bench tick_to_trade
```

### Save Results for Baseline Comparison
```bash
cargo bench --package bog-core 2>&1 | tee benchmark_results_$(date +%Y%m%d).txt
```

## Interpreting Results

### Good Performance
```
time:   [70.49 ns 70.79 ns 71.09 ns]
```
- Mean: 70.79ns
- Lower bound: 70.49ns (95% CI)
- Upper bound: 71.09ns (95% CI)
- **Status**: ✅ Well under 1μs target (14x faster)

### Performance Regression
```
change: [+25.1% +27.3% +29.5%] (p = 0.00 < 0.01)
Performance has regressed.
```
- Mean increased 27.3%
- High statistical confidence (p < 0.01)
- **Action**: Investigate recent changes, optimize

### High Variance
```
Found 96 outliers among 10000 measurements (0.96%)
  52 (0.52%) high mild
  44 (0.44%) high severe
```
- <1% outliers is normal (scheduler interrupts, cache misses)
- >5% outliers indicates instability
- **Action**: Check CPU pinning, reduce background load

## Performance Targets

| Component | Target | Measured | Status |
|-----------|--------|----------|--------|
| Tick-to-trade | <1000ns | 70.79ns | ✅ 14.1x under |
| Strategy calc | <50ns | 15.66ns | ✅ 3.2x under |
| Risk validation | <50ns | 2.12ns | ✅ 23.6x under |
| Executor | <200ns | ~86ns | ✅ 2.3x under |
| Orderbook VWAP | <50ns | ~30ns | ✅ 1.7x under |
| Position read | <5ns | ~2ns | ✅ 2.5x under |
| Gap detection | <10ns | <10ns | ✅ At target |

### Enhanced Benchmarks (New - Added 2025-11-21)

#### inventory_strategy_bench.rs
**Purpose**: InventoryBased strategy performance across inventory levels
**Target**: <50ns calculation (comparable to SimpleSpread)
**Tests**: 6 benchmarks (neutral, small +/-, large +/-, vs SimpleSpread)

#### circuit_breaker_bench.rs
**Purpose**: Risk circuit breaker overhead and state transitions
**Target**: <5ns check (normal), <20ns transition
**Tests**: 6 benchmarks (normal, spread detection, spike, liquidity, reset, violations)

#### multi_tick_bench.rs
**Purpose**: Realistic multi-tick scenarios and sustained performance
**Target**: <100μs for 1000 ticks
**Tests**: 6 benchmarks (100/1000 sequential, oscillating, trending, volatile, gaps)

#### order_fsm_bench.rs
**Purpose**: Typestate order lifecycle transition overhead
**Target**: <10ns per transition
**Tests**: 8 benchmarks (all state transitions + 100-fill stress test)

#### fill_processing_bench.rs
**Purpose**: Fill handling and position update performance
**Target**: <50ns per fill
**Tests**: 6 benchmarks (single, partial, batch, PnL, fees, round-trip)

#### throughput_bench.rs
**Purpose**: Maximum sustained throughput limits
**Target**: >10,000 ticks/second
**Tests**: 4 benchmarks (1k/5k/10k ticks, orders/sec, fills/sec, realistic)

#### reconciliation_bench.rs
**Purpose**: Position reconciliation overhead
**Target**: <100ns per reconciliation
**Tests**: 6 benchmarks (no drift, small/large drift, varying, config, on_fill)

## Coverage Status (Updated)

| Component | Benchmarked | Coverage | Status |
|-----------|-------------|----------|--------|
| Engine | ✅ Yes | Core + error paths | Complete |
| SimpleSpread | ✅ Yes | Full calculation | Complete |
| **InventoryBased** | ✅ Yes | **6 scenarios** | **NEW** |
| SimulatedExecutor | ✅ Yes | Basic execution | Complete |
| LighterExecutor | ❌ No | Stubbed | N/A |
| **Order FSM** | ✅ Yes | **All transitions** | **NEW** |
| **Circuit Breaker** | ✅ Yes | **Check + trip** | **NEW** |
| Position | ✅ Yes | Reads + updates + reconciliation | Enhanced |
| Orderbook | ✅ Yes | Depth calculations | Complete |
| Conversions | ✅ Yes | All types | Complete |
| Resilience | ✅ Yes | Gap + stale detection | Complete |
| **Fill Processing** | ✅ Yes | **Single + batch** | **NEW** |
| **Multi-tick** | ✅ Yes | **Realistic sequences** | **NEW** |
| **Throughput** | ✅ Yes | **System limits** | **NEW** |

## Summary Statistics

**Total Benchmark Files**: 13 (was 6, added 7)
**Total Individual Tests**: 67 (was 26, added 41)
**Coverage**: Comprehensive across all critical paths
**Last Expansion**: 2025-11-21

**New Additions**:
✅ InventoryBased strategy (6 tests)
✅ Circuit breaker operations (6 tests)
✅ Multi-tick scenarios (6 tests)
✅ Order FSM transitions (8 tests)
✅ Fill processing (6 tests)
✅ Error path overhead (2 tests)
✅ Throughput limits (4 tests)
✅ Position reconciliation (6 tests)

**Total new tests**: 44 added (including 2 error paths in engine_bench)

## HTML Reports

After running benchmarks, view HTML reports at:
```
target/criterion/*/report/index.html
```

Open in browser for interactive charts and statistical analysis.

## Baseline Establishment

Current baseline: 2025-11-21 (post-documentation-refactor)

See [BASELINE.md](BASELINE.md) for reference numbers.

## Contributing

When adding new benchmarks:
1. Use Criterion.rs with `significance_level(0.01)`
2. Use `sample_size(10000)` for micro-benchmarks
3. Use `sample_size(100)` for heavy operations
4. Always use `black_box()` to prevent over-optimization
5. Add clear comments explaining what's measured and why
6. Document expected performance targets
7. Update this README with new benchmark
8. Add results to BASELINE.md

---

**Last Updated**: 2025-11-21
**Maintained by**: Bog Team
