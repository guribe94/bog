# Bog Documentation Hub

> Sub-microsecond HFT trading engine with zero-overhead abstractions

**Quick Links**: [Complete Index](INDEX.md) | [Quick Start](../README.md) | [Benchmarks](benchmarks/LATEST.md)

---

##  Start Here

**New to Bog?**
1. [Quick Start (../README.md)](../README.md) - Build and run in 5 minutes
2. [Market Making Guide](guides/market-making-guide.md) - Understand the strategy
3. [Command Reference](guides/command-reference.md) - Basic operations

**Deploying to Production?**
1. [Production Readiness](deployment/PRODUCTION_READINESS.md) - Complete checklist
2. [24H Deployment Guide](deployment/24H_DEPLOYMENT_GUIDE.md) - Quick deployment
3. [Failure Modes](deployment/failure-modes.md) - Troubleshooting

**Developing?**
1. [System Design](architecture/system-design.md) - Architecture deep dive
2. [State Machines](architecture/STATE_MACHINES.md) - Safety patterns
3. [Project Roadmap](PROJECT_ROADMAP.md) - Current status

---

##  Documentation Structure

```
docs/
 INDEX.md                     ← Complete navigation
 README.md                    ← You are here
 PROJECT_ROADMAP.md           ← Development phases
 HUGINN_INTEGRATION_GUIDE.md  ← Market data IPC
 architecture/                ← Core design
    system-design.md         ← Start here for architecture
    STATE_MACHINES.md        ← Typestate FSMs
    overflow-handling.md     ← Safety architecture
    fill-processing-trace.md ← Order lifecycle
 benchmarks/                  ← Performance data & analysis
    LATEST.md                ← Most recent results
    INDEX.md                 ← All benchmark runs
    latency-budget.md        ← Component targets
    2025-11/                 ← Historical results by date
 deployment/                  ← Operations
    PRODUCTION_READINESS.md  ← Complete ops manual
    failure-modes.md         ← Troubleshooting (1116 lines!)
    24H_DEPLOYMENT_GUIDE.md  ← Quick deploy
    PAPER_TRADING_REALISM.md ← Testing
 guides/                      ← User guides
    market-making-guide.md   ← Strategy deep dive
    command-reference.md     ← CLI commands
    market-selection.md      ← Market config
 design/                      ← Historical
     PHASE4_REALISTIC_FILLS.md
```

---

##  By Topic

### Architecture & Design

**Core system design and implementation patterns**

1. **[System Design](architecture/system-design.md)** - Start here to understand bog's architecture
   - Zero-overhead design principles
   - Const generic monomorphization
   - Cache-line alignment
   - Shared memory IPC with Huginn
   - Strategy and Executor patterns

2. **[Benchmark Results](benchmarks/LATEST.md)** - Verified performance data
   - 70.79ns tick-to-trade latency measured
   - Comprehensive component benchmarks
   - See [latency-budget.md](benchmarks/latency-budget.md) for targets

3. **[Failure Modes](deployment/failure-modes.md)** - Learn operational considerations
   - 10 major failure scenarios
   - Detection and mitigation strategies
   - Incident response procedures

### For Developers

1. **[Overflow Handling](architecture/overflow-handling.md)** - Critical safety architecture
   - Checked vs saturating vs wrapping arithmetic
   - Fixed-point conversion safety
   - Error handling patterns
   - Testing strategies

2. **[System Design](architecture/system-design.md)** - Deep dive into implementation
   - ZST strategies and executors
   - Atomic operations and lock-free algorithms
   - Bounded collections for backpressure
   - Fixed-point arithmetic details

3. **Property Tests** - Mathematical verification
   - See `bog-core/src/core/fixed_point_proptest.rs`
   - 17 property tests, 1700+ randomized cases

4. **Fuzz Tests** - Edge case discovery
   - See `bog-core/fuzz/README.md`
   - 3 fuzz targets, billions of executions

### For Operators

1. **[Failure Modes](deployment/failure-modes.md)** - Operational playbook
   - Position overflow handling
   - Flash crash detection
   - Network failure recovery
   - Dependency monitoring

2. **[Latency Budget](performance/latency-budget.md)** - Performance expectations
   - Normal: 27ns (p50)
   - Degraded: 45ns (p99)
   - Alert thresholds

3. **Monitoring** (future: `deployment/monitoring.md`)
   - Prometheus metrics
   - Alert rules
   - Dashboard setup

---

## Architecture Overview

### Design Principles

**1. Zero-Cost Abstractions**
```rust
// Strategy trait with zero runtime cost
pub trait Strategy {
    fn generate_signal(&self, market: &MarketState, position: &Position)
        -> Option<Signal>;
}

// Zero-sized type implementation
pub struct SimpleSpread;  // 0 bytes

impl Strategy for SimpleSpread {
    #[inline(always)]
    fn generate_signal(&self, ...) -> Option<Signal> {
        // All constants folded at compile time
        let spread_bps = Self::SPREAD_BPS;  // Const
        // ...
    }
}

// Monomorphized at compile time (no vtables, no dispatch)
type MyEngine = Engine<SimpleSpread, SimulatedExecutor>;
```

