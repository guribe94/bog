# 24-Hour Paper Trading Deployment

## Prerequisites

- **Huginn** must be running: `./target/release/huginn lighter start --market-id 1 --hft`
- **Rust** 1.70+ (for building)
- **4GB+ RAM**, **10GB+ disk** for logs
- **macOS or Linux**

## Pre-Deployment

1. **Build and test:**
   ```bash
   cargo build --release
   cargo test --test engine_integration
   cargo test --test position_update_unit
   ```

2. **Verify Huginn is running:**
   ```bash
   ls -lh /dev/shm/hg_m1000001  # Should exist
   ps aux | grep huginn
   ```

3. **Review configuration** (Cargo.toml features):
   - MAX_POSITION: 1.0 BTC
   - MAX_SHORT: 1.0 BTC
   - MAX_DAILY_LOSS: $1000 USD
   - Strategy: SimpleSpread, 10 bps spread, 0.1 BTC order size

## Deployment

```bash
# Create log directory
mkdir -p ~/bog-logs

# Build if not already done
cargo build --release

# Start bot
RUST_LOG=info ./target/release/bog-simple-spread-simulated --market-id 1 \
    2>&1 | tee ~/bog-logs/run-$(date +%Y%m%d-%H%M%S).log
```

Or run in background:

```bash
nohup ./target/release/bog-simple-spread-simulated --market-id 1 \
    > ~/bog-logs/run-$(date +%Y%m%d-%H%M%S).log 2>&1 &
echo $! > ~/bog-logs/bot.pid
```

## Execution Mode

**Real market data** (from Huginn → Lighter DEX)
**Simulated execution** (not sent to exchange)

- Fill probability: 40-80% based on queue position
- Partial fills: Enabled
- Fees: 2 bps per fill
- Slippage: 2 bps per fill
- Network latency: 4ms (simulated round-trip)
- Exchange latency: 3ms (simulated matching)

See [Paper Trading Realism](PAPER_TRADING_REALISM.md) for details on what is/isn't simulated.

## Expected Results

**Throughput:** ~60% fill rate on average (lower if market moves quickly)

**Latency:** ~10-15ms per tick (application ~70ns + simulated delays)

**Profitability:**
- Spread captured: 10 bps
- Less fees: 2 bps
- Less slippage: 2 bps
- **Expected profit: 6 bps per successful round-trip**

If unprofitable, check:
- Fill imbalance (buying more than selling)
- Market moving against positions
- Fee/slippage calculations

## Monitoring

```bash
# View logs
tail -f ~/bog-logs/run-*.log

# Monitor for errors
tail -f ~/bog-logs/run-*.log | grep -i error

# Check process
ps aux | grep bog-simple-spread-simulated

# Extract performance
grep "Tick latency:" ~/bog-logs/run-*.log | tail -20
```

## Shutdown

**Graceful:**
```bash
kill $(cat ~/bog-logs/bot.pid)
```

**Emergency:**
```bash
kill -KILL $(pgrep -f bog-simple-spread-simulated)
```

## Troubleshooting

### Bot won't connect
```bash
# Verify Huginn is running on market 1
ps aux | grep huginn

# Start Huginn if missing
./target/release/huginn lighter start --market-id 1 --hft
```

### Compilation fails
```bash
cargo clean && cargo build --release
cargo update  # Update dependencies if needed
```

### No fills occurring
- Verify market has trading activity
- Check fill probability is reasonable
- Confirm strategy is generating signals

### High latency
- Check system load (CPU, memory, disk I/O)
- Verify no competing processes
- Check for thermal throttling
