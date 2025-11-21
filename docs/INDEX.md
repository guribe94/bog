# Bog Documentation Index

**Quick Navigation**: Master index for all documentation

---

## 🚀 Getting Started

**New to Bog?** Start here:
1. [README.md](../README.md) - Project overview, quick start
2. [Market Making Guide](guides/market-making-guide.md) - How the strategy works
3. [Command Reference](guides/command-reference.md) - How to run the bot

---

## 📚 By Role

### For New Users
- ✅ [README.md](../README.md) - Quick start
- ✅ [Market Making Guide](guides/market-making-guide.md) - Strategy walkthrough
- ✅ [Command Reference](guides/command-reference.md) - Basic commands
- ✅ [Market Selection](guides/market-selection.md) - Choosing markets

### For Developers
- ✅ [System Design](architecture/system-design.md) - Core architecture
- ✅ [State Machines](architecture/STATE_MACHINES.md) - Typestate patterns
- ✅ [Overflow Handling](architecture/overflow-handling.md) - Safety design
- ✅ [Huginn Integration](HUGINN_INTEGRATION_GUIDE.md) - Market data IPC
- ✅ [Project Roadmap](PROJECT_ROADMAP.md) - Development phases

### For Operators
- ✅ [Production Readiness](deployment/PRODUCTION_READINESS.md) - Complete ops manual
- ✅ [24H Deployment Guide](deployment/24H_DEPLOYMENT_GUIDE.md) - Quick deploy
- ✅ [Failure Modes](deployment/failure-modes.md) - Troubleshooting guide
- ✅ [Paper Trading Realism](deployment/PAPER_TRADING_REALISM.md) - Testing guide

### For Performance Engineers
- ✅ [Latency Budget](performance/latency-budget.md) - Component breakdown
- ✅ [Measured Performance](performance/MEASURED_PERFORMANCE_COMPLETE.md) - Benchmark results

---

## 📂 By Topic

### Architecture
Core system design and implementation patterns

| Document | Lines | Purpose |
|----------|-------|---------|
| [system-design.md](architecture/system-design.md) | 753 | Zero-overhead architecture |
| [STATE_MACHINES.md](architecture/STATE_MACHINES.md) | 586 | Typestate FSM patterns |
| [overflow-handling.md](architecture/overflow-handling.md) | 352 | Safety architecture |
| [fill-processing-trace.md](architecture/fill-processing-trace.md) | 418 | Order lifecycle trace |

**Key concepts**: Zero-cost abstractions, compile-time safety, cache optimization

### Performance
Latency analysis and benchmarks

| Document | Lines | Purpose |
|----------|-------|---------|
| [latency-budget.md](performance/latency-budget.md) | 803 | Component latencies |
| [MEASURED_PERFORMANCE_COMPLETE.md](performance/MEASURED_PERFORMANCE_COMPLETE.md) | 680 | Verified benchmarks |

**Target**: <1μs tick-to-trade (measured: 70ns)

### Deployment
Production operations and guides

| Document | Lines | Purpose |
|----------|-------|---------|
| [PRODUCTION_READINESS.md](deployment/PRODUCTION_READINESS.md) | 921 | Complete ops manual |
| [failure-modes.md](deployment/failure-modes.md) | 1116 | Failure scenarios |
| [24H_DEPLOYMENT_GUIDE.md](deployment/24H_DEPLOYMENT_GUIDE.md) | 137 | Quick deployment |
| [PAPER_TRADING_REALISM.md](deployment/PAPER_TRADING_REALISM.md) | 91 | Testing realism |

**Status**: 95% production-ready (pending Lighter SDK)

### Guides
User-facing tutorials and references

| Document | Lines | Purpose |
|----------|-------|---------|
| [market-making-guide.md](guides/market-making-guide.md) | ~700 | Strategy deep dive |
| [command-reference.md](guides/command-reference.md) | 134 | CLI commands |
| [market-selection.md](guides/market-selection.md) | 77 | Market configuration |

**Best for**: Learning the system, daily operations

### Integration
External system connections

