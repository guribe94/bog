# bog-bins

**Production trading binaries**

Pre-configured HFT trading binaries with full compile-time monomorphization for maximum performance.

## Available Binaries

### bog-simple-spread-simulated

**SimpleSpread strategy with paper trading**

```bash
cargo run --release --bin bog-simple-spread-simulated -- --market-id 1
```

Features:
- ✅ Instant fills (simulated)
- ✅ 2 bps taker fee accounting
- ✅ Full position tracking
- ✅ Realistic spread/size configuration

**Use case**: Strategy testing, backtesting, paper trading

### bog-simple-spread-paper

**SimpleSpread with Lighter DEX paper trading API**

```bash
cargo run --release --bin bog-simple-spread-paper -- --market-id 1
```

Features:
- ✅ Real Lighter DEX API (paper mode)
- ✅ Realistic order acknowledgement
- ✅ Fill simulation based on market
- ✅ No actual funds at risk

**Use case**: Pre-production testing with real exchange API

### bog-inventory-simulated

**InventoryBased strategy with paper trading**

```bash
cargo run --release --bin bog-inventory-simulated -- --market-id 1
```

**Status**: 🚧 Strategy stub - awaits Phase 4 implementation

### bog-simple-spread-live

**LIVE TRADING - SimpleSpread on Lighter DEX**

```bash
# ⚠️  REQUIRES REAL FUNDS AND PRIVATE KEYS ⚠️
cargo run --release --bin bog-simple-spread-live -- \
    --market-id 1 \
    --cpu-core 3 \
    --realtime
```

Features:
- 🔴 Real funds at risk
- 🔴 Requires API credentials
- ✅ Sub-microsecond latency
- ✅ Full monitoring and alerts

**Use case**: Production trading (use with extreme caution)

## Binary Selection

### Decision Tree

```
Do you want to risk real funds?
│
├─ NO → Do you want realistic exchange API?
│       ├─ YES → bog-simple-spread-paper
│       └─ NO → bog-simple-spread-simulated
│
└─ YES → bog-simple-spread-live
         ⚠️  PRODUCTION - REQUIRES SETUP ⚠️
```

### Quick Comparison

| Binary | Strategy | Execution | API Calls | Risk | Latency |
|--------|----------|-----------|-----------|------|---------|
| **simulated** | SimpleSpread | Simulated | None | ✅ None | ~380ns |
| **paper** | SimpleSpread | Paper | Real | ✅ None | ~2-5ms |
| **live** | SimpleSpread | Live | Real | 🔴 Real | ~400ns-2ms |
| **inventory-simulated** | Inventory | Simulated | None | ✅ None | TBD |

## CLI Reference

### Common Arguments

All binaries support these arguments:

```bash
# Basic usage
--market-id <ID>        Market ID to trade (default: 1)

# Performance tuning
--cpu-core <CORE>       Pin to specific CPU core (e.g., 3)
--realtime              Enable SCHED_FIFO real-time priority (Linux only)

# Monitoring
--metrics               Enable Prometheus metrics output
--log-level <LEVEL>     Set log level: trace, debug, info, warn, error (default: info)
```

### Examples

```bash
# Basic run with default settings
cargo run --release --bin bog-simple-spread-simulated

# High-performance setup
cargo run --release --bin bog-simple-spread-simulated -- \
    --market-id 1 \
    --cpu-core 3 \
    --realtime \
    --log-level warn

# With metrics
cargo run --release --bin bog-simple-spread-simulated -- \
    --metrics \
    --log-level info
```

## Build Configuration

### Strategy Configuration

Strategies are configured via Cargo features at compile-time:

```toml
[dependencies]
bog-strategies = { features = [
    "spread-10bps",      # 10 basis point spread
    "size-medium",       # 0.1 BTC orders
    "min-spread-1bps",   # Min market spread to trade
] }
```

### Risk Configuration

Risk limits are configured in bog-core:

```toml
[dependencies]
bog-core = { features = [
    "max-position-one",     # Max 1.0 BTC position
    "max-order-half",       # Max 0.5 BTC per order
    "max-daily-loss-1000",  # Max $1000 daily loss
] }
```

### Pre-configured Profiles

Use meta-features for common setups:

```bash
# Conservative profile
cargo build --release --features conservative

# Aggressive profile
cargo build --release --features aggressive

# Testing profile (small limits)
cargo build --release --features testing
```

