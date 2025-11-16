# Command Reference

## Build & Test

```bash
# Release build (optimized)
cargo build --release

# Run tests
cargo test

# Run benchmarks
cargo bench

# Check for warnings
cargo clippy

# Format code
cargo fmt
```

## Running the Bot

**Prerequisite:** Huginn must be running on the same market ID

```bash
# Paper trading on market 1
./target/release/bog-simple-spread-simulated --market 1

# With logging
RUST_LOG=info ./target/release/bog-simple-spread-simulated --market 1 \
    2>&1 | tee ~/logs/bog-run-$(date +%Y%m%d-%H%M%S).log

# Background execution
nohup ./target/release/bog-simple-spread-simulated --market 1 \
    > ~/logs/bog-run-$(date +%Y%m%d-%H%M%S).log 2>&1 &
```

## Troubleshooting

### Bot Won't Connect to Market Data

```bash
# Verify Huginn is running
ps aux | grep huginn

# Start Huginn (if not running)
cd ../huginn && ./target/release/huginn --market 1 --hft
```

### Compilation Errors

```bash
# Clean rebuild
cargo clean && cargo build --release

# Update dependencies
cargo update
```

### Runtime Issues

```bash
# Monitor logs for errors
tail -f ~/logs/bog-run-*.log | grep -i error

# Check process is running
ps aux | grep bog-simple-spread-simulated

# Kill bot gracefully
kill $(pgrep -f bog-simple-spread-simulated)

# Emergency stop
kill -KILL $(pgrep -f bog-simple-spread-simulated)
```

## Development

```bash
# Add feature to Cargo.toml, then rebuild
cargo build --release --features feature-name

# Run specific test
cargo test test_name -- --nocapture

# Run with debug output
RUST_LOG=debug cargo run --release --bin simple-spread-simulated -- --market 1

# Generate documentation
cargo doc --open
```

## Common Arguments

```bash
# View all available options
./target/release/bog-simple-spread-simulated --help

# Market selection (required)
--market 1
-m 1
```

## Performance Analysis

```bash
# Extract latency measurements from logs
grep "Tick latency:" logfile | awk '{print $NF}' | sort -n

# Count errors in log
grep -i "error" logfile | wc -l

# Find ERROR or CRITICAL messages
grep -E "ERROR|CRITICAL" logfile

# Monitor position in real-time
tail -f logfile | grep "POSITION"
```

## Monitoring

```bash
# View Prometheus metrics (if running)
curl http://localhost:9090/metrics

# Check system resources while bot runs
top -p $(pgrep -f bog-simple-spread-simulated)

# Memory usage
free -h

# Disk space
df -h
```
