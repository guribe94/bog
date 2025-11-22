# Benchmark Baseline Numbers

**Established**: 2025-11-21 (post-documentation-refactor)
**Hardware**: MacBook (Darwin 24.6.0)
**Rust Version**: 1.70+
**Benchmark Framework**: Criterion 0.5
**Sample Size**: 10,000 iterations
**Confidence**: 99% (p < 0.01)

## Purpose

This baseline serves as reference for detecting performance regressions.
Any future benchmark run showing >10% degradation should be investigated.

## Tick-to-Trade Pipeline

**Benchmark**: `tick_to_trade_pipeline/complete_pipeline`

| Metric | Value |
|--------|-------|
| Mean | 70.79ns |
| Lower bound (95% CI) | 70.49ns |
| Upper bound (95% CI) | 71.09ns |
| Std dev | ~0.6ns |
| Target | 1,000ns |
| Headroom | 929ns (92.9%) |

**Regression threshold**: >77ns (>10% slower)

## Component Benchmarks

### Engine Operations

| Benchmark | Mean | Lower | Upper | Target | Threshold |
|-----------|------|-------|-------|--------|-----------|
| Engine tick processing | 2.17ns | 2.17ns | 2.17ns | <100ns | >2.39ns |
| Market change detection | 2.18ns | 2.17ns | 2.18ns | <5ns | >2.40ns |

### Strategy

| Benchmark | Mean | Lower | Upper | Target | Threshold |
|-----------|------|-------|-------|--------|-----------|
| SimpleSpread.calculate() | 15.66ns | 15.64ns | 15.67ns | <50ns | >17.23ns |

### Risk & Execution

| Benchmark | Mean | Lower | Upper | Target | Threshold |
|-----------|------|-------|-------|--------|-----------|
| Risk validation | 2.12ns | 2.12ns | 2.12ns | <50ns | >2.33ns |
| SimulatedExecutor.execute() | 86.44ns | 86.24ns | 86.68ns | <200ns | >95.08ns |

### Signal & Position

| Benchmark | Mean | Lower | Upper | Target | Threshold |
|-----------|------|-------|-------|--------|-----------|
| Signal creation | 13.24ns | 13.20ns | 13.29ns | <10ns | >14.56ns |
| Position.quantity() | ~2ns | - | - | <5ns | >2.2ns |
| Position atomic update | ~2ns | - | - | <5ns | >2.2ns |

## Orderbook Depth

| Benchmark | Mean | Lower | Upper | Target | Threshold |
|-----------|------|-------|-------|--------|-----------|
| VWAP (5 levels, bid) | 12.28ns | - | - | <20ns | >13.51ns |
| VWAP (3 levels, ask) | 7.86ns | - | - | <15ns | >8.65ns |
| Imbalance (5 levels) | 9.75ns | - | - | <15ns | >10.73ns |
| Imbalance (10 levels) | 11.01ns | - | - | <20ns | >12.11ns |
| Liquidity (5 levels) | 8.39ns | - | - | <15ns | >9.23ns |
| Mid price | 1.87ns | - | - | <5ns | >2.06ns |
| Spread BPS | ~2ns | - | - | <5ns | >2.2ns |

## Type Conversions

| Benchmark | Mean | Target | Threshold |
|-----------|------|--------|-----------|
| Decimal→u64 (small) | ~15ns | <20ns | >16.5ns |
| Decimal→u64 (medium) | ~15ns | <20ns | >16.5ns |
| Decimal→u64 (large) | ~15ns | <20ns | >16.5ns |
| u64→Decimal (all) | ~25ns | <30ns | >27.5ns |
| f64→u64 | ~10ns | <15ns | >11ns |
| u64→f64 | ~8ns | <15ns | >8.8ns |
| Decimal roundtrip | ~40ns | <50ns | >44ns |

## Atomic Operations

| Benchmark | Mean | Target | Threshold |
|-----------|------|--------|-----------|
| Position read (quantity) | ~2ns | <5ns | >2.2ns |
| Position read (PnL) | ~2ns | <5ns | >2.2ns |
| Position update (unchecked) | ~2ns | <5ns | >2.2ns |
| Position update (checked) | ~4ns | <10ns | >4.4ns |
| OrderId generation (single) | ~20ns | <30ns | >22ns |
| OrderId generation (batch 100) | ~15ns avg | <25ns | >16.5ns |

### Contention

| Threads | Mean | Threshold |
|---------|------|-----------|
| 1 thread (baseline) | ~2ns | >2.2ns |
| 2 threads | ~15ns | >16.5ns |
| 4 threads | ~45ns | >49.5ns |

## Resilience Components

| Benchmark | Mean | Target | Threshold |
|-----------|------|--------|-----------|
| Gap detection (sequential) | <5ns | <10ns | >5.5ns |
| Gap detection (small gap 9) | <10ns | <15ns | >11ns |
| Gap detection (large gap 1099) | <10ns | <15ns | >11ns |
| Stale data check | <5ns | <10ns | >5.5ns |
| Health monitoring | <10ns | <15ns | >11ns |
| Wraparound handling | <5ns | <10ns | >5.5ns |

### High-Frequency Processing

| Messages | Mean | Threshold |
|----------|------|-----------|
| 1,000 ticks | ~5μs | >5.5μs |
| 10,000 ticks | ~50μs | >55μs |
| 100,000 ticks | ~500μs | >550μs |

## Regression Detection

**How to use this baseline:**

1. Run benchmarks: `cargo bench --package bog-core`
2. Compare results to this baseline
3. If any component >10% slower → investigate
4. Common causes:
   - Code changes introducing allocations
   - Cache alignment issues
   - Lock contention
   - Compiler optimization regression

**Example**:
```
Baseline: SimpleSpread.calculate() = 15.66ns
New run: SimpleSpread.calculate() = 18.20ns
Difference: +16.2% → REGRESSION! Investigate.
```

## Hardware Specs

**CPU**: Apple M1 (8 cores, arm64)
**RAM**: 8 GB
**OS**: macOS Darwin 24.6.0
**Rust**: rustc 1.90.0 (1159e78c4 2025-09-14)
**Cargo**: 1.90.0 (840b83a10 2025-07-30)
**Compiler Flags**: `--release` (optimized + debuginfo)

**Notes**:
- Benchmarks run on laptop, not dedicated server
- Background processes may cause variance
- For production baseline, use isolated environment

## Next Baseline Update

Update this file when:
- Major refactoring completed
- New optimizations added
- Hardware changes
- Compiler version changes

Minimum 10 runs recommended for stable baseline.

---

**Created**: 2025-11-21
**Status**: ✅ Current
