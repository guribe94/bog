# Bog - High-Frequency Market Making Bot

**Latency:** 70.79ns tick-to-trade (measured)
**Target:** <1μs (14.1x headroom)
**Status:** Production-ready for paper trading

## Overview

HFT market maker for cryptocurrency exchanges. Uses compile-time verified state machines and zero-overhead abstractions for sub-microsecond latency.

## Quick Start

```bash
# Build
cargo build --release

# Start market data feed (Huginn)
cd ../huginn && ./target/release/huginn lighter --market 1

# Run paper trading
cd bog && ./target/release/bog-simple-spread-simulated --market 1
```

## Architecture

- **bog-core**: Trading engine, risk management, orderbook
- **bog-strategies**: Market making strategies (ZST-based)
- **bog-bins**: Executable binaries
- **bog-debug**: Visualization tools

## Key Features

- Sub-microsecond tick-to-trade latency (70.79ns measured)
- Compile-time state machines (typestate FSMs)
- Zero-overhead abstractions (const generics, ZSTs)
- Lock-free atomic position tracking
- Comprehensive safety (6 validation layers)
- Realistic paper trading with fee accounting

## Performance

| Component | Measured |
|-----------|----------|
| Tick-to-trade | 70.79ns |
| Strategy | 15-18ns |
| Risk validation | 2.18ns |

See [docs/benchmarks/LATEST.md](docs/benchmarks/LATEST.md) for full results.

## Documentation

**[→ Complete Documentation](docs/README.md)**

Quick links:
- [Architecture](docs/architecture/system-design.md) - Zero-overhead design
- [Benchmarks](docs/benchmarks/LATEST.md) - Performance results
- [Production Guide](docs/deployment/PRODUCTION_READINESS.md) - Operations
- [Roadmap](docs/PROJECT_ROADMAP.md) - Development phases

## Testing

```bash
cargo test --release    # 408 tests
cargo bench            # Performance benchmarks
```

## Status

**Production-ready:** Paper trading (SimulatedExecutor)
**Pending:** Live trading (requires Lighter SDK)

## License

MIT