| Document | Lines | Purpose |
|----------|-------|---------|
| [HUGINN_INTEGRATION_GUIDE.md](HUGINN_INTEGRATION_GUIDE.md) | 376 | Shared memory IPC |

**Dependency**: Huginn v0.4.0+ for market data

### Design
Historical design documents

| Document | Lines | Purpose |
|----------|-------|---------|
| [PHASE4_REALISTIC_FILLS.md](design/PHASE4_REALISTIC_FILLS.md) | 462 | Realistic fill simulation |

**Note**: Historical context, may be outdated

---

## 🔍 By Use Case

### "I want to run the bot for the first time"
1. [README.md](../README.md) - Quick start
2. [24H Deployment Guide](deployment/24H_DEPLOYMENT_GUIDE.md) - Step-by-step
3. [Command Reference](guides/command-reference.md) - CLI usage

### "I want to understand how it makes money"
1. [Market Making Guide](guides/market-making-guide.md) - Complete strategy explanation

### "I want to deploy to production"
1. [Production Readiness](deployment/PRODUCTION_READINESS.md) - Pre-flight checklist
2. [Failure Modes](deployment/failure-modes.md) - What can go wrong
3. [Measured Performance](performance/MEASURED_PERFORMANCE_COMPLETE.md) - Verify targets

### "I want to modify the code"
1. [System Design](architecture/system-design.md) - Architecture overview
2. [State Machines](architecture/STATE_MACHINES.md) - Safety patterns
3. [Project Roadmap](PROJECT_ROADMAP.md) - Current status

### "Something went wrong"
1. [Failure Modes](deployment/failure-modes.md) - Troubleshooting
2. [Command Reference](guides/command-reference.md) - Debug commands
3. [Production Readiness](deployment/PRODUCTION_READINESS.md) - Emergency procedures

---

## 📊 Quick Reference Tables

### Performance Targets
| Component | Budget | Measured | Status |
|-----------|--------|----------|--------|
| Tick-to-trade | <1μs | 70.79ns | ✅ 14x under |
| Strategy calc | <100ns | 17.28ns | ✅ 5.8x under |
| Risk validation | <50ns | 2.37ns | ✅ 21x under |
| Orderbook sync | <50ns | ~20ns | ✅ 2.5x under |

### Safety Layers
1. ✅ Compile-time spread validation (won't compile if unprofitable)
2. ✅ Market data validation (spread, liquidity, prices)
3. ✅ Position limits (max 1.0 BTC)
4. ✅ Daily loss limits (max $1,000)
5. ✅ Circuit breaker (>10% move halts)
6. ✅ Rate limiter (10 orders/sec)
7. ✅ Pre-trade validation (6 checks)
8. ✅ Kill switch (SIGUSR1)

### File Organization
```
docs/
├── INDEX.md                    ← You are here
├── README.md                   ← Start here
├── PROJECT_ROADMAP.md          ← Development plan
├── HUGINN_INTEGRATION_GUIDE.md ← Market data
├── architecture/               ← Design docs
├── performance/                ← Benchmarks
├── deployment/                 ← Operations
├── guides/                     ← User guides
└── design/                     ← Historical
```

---

## 🏷️ Document Status

| Badge | Meaning |
|-------|---------|
| ✅ Current | Up-to-date, actively maintained |
| ⚠️ Needs Update | May have outdated info |
| 🚧 Work in Progress | Incomplete |
| 📜 Historical | Archived for reference |

All documents in this index are ✅ Current unless noted.

---

## 📝 Contributing to Documentation

When adding or updating docs:
1. **Add header block** with: Purpose, Audience, Prerequisites, Related
2. **Add TL;DR section** for LLM quick reference
3. **Update this index** with new document
4. **Fix all cross-references** to use relative paths
5. **Add status badge** (✅ ⚠️ 🚧 📜)

---

## 🔗 External Resources

- **Huginn Repository**: `../../huginn/` (sibling repo)
- **Lighter DEX Docs**: https://docs.lighter.xyz
- **Rust Performance Book**: https://nnethercote.github.io/perf-book/

---

**Last Updated**: 2025-11-21
**Maintained by**: Bog Team
**Feedback**: Open an issue or submit a PR
