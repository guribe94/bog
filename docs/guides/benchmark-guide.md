# Benchmark Guide

**Purpose**: Guide for running, interpreting, and contributing to bog's benchmark suite
**Audience**: Developers, performance engineers
**Prerequisites**: Rust 1.90+, Criterion.rs knowledge helpful
**Related**: [Performance Docs](../performance/) | [BASELINE.md](../../bog-core/benches/BASELINE.md)

---

## Quick Reference

**Run all benchmarks**:
```bash
cargo bench --package bog-core
```

**Run specific benchmark**:
```bash
cargo bench --package bog-core --bench engine_bench
```

**Compare against baseline**:
```bash
cargo bench --package bog-core --baseline main
```

---

## Benchmark Suite Overview

**Total Benchmarks**: 13 files, 60+ individual tests
**Framework**: Criterion.rs 0.5 with HTML reports
**Sample Size**: 10,000 iterations (100-1,000 for heavy tests)
**Confidence**: 99% (p < 0.01)

### Benchmark Files

| File | Focus | Tests | Target |
|------|-------|-------|--------|
| engine_bench.rs | Core engine + error paths | 11 | <1μs pipeline |
| depth_bench.rs | Orderbook calculations | 4 | <50ns per op |
| conversion_bench.rs | Type conversions | 5 | <20ns per convert |
| atomic_bench.rs | Atomic operations + contention | 5 | <5ns per op |
| tls_overhead_bench.rs | Thread-local overhead | 5 | <10ns TLS access |
| resilience_bench.rs | Gap/stale detection | 7 | <10ns per check |
| **inventory_strategy_bench.rs** | InventoryBased strategy | 6 | <50ns calc |
| **circuit_breaker_bench.rs** | Risk circuit breaker | 6 | <5ns check |
| **multi_tick_bench.rs** | Realistic sequences | 6 | <100μs/1000 ticks |
| **order_fsm_bench.rs** | State transitions | 8 | <10ns transition |
| **fill_processing_bench.rs** | Fill handling | 6 | <50ns per fill |
| **throughput_bench.rs** | System limits | 4 | >10k ticks/sec |
| **reconciliation_bench.rs** | Position reconciliation | 6 | <100ns per check |

**Bold** = newly added in 2025-11-21 expansion

---

## Running Benchmarks

### Quick Run (Skip Heavy Tests)
```bash
cargo bench --package bog-core -- --quick
```

### Save Results
```bash
cargo bench --package bog-core 2>&1 | tee benchmark_results_$(date +%Y%m%d).txt
```

### Specific Test
```bash
cargo bench --package bog-core --bench engine_bench tick_to_trade
```

### With Baseline Comparison
```bash
# Establish baseline
cargo bench --package bog-core -- --save-baseline main

# Later, compare against it
cargo bench --package bog-core -- --baseline main
```

---

## Interpreting Results

### Understanding Output

```
time:   [70.49 ns 70.79 ns 71.09 ns]
        └─────┬─────┘  │  └─────┬─────┘
              │        │        │
        Lower bound  Mean  Upper bound
        (95% CI)           (95% CI)
```

**What it means**:
- Mean: 70.79ns (best estimate)
- 95% confident true mean is between 70.49ns and 71.09ns
- Tight range = stable, consistent performance

### Performance Status

| Result | Interpretation | Action |
|--------|----------------|--------|
| ✅ < Target | Good! Under budget | No action |
| ⚠️ Near Target | Acceptable but watch | Monitor for regressions |
| ❌ > Target | Over budget | Optimize or revise target |

### Regression Detection

```
change: [+25.1% +27.3% +29.5%] (p = 0.00 < 0.01)
Performance has regressed.
```

**This means**:
- Performance degraded by ~27%
- High statistical confidence (p < 0.01)
- **Action Required**: Investigate recent changes

### Outliers

```
Found 96 outliers among 10000 measurements (0.96%)
  52 (0.52%) high mild
  44 (0.44%) high severe
```

**Normal**: <1-2% outliers (OS scheduler, cache misses)
**Problematic**: >5% outliers (indicates instability)

**Common causes**:
- Background processes (close browsers, IDEs)
- Thermal throttling (laptop overheating)
- Power management (plug in laptop)
- CPU frequency scaling (use performance governor)

---

## Performance Targets

See [BASELINE.md](../../bog-core/benches/BASELINE.md) for complete reference.

### Critical Paths

| Component | Target | Baseline | Threshold |
|-----------|--------|----------|-----------|
| **Tick-to-trade** | <1000ns | 70.79ns | >77ns |
| SimpleSpread | <50ns | 15.66ns | >17ns |
| InventoryBased | <50ns | TBD | TBD |
| Risk validation | <50ns | 2.12ns | >2.3ns |
| Circuit breaker | <5ns | TBD | TBD |
| Order FSM transition | <10ns | TBD | TBD |
| Fill processing | <50ns | TBD | TBD |

**TBD** = To be determined after running new benchmarks

---

## Hardware Requirements

**Current Baseline** (Apple M1, 8GB RAM):
See [BASELINE.md](../../bog-core/benches/BASELINE.md) for verified specs.

**For Accurate Benchmarking**:
- Dedicated machine (no background processes)
- Stable clock speed (disable CPU frequency scaling)
- Plugged in (not on battery)
- Cool environment (prevent thermal throttling)
- Linux with performance governor recommended

**Commands for Linux**:
```bash
# Set performance governor
sudo cpufreq-set -g performance

# Disable CPU frequency scaling
echo performance | sudo tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor

# Pin to specific CPU
taskset -c 0 cargo bench

# Real-time priority (requires cap_sys_nice)
sudo nice -n -20 cargo bench
```