**Result**: ~10ns signal generation (vs ~60ns with dynamic dispatch).

**2. Compile-Time Configuration**
```bash
# All configuration via Cargo features (no TOML parsing)
cargo build --release \
  --features spread-10bps,size-medium,min-spread-1bps
```

**Result**: 0ns runtime config lookup (vs ~2ns per lookup).

**3. Cache-First Design**
```rust
#[repr(C, align(64))]  // Force 64-byte alignment (one cache line)
pub struct Position {
    pub quantity: AtomicI64,      // Most accessed field first
    pub realized_pnl: AtomicI64,
    pub daily_pnl: AtomicI64,
    // ... (64 bytes total)
}
```

**Result**: ~2ns position updates (all L1 cache hits).

---

##  Performance at a Glance

| Metric | Target | Measured | Status |
|--------|--------|----------|--------|
| **Tick-to-trade** | <1μs | **70.79ns** |  **14x under budget** |
| Strategy calc | <100ns | 17.28ns |  5.8x under |
| Risk validation | <50ns | 2.37ns |  21x under |
| Orderbook sync | <50ns | ~20ns |  2.5x under |

→ See [Measured Performance](performance/MEASURED_PERFORMANCE_COMPLETE.md) for full benchmarks

---

##  Document Summaries

### Architecture

| Document | What It Covers | Read Time |
|----------|----------------|-----------|
| [system-design.md](architecture/system-design.md) | Zero-overhead abstractions, cache design, IPC | 30 min |
| [STATE_MACHINES.md](architecture/STATE_MACHINES.md) | Typestate FSMs, compile-time safety | 25 min |
| [overflow-handling.md](architecture/overflow-handling.md) | Arithmetic safety, checked operations | 20 min |
| [fill-processing-trace.md](architecture/fill-processing-trace.md) | Order lifecycle walkthrough | 15 min |

### Performance

| Document | What It Covers | Read Time |
|----------|----------------|-----------|
| [latency-budget.md](performance/latency-budget.md) | Component-by-component breakdown | 40 min |
| [MEASURED_PERFORMANCE_COMPLETE.md](performance/MEASURED_PERFORMANCE_COMPLETE.md) | Verified benchmark results | 20 min |

### Deployment

| Document | What It Covers | Read Time |
|----------|----------------|-----------|
| [PRODUCTION_READINESS.md](deployment/PRODUCTION_READINESS.md) | Complete ops manual, checklists | 45 min |
| [failure-modes.md](deployment/failure-modes.md) | 10 failure scenarios + mitigations | 45 min |
| [24H_DEPLOYMENT_GUIDE.md](deployment/24H_DEPLOYMENT_GUIDE.md) | Quick deployment steps | 10 min |
| [PAPER_TRADING_REALISM.md](deployment/PAPER_TRADING_REALISM.md) | Testing methodology | 10 min |

### Guides

| Document | What It Covers | Read Time |
|----------|----------------|-----------|
| [market-making-guide.md](guides/market-making-guide.md) | Strategy theory + examples | 35 min |
| [benchmark-guide.md](guides/benchmark-guide.md) | Running and interpreting benchmarks | 20 min |
| [command-reference.md](guides/command-reference.md) | CLI commands | 5 min |
| [market-selection.md](guides/market-selection.md) | Market configuration | 5 min |

---

##  Learning Paths

### Path 1: "I Want to Run It" (30 minutes)
1. [Quick Start](../README.md) - 5 min
2. [Market Making Guide](guides/market-making-guide.md) - 15 min (skim)
3. [Command Reference](guides/command-reference.md) - 5 min
4. [24H Deployment](deployment/24H_DEPLOYMENT_GUIDE.md) - 5 min

### Path 2: "I Want to Understand It" (2 hours)
1. [Market Making Guide](guides/market-making-guide.md) - 35 min (full read)
2. [System Design](architecture/system-design.md) - 30 min
3. [State Machines](architecture/STATE_MACHINES.md) - 25 min
4. [Measured Performance](performance/MEASURED_PERFORMANCE_COMPLETE.md) - 20 min
5. [Huginn Integration](HUGINN_INTEGRATION_GUIDE.md) - 15 min

### Path 3: "I Want to Deploy It" (3 hours)
1. [Production Readiness](deployment/PRODUCTION_READINESS.md) - 45 min
2. [Failure Modes](deployment/failure-modes.md) - 45 min
3. [24H Deployment Guide](deployment/24H_DEPLOYMENT_GUIDE.md) - 10 min
4. [Measured Performance](performance/MEASURED_PERFORMANCE_COMPLETE.md) - 20 min
5. Practice deployment - 60 min

