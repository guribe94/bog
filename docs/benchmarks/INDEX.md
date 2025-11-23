# Benchmark Run Index

**Purpose:** Chronological manifest tracking all benchmark runs and their contexts
**Audience:** Developers tracking performance evolution
**Status:** Current

---

## Active Benchmark Runs

This index tracks all benchmark runs in chronological order, providing context for each run and enabling performance trend analysis.

### 2025-11-21: Expanded Benchmark Suite

**Location:** `docs/benchmarks/2025-11/2025-11-21/`

**Context:**
- Comprehensive benchmark suite expansion
- Added new benchmark groups for realistic trading scenarios
- Expanded test coverage across all components

**System:**
- Platform: macOS (Darwin 24.6.0)
- Rust: [version from benchmark run]
- Compilation: release mode with optimizations

**Key Results:**
- Signal generation: 37-49 ns
- Order creation: 27-39 ns
- Position updates: 12-30 ns
- Fill processing: 118-152 ns (complete realistic simulation)
- Total engine overhead: <100 ns

**Notable Changes:**
- Expanded from baseline suite
- Added fill queue, safety, position limit, and fee accounting benchmarks
- Demonstrated sub-microsecond tick-to-trade capability

**Files:**
- `full_suite.txt` - Complete criterion output
- `REPORT.md` - Analysis and summary
- `comparison.md` - Delta vs 2025-11-19

---

### 2025-11-19: Initial Baseline

**Location:** `docs/benchmarks/2025-11/2025-11-19/`

**Context:**
- Established performance baseline for zero-overhead architecture
- Initial benchmark suite after Phase 1-3 implementation
- Reference point for future regression detection

**System:**
- Platform: macOS (Darwin 24.6.0)
- Rust: [version from benchmark run]
- Compilation: release mode with optimizations

**Key Results:**
- Signal generation: ~40 ns
- Order creation: ~30 ns
- Position updates: ~20 ns
- Engine processing: <100 ns total

**Notable Changes:**
- First comprehensive benchmark run
- Established baseline metrics for regression testing
- Validated sub-100ns component latencies

**Files:**
- `full_suite.txt` - Full benchmark suite output
- `engine_only.txt` - Engine-specific benchmarks
- `REPORT.md` - Analysis and summary

---

## Benchmark Evolution Summary

| Date | Total Tests | Avg Engine Latency | Key Achievement | Notable Regressions |
|------|-------------|-------------------|-----------------|---------------------|
| 2025-11-19 | ~20 | <100 ns | Initial baseline established | N/A (baseline) |
| 2025-11-21 | ~45 | <100 ns | Expanded test coverage, validated realistic fills | None detected |

## Performance Trends

### Signal Generation
- 2025-11-19: ~40 ns baseline
- 2025-11-21: 37-49 ns (maintained, slight variation across test scenarios)
- **Trend:** Stable, meeting <100ns target

### Order Creation
- 2025-11-19: ~30 ns baseline
- 2025-11-21: 27-39 ns (maintained)
- **Trend:** Stable, well under <50ns target

### Fill Processing
- 2025-11-19: Not benchmarked
- 2025-11-21: 118-152 ns (realistic simulation with all safety checks)
- **Trend:** New capability, exceeds budget but includes comprehensive validation

### Total Tick-to-Trade
- Both runs demonstrate <1 microsecond capability
- Consistent sub-microsecond performance across expansion

## Adding New Runs

When adding a new benchmark run:

1. Create directory: `docs/benchmarks/YYYY-MM/YYYY-MM-DD/`
2. Add raw output, REPORT.md, and comparison.md
3. Add entry to this INDEX.md following the template below
4. Update "Benchmark Evolution Summary" table
5. Update "Performance Trends" section with new data points
6. Update `LATEST.md` to point to new run

### Entry Template

```markdown
### YYYY-MM-DD: [Descriptive Title]

**Location:** `docs/benchmarks/YYYY-MM/YYYY-MM-DD/`

**Context:**
- Reason for benchmark run
- Notable code changes since last run
- Testing objectives

**System:**
- Platform: [OS, version]
- CPU: [model, frequency]
- RAM: [amount]
- Rust: [version]
- Compilation: [flags, features]

**Key Results:**
- Component X: N ns
- Component Y: N ns
- Total latency: N μs

**Notable Changes:**
- Improvements: [list with percentages]
- Regressions: [list with percentages and reasons]

**Files:**
- `[name].txt` - [description]
- `REPORT.md` - Analysis and summary
- `comparison.md` - Delta vs [previous date]
```

## Historical Context

### Phase 1-3 Implementation (Pre-Benchmarking)
Prior to 2025-11-19, the system underwent major architectural changes:
- Phase 1: Const generic engine foundation
- Phase 2: Strategy migration to zero-sized types
- Phase 3: Simulated executor with object pools

The 2025-11-19 baseline represents the first comprehensive benchmarking after these foundational changes.

## Regression Investigation

If regressions are detected:

1. Compare REPORT.md summary tables between runs
2. Review comparison.md for detailed delta analysis
3. Correlate with git commits between benchmark dates
4. Profile with `perf` or `flamegraph` if regression >5%
5. Document findings in the affected run's comparison.md

## Notes

- All times are in nanoseconds unless otherwise specified
- Latency targets defined in `latency-budget.md`
- Statistical significance: changes >5% warrant investigation
- System configuration affects results - maintain consistent environment