---

## Benchmark Best Practices

### Writing New Benchmarks

**Template**:
```rust
//! Benchmark: <Component> Performance
//!
//! Purpose: <What you're measuring and why>
//! Target: <Performance target>
//!
//! What's Measured:
//! - <Specific operation 1>
//! - <Specific operation 2>
//!
//! Why This Matters:
//! <Explain business/technical importance>

use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn bench_my_operation(c: &mut Criterion) {
    let mut group = c.benchmark_group("my_group");
    group.significance_level(0.01).sample_size(10000);

    // Setup code here

    group.bench_function("my_test", |b| {
        b.iter(|| {
            black_box(my_operation(black_box(input)));
        });
    });

    group.finish();
}

criterion_group!(benches, bench_my_operation);
criterion_main!(benches);
```

**Key Points**:
1. Always use `black_box()` to prevent compiler over-optimization
2. Set `significance_level(0.01)` for 99% confidence
3. Use `sample_size(10000)` for micro-benchmarks
4. Use `sample_size(100-1000)` for heavy operations
5. Add clear comments explaining what's measured and why
6. Document expected performance target
7. Group related benchmarks together

### Preventing Over-Optimization

**Bad** (compiler optimizes away):
```rust
b.iter(|| {
    my_function(42);  // Compiler sees constant input, optimizes
});
```

**Good** (forces real execution):
```rust
b.iter(|| {
    black_box(my_function(black_box(42)));
});
```

### Sample Size Selection

| Operation Type | Sample Size | Reason |
|----------------|-------------|--------|
| <100ns micro-op | 10,000 | Need high precision |
| 100ns-1μs | 1,000 | Balance precision/time |
| 1μs-100μs | 100 | Prevent timeout |
| >100μs | 50 | Very slow operations |

---

## Regression Prevention

### Establishing Baselines

```bash
# After major optimization
cargo bench --package bog-core -- --save-baseline optimized_v1

# Later development
cargo bench --package bog-core -- --baseline optimized_v1
```

### Acceptable Variance

**Guidelines**:
- <5% change: Within noise, acceptable
- 5-10% change: Investigate if intentional
- >10% change: **Regression or improvement**, requires analysis
- >20% change: **Significant**, document or fix immediately

### Investigation Steps

1. **Check recent commits**: `git log --oneline -10`
2. **Review changes**: `git diff HEAD~5`
3. **Run baseline comparison**: `cargo bench -- --baseline main`
4. **Profile the code**: Use `perf` or `flamegraph`
5. **Check assembly**: `cargo asm <function>`

---

## Viewing Reports

### HTML Reports

After running benchmarks:
```bash
# Open in browser
open target/criterion/tick_to_trade_pipeline/complete_pipeline/report/index.html
```

Reports include:
- Performance plots over time
- Statistical analysis
- Outlier detection
- Regression analysis

### Command Line

Criterion prints summary to stdout:
```
tick_to_trade_pipeline/complete_pipeline
                        time:   [70.49 ns 70.79 ns 71.09 ns]
                        change: [-1.5% -0.8% -0.1%] (p = 0.03 < 0.05)
                        Change within noise threshold.
```

---

## Common Issues

### High Variance

**Problem**: Wide confidence intervals
**Causes**:
- Background processes
- CPU frequency scaling
- Thermal throttling
- Insufficient warm-up

**Solutions**:
- Close all applications
- Use dedicated benchmark machine
- Increase warm-up time
- Pin to specific CPU core

### Benchmark Failures

**Problem**: Benchmark panics or errors
**Common Causes**:
1. Queue overflow (drain periodically in loops)
2. Invalid test data (check market snapshots)
3. Type mismatches (verify imports)

**Solutions**: See source code comments for each benchmark

### Long Run Times

**Problem**: Benchmarks take hours
**Solutions**:
- Use `--quick` flag for faster runs
- Run specific benchmarks: `--bench engine_bench`
- Reduce sample size for development (increase for release)
- Use `--test` instead of full benchmark for quick checks

---

## Contributing

### Adding New Benchmarks

1. **Create file**: `bog-core/benches/my_bench.rs`
2. **Add to Cargo.toml**:
   ```toml
   [[bench]]
   name = "my_bench"
   harness = false
   ```
3. **Write benchmark**: Follow template above
4. **Verify compiles**: `cargo bench --no-run --bench my_bench`
5. **Run once**: `cargo bench --bench my_bench`
6. **Update docs**: Add to this guide and README.md
7. **Update baseline**: Add to BASELINE.md

### Review Checklist

- [ ] Uses Criterion.rs framework
- [ ] Has clear documentation header
- [ ] Uses `black_box()` appropriately
- [ ] Sets significance level (0.01)
- [ ] Sets appropriate sample size
- [ ] Documents performance target
- [ ] Compiles without errors
- [ ] Runs successfully
- [ ] Added to Cargo.toml
- [ ] Added to README.md
- [ ] Added to BASELINE.md

---

## Resources

- **Criterion.rs Guide**: https://bheisler.github.io/criterion.rs/book/
- **Rust Performance Book**: https://nnethercote.github.io/perf-book/benchmarking.html
- **Flamegraph Guide**: https://github.com/flamegraph-rs/flamegraph

**Internal**:
- [Benchmark README](../../bog-core/benches/README.md) - Suite overview
- [BASELINE.md](../../bog-core/benches/BASELINE.md) - Reference numbers
- [Measured Performance](../performance/MEASURED_PERFORMANCE_COMPLETE.md) - Analysis

---

**Last Updated**: 2025-11-21
**Status**: ✅ Current
