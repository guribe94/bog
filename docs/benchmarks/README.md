# Benchmark Results

**Purpose:** Central repository for all benchmark data, tracking performance evolution over time
**Audience:** Developers, performance engineers, system operators
**Status:** Current

---

## Overview

This directory contains comprehensive benchmark results for the Bog HFT trading system, organized chronologically to enable performance tracking and regression detection.

## Directory Structure

```
benchmarks/
├── README.md           # This file
├── INDEX.md            # Chronological manifest of all benchmark runs
├── LATEST.md           # Quick reference to most recent results
├── latency-budget.md   # Component-level latency targets and budgets
└── YYYY-MM/            # Results organized by year-month
    └── YYYY-MM-DD/     # Individual benchmark run
        ├── *.txt       # Raw criterion benchmark output
        ├── REPORT.md   # Analysis with summary tables and system metadata
        └── comparison.md  # Delta analysis vs previous run
```

## Running Benchmarks

### Prerequisites

- Release build: `cargo build --release`
- Stable system state: disable CPU throttling, close background applications
- Recommended: pin to performance CPU cores (see `bog-core/src/perf/cpu.rs`)

### Standard Benchmark Suite

```bash
# Full benchmark suite
cargo bench

# Specific benchmark group
cargo bench --bench engine_bench
cargo bench --bench strategy_bench
cargo bench --bench executor_bench
```

### Recording Results

1. Run benchmarks: `cargo bench 2>&1 | tee benchmark_output.txt`
2. Create dated directory: `docs/benchmarks/YYYY-MM/YYYY-MM-DD/`
3. Move raw output: `mv benchmark_output.txt docs/benchmarks/YYYY-MM/YYYY-MM-DD/full_suite.txt`
4. Create `REPORT.md` using the template below
5. Create `comparison.md` with delta analysis vs previous run
6. Update `INDEX.md` with entry for this run
7. Update `LATEST.md` to point to new results

## Report Template

Each benchmark run should include a `REPORT.md` following this structure:

```markdown
# Benchmark Report: YYYY-MM-DD

## System Context

- **Date:** YYYY-MM-DD HH:MM:SS TZ
- **CPU:** [model, cores, base/boost frequency]
- **RAM:** [amount, speed]
- **OS:** [name, version, kernel]
- **Rust:** [version]
- **Compilation:** [profile, features, target-cpu, lto settings]

## Summary Table

| Benchmark | Mean | Std Dev | Throughput | vs Previous |
|-----------|------|---------|------------|-------------|
| engine/signal_generation | X.XX ns | ±Y.YY ns | Z.ZZ M/s | +A.A% |
| ... | ... | ... | ... | ... |

## Key Findings

- Notable performance improvements
- Identified regressions and root causes
- Achievement of latency targets
- Areas requiring optimization

## Full Results

[Complete criterion output from *.txt files]
```

## Comparison Template

Each `comparison.md` should analyze changes since the previous benchmark run:

```markdown
# Performance Comparison: YYYY-MM-DD vs YYYY-MM-DD

## Summary

- **Total benchmarks:** N
- **Improved:** X (list)
- **Regressed:** Y (list)
- **Unchanged:** Z (within statistical noise)

## Detailed Analysis

### Improvements

| Benchmark | Previous | Current | Delta | Reason |
|-----------|----------|---------|-------|--------|
| ... | ... | ... | +X.X% | [code change/optimization] |

### Regressions

| Benchmark | Previous | Current | Delta | Reason |
|-----------|----------|---------|-------|--------|
| ... | ... | ... | -X.X% | [code change/tradeoff] |

## Root Cause Analysis

[Detailed investigation of significant changes]

## Recommendations

[Actions to address regressions or further optimize improvements]
```

## Interpreting Results

### Criterion Metrics

- **Mean:** Average execution time
- **Std Dev:** Standard deviation, indicates consistency
- **Median:** 50th percentile, robust to outliers
- **MAD:** Median Absolute Deviation
- **Throughput:** Operations per second

### Statistical Significance

Criterion reports changes as:
- **No change:** Difference within noise threshold
- **Improved:** Statistically significant speedup
- **Regressed:** Statistically significant slowdown

Changes under 1-2% may not be statistically significant.

### Latency Targets

Refer to `latency-budget.md` for component-level targets. Key objectives:

- Total tick-to-trade: <1 microsecond
- Signal generation: <100 nanoseconds
- Order creation: <50 nanoseconds
- Engine overhead: <50 nanoseconds

## Regression Detection

Compare new results against previous runs in `INDEX.md`:

1. Identify benchmarks with >5% regression
2. Investigate recent code changes
3. Profile with `perf` or `flamegraph` if needed
4. Document findings in `comparison.md`

## Adding New Benchmarks

When adding new benchmark code:

1. Place in `bog-core/benches/` or appropriate crate
2. Follow naming convention: `component_bench.rs`
3. Use criterion groups for organization
4. Document benchmark purpose and methodology
5. Update this README if needed

## Historical Data

All benchmark runs are preserved in their original dated directories. To track performance trends:

1. Consult `INDEX.md` for run history
2. Compare `REPORT.md` summary tables across dates
3. Review `comparison.md` files for cumulative changes

## Notes

- Benchmark results are system-dependent
- Run on consistent hardware for valid comparisons
- Kernel scheduler, CPU governor, and background processes affect results
- Use multiple runs to establish confidence intervals
- Warm-up iterations reduce cold-start effects
