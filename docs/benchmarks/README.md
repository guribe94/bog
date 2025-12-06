# Bog Benchmark Results

**Purpose:** Central repository for all benchmark data, tracking performance evolution over time
**Audience:** Developers, performance engineers, system operators
**Status:** Current - Consolidated format

---

## Overview

This directory contains comprehensive benchmark results for the Bog HFT trading system. All results are now stored in a consolidated format with one markdown file per run, making it easy to track performance trends and detect regressions.

## Directory Structure

```
benchmarks/
├── README.md                                    # This file
├── LATEST.md                                    # Most recent results
├── latency-budget.md                            # Latency targets
├── results/                                     # Consolidated benchmark results
│   ├── README.md                                # Results index
│   └── YYYY-MM/                                 # Results by year-month
│       └── YYYY-MM-DD_HHmmss_platform.md       # Consolidated result files
└── 2025-11/                                     # Legacy format (being migrated)
    └── 2025-11-21/
        └── REPORT.md
```

## New Consolidated Format

Each consolidated markdown file contains:

- Metadata (platform, CPU, RAM, OS, Rust version, git commit)
- All benchmark results (13 benchmarks for bog)
- Summary statistics per benchmark
- Regression Analysis (comparison with previous run)
- Notes and raw data references

Benchmarks not run in a particular session are marked "NOT RUN" to maintain consistent structure across all files.

## Running Benchmarks (Automated)

The benchmark.sh script automates the entire process:

```bash
# Quick mode (3 critical benchmarks, ~2-5 minutes)
./benchmark.sh

# Full mode (all 13 benchmarks, ~10-20 minutes)
./benchmark.sh --full

# Override platform detection
./benchmark.sh --platform M1

# Skip cargo clean for faster iteration
./benchmark.sh --no-clean

# Skip baseline comparison
./benchmark.sh --no-compare
```

The script will:
1. Auto-detect platform (M1, M2, c6in_xlarge, etc.)
2. Collect system information
3. Run benchmarks (bog-core/benches/)
4. Process results into consolidated markdown
5. Compare with previous run (regression detection >5%)
6. Save to `docs/benchmarks/YYYY-MM/YYYY-MM-DD_HHmmss_platform.md`

## Manual Benchmark Processing

If you have raw Criterion output:

```bash
# Process into consolidated markdown
python3 benchmarks/process_benchmarks.py \
  benchmarks/raw/YYYY-MM/YYYY-MM-DD_HHmmss_platform/ \
  docs/benchmarks/YYYY-MM/YYYY-MM-DD_HHmmss_platform.md \
  --platform M1 \
  --compare-with docs/benchmarks/YYYY-MM/previous_run.md
```

## Bog Benchmarks

The following 13 benchmarks are tracked:

1. **engine_bench** - Core trading engine performance
2. **conversion_bench** - Data type conversions
3. **atomic_bench** - Atomic operations
4. **fill_processing_bench** - Order fill processing
5. **inventory_strategy_bench** - Inventory management
6. **tls_overhead_bench** - Thread-local storage overhead
7. **multi_tick_bench** - Multi-tick processing
8. **circuit_breaker_bench** - Circuit breaker logic
9. **depth_bench** - Order book depth operations
10. **throughput_bench** - Overall system throughput
11. **order_fsm_bench** - Order state machine
12. **reconciliation_bench** - Position reconciliation
13. **resilience_bench** - Error recovery and resilience

## Latency Targets

Refer to `latency-budget.md` for component-level targets. Key objectives:

- **Total tick-to-trade**: <1 microsecond
- **Signal generation**: <100 nanoseconds
- **Order creation**: <50 nanoseconds
- **Engine overhead**: <50 nanoseconds

## Regression Detection

Automatically compares with the most recent run on the same platform:

- **Regressions**: >5% slower (highlighted in report)
- **Improvements**: >5% faster (highlighted in report)
- **No change**: Within ±5% threshold

Changes under 1-2% may not be statistically significant.

## File Locations

### Raw Data
```
benchmarks/raw/YYYY-MM/YYYY-MM-DD_HHmmss_platform/
├── engine_bench.txt                # Raw Criterion output
├── conversion_bench.txt
├── ...
└── system_info.env                 # Platform metadata
```

### Consolidated Reports
```
docs/benchmarks/YYYY-MM/
└── YYYY-MM-DD_HHmmss_platform.md   # Single consolidated file
```

## Migrating Historical Data

To migrate old benchmark files to the new format:

```bash
# Dry run (see what would happen)
python3 benchmarks/migrate_historical_data.py --dry-run

# Run migration
python3 benchmarks/migrate_historical_data.py
```

This preserves original REPORT.md files while creating new consolidated versions.

## Prerequisites for Benchmarking

- **Release build**: `cargo build --release`
- **Stable system state**: Disable CPU throttling, close background applications
- **Recommended**: Pin to performance CPU cores (see `bog-core/src/perf/cpu.rs`)