### Path 4: "I Want to Modify It" (4 hours)
1. [System Design](architecture/system-design.md) - 30 min
2. [State Machines](architecture/STATE_MACHINES.md) - 25 min
3. [Overflow Handling](architecture/overflow-handling.md) - 20 min
4. [Latency Budget](performance/latency-budget.md) - 40 min
5. [Fill Processing Trace](architecture/fill-processing-trace.md) - 15 min
6. [Project Roadmap](PROJECT_ROADMAP.md) - 20 min
7. Code exploration - 90 min

---

##  Quick Operations

### Common Tasks

**Run paper trading**:
```bash
./target/release/bog-simple-spread-simulated --market 1
```
→ [Command Reference](guides/command-reference.md)

**Check performance**:
```bash
cargo bench
```
→ [Measured Performance](performance/MEASURED_PERFORMANCE_COMPLETE.md)

**Deploy to production** (when ready):
1. Review [Production Readiness](deployment/PRODUCTION_READINESS.md)
2. Follow [24H Deployment Guide](deployment/24H_DEPLOYMENT_GUIDE.md)
3. Monitor with [Failure Modes](deployment/failure-modes.md) guide

**Troubleshoot issues**:
1. Check [Failure Modes](deployment/failure-modes.md) - Section by symptom
2. Review logs with [Command Reference](guides/command-reference.md)
3. See [Production Readiness](deployment/PRODUCTION_READINESS.md) - Part 10 (Troubleshooting)

---

##  System Status

### Production Readiness: 95%

 **Complete**:
- Market making strategy
- Data ingestion (Huginn)
- Risk management
- State machines
- Monitoring & alerts
- Visualization tools
- Safety infrastructure

 **Pending**:
- Lighter SDK integration (execution stubbed)
- Live trading deployment

---

##  External Resources

- **Huginn Repository**: `../../huginn/` (sibling repo)
- **Lighter DEX API**: https://docs.lighter.xyz
- **Rust Performance Book**: https://nnethercote.github.io/perf-book/

---

##  Contributing

When updating documentation:
1. Add header block: Purpose, Audience, Prerequisites, Related
2. Add TL;DR section for LLM quick reference
3. Update [INDEX.md](INDEX.md) with new document
4. Fix all cross-references (use relative paths)
5. Add status badge ( Current |  Needs Update |  WIP |  Historical)

### Quality Standards
-  All code examples must compile
-  All benchmarks must be verified
-  All broken links must be fixed
-  All outdated info must be marked or updated

---

##  Complete Navigation

Need something specific? Use the [Complete Index](INDEX.md) for:
- By role (users, developers, operators)
- By topic (architecture, performance, deployment)
- By use case ("I want to...")
- Quick reference tables
- All 15+ documents catalogued

---

##  Quick Reference Cards

### Performance Targets

| Component | Budget | Measured | Status |
|-----------|--------|----------|--------|
| SHM read | 10ns | ~5ns |  50% under |
| Signal gen | 100ns | ~10ns |  90% under |
| Order exec | 500ns | ~10ns |  98% under |
| Position update | 20ns | ~2ns |  90% under |
| Overflow checks | 10ns | ~2ns |  80% under |
| **Total** | **640ns** | **~27ns** |  **96% under** |

### Failure Mode Summary

| Failure | Severity | Probability | Status |
|---------|----------|-------------|--------|
| Position overflow | Critical | Near zero |  Protected |
| Conversion errors | High | Low |  Protected |
| Fill queue overflow | High | Medium |  Protected |
| Flash crash | High | Medium |  Partial |
| Clock desync | Medium | Low |  Protected |
| Memory exhaustion | Critical | Near zero |  Protected |
| Network failures | High | Medium |  Partial |
| Race conditions | Critical | Zero |  Protected |
| Strategy errors | High | Low |  Protected |
| Dependency failures | Varies | Medium |  Partial |

### Key Metrics

#### Performance
```promql
# Tick processing latency
histogram_quantile(0.5, bog_tick_latency_ns)  # p50: ~27ns
histogram_quantile(0.99, bog_tick_latency_ns) # p99: ~45ns

# Throughput
rate(bog_ticks_processed_total[1m])  # ~1000 ticks/sec
```

#### Safety
```promql
# Overflow detection
rate(bog_overflow_errors_total[5m]) > 0  # Alert: CRITICAL

# Queue pressure
bog_queue_depth > 100  # Alert: WARNING

# Dropped fills
rate(bog_dropped_fills_total[5m]) > 0  # Alert: CRITICAL
```

---

##  You're Ready!

**For detailed information on any topic**, see:
- [ Complete Index](INDEX.md) - Master navigation
- [ Architecture docs](architecture/) - System design
- [ Performance docs](performance/) - Benchmarks
- [ Deployment docs](deployment/) - Operations
- [ User guides](guides/) - Tutorials

**Need help?** Start with the [Complete Index](INDEX.md) which organizes everything by role, topic, and use case.

---

**Last Updated**: 2025-11-21
**Status**:  Current
**Maintained by**: Bog Team
