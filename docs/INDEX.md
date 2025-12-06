# Bog Documentation Index

Master navigation for all documentation.

## Getting Started

1. [README.md](../README.md) - Project overview, quick start
2. [Market Making Guide](guides/market-making-guide.md) - Strategy explanation
3. [Command Reference](guides/command-reference.md) - CLI usage

## By Role

### For New Users
- [README.md](../README.md) - Quick start
- [Market Making Guide](guides/market-making-guide.md) - Strategy walkthrough
- [Command Reference](guides/command-reference.md) - Basic commands
- [Market Selection](guides/market-selection.md) - Choosing markets

### For Developers
- [System Design](architecture/system-design.md) - Core architecture
- [State Machines](architecture/STATE_MACHINES.md) - Typestate patterns
- [Overflow Handling](architecture/overflow-handling.md) - Safety design
- [Huginn Integration](HUGINN_INTEGRATION_GUIDE.md) - Market data IPC
- [Project Roadmap](PROJECT_ROADMAP.md) - Development phases

### For Operators
- [Production Readiness](deployment/PRODUCTION_READINESS.md) - Complete ops manual
- [24H Deployment Guide](deployment/24H_DEPLOYMENT_GUIDE.md) - Quick deploy
- [Failure Modes](deployment/failure-modes.md) - Troubleshooting guide
- [Paper Trading Realism](deployment/PAPER_TRADING_REALISM.md) - Testing guide

### For Performance Engineers
- [Benchmark Results](benchmarks/LATEST.md) - Latest performance data
- [Latency Budget](benchmarks/latency-budget.md) - Component breakdown
- [Benchmark Guide](benchmarks/README.md) - How to run benchmarks

## By Topic

### Architecture
Core system design and implementation patterns

| Document | Lines | Purpose |
|----------|-------|---------|
| [system-design.md](architecture/system-design.md) | 753 | Zero-overhead architecture |
| [STATE_MACHINES.md](architecture/STATE_MACHINES.md) | 586 | Typestate FSM patterns |
| [overflow-handling.md](architecture/overflow-handling.md) | 352 | Safety architecture |
| [fill-processing-trace.md](architecture/fill-processing-trace.md) | 418 | Order lifecycle trace |

**Key concepts**: Zero-cost abstractions, compile-time safety, cache optimization

### Benchmarks
Latency analysis and performance results

| Document | Lines | Purpose |
|----------|-------|---------|
| [LATEST.md](benchmarks/LATEST.md) | - | Most recent benchmark results |
| [latency-budget.md](benchmarks/latency-budget.md) | 803 | Component latency targets |
| [results/README.md](benchmarks/results/README.md) | - | All benchmark runs over time |
| [README.md](benchmarks/README.md) | - | Benchmark guide and structure |

**Latest Results**: 70.79ns tick-to-trade (14.1x under 1μs target)

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
| [trait-implementation-guide.md](guides/trait-implementation-guide.md) | ~600 | Custom Strategy/Executor guide |
| [error-handling-guide.md](guides/error-handling-guide.md) | ~450 | Production error patterns |
| [monitoring-guide.md](guides/monitoring-guide.md) | ~550 | Metrics and alerts |
| [command-reference.md](guides/command-reference.md) | 134 | CLI commands |
| [market-selection.md](guides/market-selection.md) | 77 | Market configuration |

**Best for**: Learning the system, daily operations, production deployment

### Integration
External system connections

| Document | Lines | Purpose |
|----------|-------|---------|
| [HUGINN_INTEGRATION_GUIDE.md](HUGINN_INTEGRATION_GUIDE.md) | 376 | Shared memory IPC |

**Dependency**: Huginn v0.4.0+ for market data


---

##  By Use Case

### "I want to run the bot for the first time"
1. [README.md](../README.md) - Quick start
2. [24H Deployment Guide](deployment/24H_DEPLOYMENT_GUIDE.md) - Step-by-step
3. [Command Reference](guides/command-reference.md) - CLI usage

### "I want to understand how it makes money"
1. [Market Making Guide](guides/market-making-guide.md) - Complete strategy explanation

### "I want to deploy to production"
1. [Production Readiness](deployment/PRODUCTION_READINESS.md) - Pre-flight checklist
2. [Monitoring Guide](guides/monitoring-guide.md) - Metrics and alerts
3. [Error Handling Guide](guides/error-handling-guide.md) - Critical error patterns
4. [Failure Modes](deployment/failure-modes.md) - What can go wrong
5. [Latest Benchmarks](benchmarks/LATEST.md) - Verify targets

### "I want to modify the code"
1. [System Design](architecture/system-design.md) - Architecture overview
2. [Trait Implementation Guide](guides/trait-implementation-guide.md) - Custom strategies/executors
3. [State Machines](architecture/STATE_MACHINES.md) - Safety patterns
4. [Project Roadmap](PROJECT_ROADMAP.md) - Current status

### "Something went wrong"
1. [Error Handling Guide](guides/error-handling-guide.md) - Critical error recovery
2. [Monitoring Guide](guides/monitoring-guide.md) - Health check and runbooks
3. [Failure Modes](deployment/failure-modes.md) - Troubleshooting scenarios
4. [Command Reference](guides/command-reference.md) - Debug commands
5. [Production Readiness](deployment/PRODUCTION_READINESS.md) - Emergency procedures

## Quick Reference

### Performance Targets
| Component | Budget | Measured | Status |
|-----------|--------|----------|--------|
| Tick-to-trade | <1μs | 70.79ns | 14x under |
| Strategy calc | <100ns | 17.28ns | 5.8x under |
| Risk validation | <50ns | 2.37ns | 21x under |
| Orderbook sync | <50ns | ~20ns | 2.5x under |

### Safety Layers
1. Compile-time spread validation (won't compile if unprofitable)
2. Market data validation (spread, liquidity, prices)
3. Position limits (max 1.0 BTC)
4. Daily loss limits (max $1,000)
5. Circuit breaker (>10% move halts)
6. Rate limiter (10 orders/sec)
7. Pre-trade validation (6 checks)
8. Kill switch (SIGUSR1)

### File Organization
```
docs/
 INDEX.md                    ← You are here
 README.md                   ← Start here
 PROJECT_ROADMAP.md          ← Development plan
 HUGINN_INTEGRATION_GUIDE.md ← Market data
 architecture/               ← Design docs
 benchmarks/                 ← Performance results
 deployment/                 ← Operations
 guides/                     ← User guides
 design/                     ← Historical
```

## Document Status

All documents current unless noted otherwise.

## Contributing to Documentation

When adding or updating docs:
1. Add header block: Purpose, Audience, Prerequisites, Related
2. Add TL;DR section for LLM quick reference
3. Update this index with new document
4. Fix all cross-references to use relative paths

## External Resources

- Huginn Repository: `../../huginn/` (sibling repo)
- Lighter DEX Docs: https://docs.lighter.xyz
- Rust Performance Book: https://nnethercote.github.io/perf-book/

---

**Last Updated**: 2025-11-26
