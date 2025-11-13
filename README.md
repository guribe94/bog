# Bog - High-Frequency Market Making Bot

**Status:** Production-Ready for Paper Trading (85%)  
**Target Platform:** Lighter DEX  
**Latency:** 70.79ns tick-to-trade (measured)

## Overview

Bog is a high-frequency trading market maker for cryptocurrency exchanges. The system uses compile-time verified state machines to prevent financial loss bugs and achieves sub-100ns application latency.

## Quick Start

```bash
# Build all components
cargo build --release

# Start Huginn (market data feed)
cd ../huginn && ./target/release/huginn --market 1 --hft

# Run paper trading
cd bog
./target/release/bog-simple-spread-simulated --market 1
```

## Architecture

- **bog-core**: Trading engine, state machines, execution
- **bog-strategies**: Market making strategies (SimpleSpread, InventoryBased)
- **bog-bins**: Executable binaries
- **bog-debug**: Visualization and debugging tools

## Key Features

- **Typestate FSMs**: Invalid state transitions prevented at compile time
- **L2 Orderbook**: Full 10-level depth tracking from Huginn
- **Sub-100ns Latency**: 70.79ns measured tick-to-trade
- **Comprehensive Safety**: 6 layers of validation, kill switch, rate limiter
- **Fee Accounting**: Realistic paper trading with 2 bps taker fees

## Performance (Measured)

| Component | Latency |
|-----------|---------|
| Tick-to-trade | 70.79ns |
| Strategy calculation | 17.28ns |
| Risk validation | 2.37ns |
| Orderbook sync | ~20ns |

Target: <1 microsecond (14.1x headroom)

## Documentation

- `DEPLOYMENT_CHECKLIST.md` - Pre-deployment verification
- `SECURITY_AUDIT_REPORT.md` - Complete security audit
- `STATE_MACHINES.md` - Typestate pattern guide
- `MEASURED_PERFORMANCE_COMPLETE.md` - Benchmark results
- `PRODUCTION_READINESS.md` - Operations manual

## Testing

```bash
cargo test --release    # Unit tests (408 tests)
cargo bench            # Performance benchmarks (25+ operations)
```

## Status

**Ready:** Paper trading with SimulatedExecutor  
**Not Ready:** Live trading (Lighter SDK integration needed)  
**Timeline:** 3-4 weeks to production deployment

## License

MIT
