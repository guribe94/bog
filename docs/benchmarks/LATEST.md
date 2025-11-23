# Latest Benchmark Results

**Most Recent Run:** 2025-11-21

**Location:** [docs/benchmarks/2025-11/2025-11-21/](2025-11/2025-11-21/)

---

## Quick Summary

**Date:** 2025-11-21
**Status:** VERIFIED - Comprehensive benchmark suite expansion
**System:** macOS Sequoia 15.6 (Darwin 24.6.0), Apple Silicon

### Headline Numbers

| Metric | Value | vs Target |
|--------|-------|-----------|
| **Tick-to-trade latency** | **70.79 ns** | 14.1x under 1μs target |
| **Engine overhead** | 2.19 ns | Minimal dispatch cost |
| **Strategy calculation** | 15.87-18.46 ns | ZST optimized |
| **Signal execution** | 394-438 ns | Includes OrderID generation |
| **Position reads** | 0.49-0.54 ns | Sub-nanosecond atomics |
| **Position updates** | 6.37-7.31 ns | Lock-free, cache-aligned |

### Key Achievements

1. **Sub-microsecond performance validated**
   - Complete pipeline: 71 ns
   - With SHM I/O: ~171 ns estimated
   - Leaves 83% of 1μs budget for network

2. **Comprehensive test coverage**
   - 13 benchmark files
   - 67 test cases
   - 121 measurements
   - 670,000+ iterations

3. **Zero-overhead abstractions confirmed**
   - Const generics fully optimized
   - ZST strategies inlined
   - Type-state FSM compiled away
   - No heap allocations in hot path

4. **Safety features have negligible overhead**
   - Circuit breaker: 28.87 ns
   - Risk validation: 2.18 ns
   - Overflow checks: same or faster than unchecked

### Files

- [full_suite.txt](2025-11/2025-11-21/full_suite.txt) - Complete criterion output (1,777 lines)
- [REPORT.md](2025-11/2025-11-21/REPORT.md) - Detailed analysis with system context

### Comparison to Previous

First successful benchmark run after Phase 1-3 implementation. The 2025-11-19 attempt failed due to compilation errors. These results establish the performance baseline for the zero-overhead architecture.

---

## Navigation

- **Full report:** [2025-11-21/REPORT.md](2025-11/2025-11-21/REPORT.md)
- **All runs:** [INDEX.md](INDEX.md)
- **Benchmark guide:** [README.md](README.md)
- **Latency budgets:** [latency-budget.md](latency-budget.md)

---

**Updated:** 2025-11-21