## Platform-Specific Guidelines

### M1/M2/M3 Mac (ARM64)
- **Platform name**: Auto-detected as "M1", "M2", or "M3"
- **Clock speeds**: Typically 3.2 GHz
- **Memory**: Unified memory architecture
- **Note**: Results differ significantly from x86-64

### x86-64 Linux (Production)
- **Platform name**: Detected as "x86_64_intel", "x86_64_amd", or "linux"
- **Clock speeds**: Varies (2.5-4.0 GHz typical)
- **Memory**: Traditional RAM architecture
- **Primary target**: Production deployment platform

### AWS EC2
- **Instance types**: Auto-detected (c6in_xlarge, c7g_xlarge, etc.)
- **Override detection**: Use `--platform` flag if needed
- **Variability**: May have higher variance due to virtualization

## Interpreting Results

### Consolidated File Sections

1. **Metadata**: Platform specs, date, versions, git info
2. **Benchmark Results**: All 13 benchmarks with summary stats
3. **Regression Analysis**: Comparison with previous run

### What to Look For

**Good signs**:
- Low outlier percentage (<10%)
- Consistent latencies across runs
- Meeting latency budget targets
- No unexpected regressions

**Red flags**:
- >20% outliers
- Regressions vs previous run
- Exceeding latency budgets
- High variance between runs

## Statistical Quality

Criterion provides:
- **Mean**: Average execution time
- **Std Dev**: Standard deviation (consistency indicator)
- **Median**: 50th percentile (robust to outliers)
- **MAD**: Median Absolute Deviation
- **Confidence intervals**: 95% confidence by default

## Adding New Benchmarks

When adding new benchmark code:

1. Place in `bog-core/benches/component_bench.rs`
2. Follow naming convention: `component_bench.rs`
3. Use criterion groups for organization
4. Document benchmark purpose and methodology
5. Update `ALL_BENCHMARKS` list in `benchmarks/process_benchmarks.py`
6. Update this README if needed

## References

- **Benchmark Script**: [../../benchmark.sh](../../benchmark.sh)
- **Processing Script**: [../../benchmarks/process_benchmarks.py](../../benchmarks/process_benchmarks.py)
- **Migration Script**: [../../benchmarks/migrate_historical_data.py](../../benchmarks/migrate_historical_data.py)
- **Latency Budget**: [latency-budget.md](latency-budget.md)
- **Results Index**: [results/README.md](results/README.md)
- **Latest Results**: [LATEST.md](LATEST.md)

## Questions?

If you're unsure about:
- How to run benchmarks → Run `./benchmark.sh --help`
- How to interpret results → See consolidated markdown file sections
- What changed from previous run → Check "Regression Analysis" section
- Latency targets → See latency-budget.md
- Historical context → See results/README.md

---

## Writing New Benchmarks

### Template

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

### Key Points

1. Always use `black_box()` to prevent compiler over-optimization
2. Set `significance_level(0.01)` for 99% confidence
3. Use `sample_size(10000)` for micro-benchmarks (<100ns)
4. Use `sample_size(100-1000)` for heavy operations (>1μs)
5. Add clear comments explaining what's measured and why
6. Document expected performance target

### Sample Size Selection

| Operation Type | Sample Size | Reason |
|----------------|-------------|--------|
| <100ns micro-op | 10,000 | Need high precision |
| 100ns-1μs | 1,000 | Balance precision/time |
| 1μs-100μs | 100 | Prevent timeout |
| >100μs | 50 | Very slow operations |

### Checklist for New Benchmarks

- [ ] Uses Criterion.rs framework
- [ ] Has clear documentation header
- [ ] Uses `black_box()` appropriately
- [ ] Sets significance level (0.01)
- [ ] Sets appropriate sample size
- [ ] Documents performance target
- [ ] Compiles without errors
- [ ] Added to Cargo.toml
- [ ] Added to bog-core/benches/README.md

---

## Troubleshooting

### High Variance

**Problem**: Wide confidence intervals
**Causes**: Background processes, CPU frequency scaling, thermal throttling

**Solutions**:
- Close all applications
- Use dedicated benchmark machine
- Pin to specific CPU core (`taskset -c 0 cargo bench`)

### Long Run Times

**Problem**: Benchmarks take hours

**Solutions**:
- Use `--quick` flag for faster runs
- Run specific benchmarks: `--bench engine_bench`
- Use `--test` instead of full benchmark for quick checks

---

## External Resources

- **Criterion.rs Guide**: https://bheisler.github.io/criterion.rs/book/
- **Rust Performance Book**: https://nnethercote.github.io/perf-book/benchmarking.html
- **Flamegraph Guide**: https://github.com/flamegraph-rs/flamegraph

**Last Updated**: 2025-12-05