See [bog-core/Cargo.toml](../bog-core/Cargo.toml#L158-L162) for profile definitions.

## Performance Optimization

### Release Build

**Always use --release for trading**:

```bash
cargo build --release --bin bog-simple-spread-simulated
```

Debug builds are ~100x slower and unsuitable for HFT.

### CPU Isolation

For minimum latency jitter:

```bash
# 1. Isolate CPU cores (kernel boot param)
isolcpus=2,3

# 2. Pin trading binary to isolated core
./target/release/bog-simple-spread-simulated --cpu-core 3 --realtime
```

### Huge Pages (Linux)

Reduce TLB misses:

```bash
# Allocate 128MB huge pages
echo 64 > /proc/sys/vm/nr_hugepages

# Run binary (automatic huge page usage if available)
./target/release/bog-simple-spread-simulated
```

### Turbo Boost (Intel)

Disable for consistent latency:

```bash
echo 0 > /sys/devices/system/cpu/intel_pstate/no_turbo
```

## Output

### Successful Run

```
=== Bog: Simple Spread + Simulated Executor ===
Market ID: 1
Strategy size: 0 bytes
Using HFT SimulatedExecutor (instant fills with fee accounting)
Starting engine...

=== Final Statistics ===
Ticks processed: 1000
Signals generated: 987
Final position: +0.05 BTC
Realized PnL: $12.50
Signal rate: 98.70%
```

### Common Issues

**Error: "Failed to connect to Huginn"**
- **Cause**: Huginn market data feed not running
- **Solution**: Start Huginn first: `huginn lighter --market-id 1`

**Error: "Permission denied (real-time priority)"**
- **Cause**: Insufficient privileges for SCHED_FIFO
- **Solution**: Run with `sudo` or set CAP_SYS_NICE capability

**Error: "CPU pinning failed"**
- **Cause**: Invalid core number or macOS (not supported)
- **Solution**: Use valid core number or remove `--cpu-core` on macOS

## Development

### Adding a New Binary

1. Create file in `src/bin/my_strategy_simulated.rs`
2. Copy template from existing binary
3. Change strategy type and executor
4. Add to `Cargo.toml` `[[bin]]` section

Example:

```rust
use bog_core::engine::{Engine, SimulatedExecutor};
use bog_strategies::MyStrategy;

fn main() -> anyhow::Result<()> {
    let args = CommonArgs::parse();
    init_logging(&args.log_level)?;

    let strategy = MyStrategy;
    let executor = SimulatedExecutor::new_default();
    let mut engine = Engine::new(strategy, executor);

    let stats = engine.run(create_test_feed(args.market_id))?;
    print_stats(&stats);

    Ok(())
}
```

### Testing

```bash
# Test all binaries compile
cargo build --bins --release

# Run simulated binary
./target/release/bog-simple-spread-simulated --market-id 1

# Check performance
cargo build --release && \
    time ./target/release/bog-simple-spread-simulated --log-level error
```

## Monitoring

### Metrics Output

Enable with `--metrics`:

```bash
./target/release/bog-simple-spread-simulated --metrics
```

Outputs:
- Ticks per second
- Signal generation rate
- Position updates
- Fill latency
- PnL tracking

### Logs

Control verbosity:

```bash
# Quiet (errors only)
--log-level error

# Normal (info + warnings + errors)
--log-level info

# Verbose (debug + all above)
--log-level debug

# Everything (trace + all above)
--log-level trace
```

## Production Deployment

See [PRODUCTION_READINESS.md](../docs/PRODUCTION_READINESS.md) for complete guide.

### Checklist

- [ ] Review [24H_DEPLOYMENT_GUIDE.md](../docs/24H_DEPLOYMENT_GUIDE.md)
- [ ] Configure risk limits appropriately
- [ ] Set up monitoring and alerts
- [ ] Test with paper trading first
- [ ] Verify API credentials
- [ ] Set up CPU isolation
- [ ] Configure logging and metrics collection
- [ ] Review failure modes

## Documentation

- **[Production Guide](../docs/deployment/PRODUCTION_READINESS.md)** - Complete deployment manual
- **[Performance Benchmarks](../docs/benchmarks/LATEST.md)** - Latest results
- **[Failure Modes](../docs/deployment/failure-modes.md)** - Troubleshooting guide

## License

MIT - See [LICENSE](../LICENSE) for details.
