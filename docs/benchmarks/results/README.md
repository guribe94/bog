# Benchmark Results Directory

Consolidated benchmark results from individual runs across different platforms and dates.

## Directory Structure

```
results/
├── README.md                    ← You are here
└── YYYY-MM/                     ← Results organized by year-month
    └── YYYY-MM-DD_HHmmss_platform.md  ← Consolidated result files
```

## Consolidated Format

Each benchmark run is consolidated into a single markdown file containing:

- **Metadata**: Platform specs, date, versions, git commit
- **Headline Numbers**: Key performance metrics
- **Core Pipeline Benchmarks**: Detailed component measurements
- **Component Breakdown**: Per-module analysis
- **Performance Budget**: Latency allocation
- **Regression Analysis**: Comparison with previous run
- **Conclusions**: Summary and insights

## Naming Convention

**Format**: `YYYY-MM-DD_HHmmss_platform.md`

**Examples**:
- `2025-11-21_120000_M1.md`
- `2025-11-22_143052_x86_64.md`
- `2025-12-01_094521_aws_c6in.md`

The timestamp ensures unique filenames for multiple runs on the same day.

## Running Benchmarks

```bash
# Run full benchmark suite
cargo bench

# Save output
cargo bench 2>&1 | tee benchmarks_YYYY-MM-DD_HHmmss.txt
```

## Platform-Specific Guidelines

### M1 Mac (ARM64)
- **Platform name**: "M1"
- **Clock speeds**: Typically 3.2 GHz
- **Memory**: Unified memory architecture
- **Notes**: May show different performance characteristics vs x86-64

### x86-64 Linux (Production)
- **Platform name**: "x86_64"
- **Clock speeds**: Varies (2.5-4.0 GHz typical)
- **Memory**: Traditional RAM architecture
- **Notes**: Expected SIMD differences vs ARM

## Interpreting Results

### Key Metrics

- **Tick-to-trade**: Complete pipeline latency (target <1μs)
- **Engine overhead**: Dispatch cost (target <100ns)
- **Strategy calculation**: Signal generation (target <100ns)
- **Position updates**: Atomic operations (target <50ns)

### What to Look For

**Good signs**:
- Low outlier percentage (<10%)
- Consistent latencies
- No unexpected regressions
- Performance within target budgets

**Red flags**:
- >20% outliers
- Regressions vs previous run
- Performance exceeding budget allocations
- Unexpected platform differences

## References

- **Latest Results**: [../LATEST.md](../LATEST.md)
- **Benchmark Guide**: [../README.md](../README.md)
- **Latency Budget**: [../latency-budget.md](../latency-budget.md)

---

**Last Updated**: 2025-11-21
