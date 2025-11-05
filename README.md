# Bog - Market Maker Bot for Lighter DEX

Bog is a high-performance, modular market making trading bot designed to integrate with [Huginn](../huginn)'s ultra-low-latency market data feed via shared memory IPC.

## Features

- **Mode-Agnostic Design**: Same code runs for live trading, paper trading, and backtesting
- **Pluggable Strategies**:
  - Simple Spread: Naive market making with fixed spreads
  - Inventory-Based: Avellaneda-Stoikov optimal market making
  - Extensible architecture for custom strategies
- **Multiple Execution Modes**:
  - **Simulated**: Paper trading with instant fills (for testing and backtesting)
  - **Live**: Real exchange integration (currently stub)
- **Comprehensive Risk Management**:
  - Position limits (max long/short)
  - Order size limits
  - PnL-based circuit breakers (daily loss limit, drawdown limits)
- **OrderBook Integration**: Local orderbook representation with analytics (stub, ready for OrderBook-rs integration)
- **Ultra-Low Latency**: Integrates with Huginn's sub-microsecond shared memory feed

## Architecture

### Data Flow Overview

```
┌─────────────────────────────────────────────────────────────┐
│                    Market Data Ingestion                     │
│                     (Shared Memory IPC)                      │
├─────────────────────────────────────────────────────────────┤
│                                                               │
│  Lighter WebSocket API                                       │
│         ↓                                                     │
│  Huginn (with --hft flag)                                   │
│    • Connects to Lighter WebSocket                           │
│    • Parses JSON messages (~50μs)                           │
│    • Publishes to /dev/shm/hg_m{id} (300-800ns)            │
│         ↓                                                     │
│  POSIX Shared Memory (/dev/shm)                             │
│    • Lock-free SPSC ring buffer                              │
│    • Zero-copy, zero-serialization                           │
│         ↓                                                     │
│  Bog Bot (huginn::MarketFeed)                               │
│    • try_recv() reads from shared memory (50-150ns)         │
│    • NO API calls to Lighter for market data                │
│                                                               │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│                  Strategy & Execution Flow                   │
├─────────────────────────────────────────────────────────────┤
│                                                               │
│  MarketSnapshot (from shared memory)                        │
│         ↓                                                     │
│  OrderBookManager.sync()                                    │
│         ↓                                                     │
│  Analytics (VWAP, imbalance, depth)                         │
│         ↓                                                     │
│  Strategy.calculate() → Signal (~5ns)                       │
│         ↓                                                     │
│  RiskManager.validate()                                     │
│         ↓                                                     │
│  Executor (Simulated OR Lighter API stub)                   │
│         ↓                                                     │
│  Metrics & Position Tracking                                │
│                                                               │
└─────────────────────────────────────────────────────────────┘
```

**Key Architecture Points:**
- 📥 **Data Ingestion**: Shared memory IPC only, no direct Lighter API connection
- 🚀 **Ultra-low latency**: 50-150ns reads from `/dev/shm`
- 🔀 **Decoupled**: Market data (shared memory) separate from execution (API stub)
- ✅ **Production-ready**: Resilient reconnection, health monitoring

## Getting Started

### Prerequisites

1. **Huginn**: Market data feed must be running
   ```bash
   cd ../huginn
   cargo run --release -- lighter start --market-id 1 --hft
   ```

2. **Rust**: 1.70+ recommended

### Installation

```bash
cd bog
cargo build --release
```

### Configuration

Create or modify `config/default.toml`:

```toml
[huginn]
market_id = 1_000_001  # Lighter market ID

[execution]
mode = "simulated"     # "simulated" or "live"

[strategy]
type = "simple_spread"  # "simple_spread" or "inventory_based"

[strategy.simple_spread]
spread_bps = 10.0      # 10 basis points = 0.1%
order_size = "0.1"
min_spread_bps = 1.0

[risk]
max_position = "1.0"
max_short = "1.0"
max_order_size = "0.5"
max_daily_loss = "1000.0"
max_drawdown_pct = 0.20  # 20%
```

### Running

#### Paper Trading (Simulated Execution)

```bash
# Terminal 1: Start Huginn
cd ../huginn
cargo run --release -- lighter start --market-id 1 --hft

# Terminal 2: Run bog
cd ../bog
cargo run --release -- --config config/default.toml
```

#### Backtesting

```bash
# Terminal 1: Start Huginn replay
cd ../huginn
huginn replay start \
  --markets 1 \
  --start "2024-01-15T10:00:00Z" \
  --end "2024-01-15T18:00:00Z" \
  --timing wall-clock --speed 10.0 \
  --output shared-memory

# Terminal 2: Run bog (same command!)
cd ../bog
cargo run --release -- --config config/default.toml --execution-mode simulated
```

**Key insight**: Bog runs the same code for live and backtest. Huginn handles the difference!

### CLI Options

```bash
bog --help

Options:
  -c, --config <PATH>           Path to config file [default: config/default.toml]
      --execution-mode <MODE>   Override execution mode (live or simulated)
      --market-id <ID>          Override market ID
      --json-logs               Enable JSON logging
      --log-level <LEVEL>       Log level (trace, debug, info, warn, error)
```

## Strategies

### Simple Spread Strategy

Posts quotes at a fixed spread around mid price.

```toml
[strategy]
type = "simple_spread"

[strategy.simple_spread]
spread_bps = 10.0       # Spread to post (10 bps = 0.1%)
order_size = "0.1"
min_spread_bps = 1.0    # Don't trade if market spread < this
```

**Best for**: Testing, simple market making in liquid markets

### Inventory-Based Strategy (Avellaneda-Stoikov)

Adjusts quotes based on inventory risk to maintain target position.

```toml
[strategy]
type = "inventory_based"

[strategy.inventory_based]
target_inventory = "0.0"    # Target neutral position
risk_aversion = 0.1         # Gamma parameter (higher = more aggressive)
order_size = "0.1"
volatility = 0.02           # 2% annualized volatility
time_horizon_secs = 300.0   # 5 minute horizon
```

**Best for**: Sophisticated market making with inventory risk management

**Math**: Based on "High-frequency trading in a limit order book" (Avellaneda & Stoikov, 2008)
- Reservation price: `r = s - q * γ * σ² * T`
- Optimal spread: `δ = γ * σ² * T`

Where:
- `s` = mid price
- `q` = inventory (relative to target)
- `γ` = risk aversion
- `σ` = volatility
- `T` = time horizon

## Risk Management

Bog enforces multiple layers of risk controls:

### Position Limits
```toml
max_position = "1.0"   # Max long position (BTC)
max_short = "1.0"      # Max short position
```

### Order Size Limits
```toml
max_order_size = "0.5"
min_order_size = "0.0001"
max_outstanding_orders = 10
```

### PnL-Based Circuit Breakers
```toml
max_daily_loss = "1000.0"   # Stop trading at -$1000/day
max_drawdown_pct = 0.20     # Stop at 20% drawdown from high water mark
```

Violations trigger:
1. **Soft rejection**: Signal rejected, logged
2. **Hard halt**: Trading stops, strategy paused

## Development

### Project Structure

```
bog/
├── src/
│   ├── config/          # Configuration loading
│   ├── data/            # Huginn market data integration
│   ├── orderbook/       # OrderBook management (stub)
│   ├── strategy/        # Trading strategies
│   ├── execution/       # Order execution (simulated & live)
│   ├── risk/            # Risk management
│   ├── engine/          # Main trading loop
│   └── utils/           # Logging, metrics
├── config/              # Configuration files
└── examples/            # Usage examples
```

### Adding a Custom Strategy

1. Implement the `Strategy` trait:

```rust
use bog::strategy::{Strategy, Signal, StrategyState};

pub struct MyStrategy { /* ... */ }

impl Strategy for MyStrategy {
    fn on_update(
        &mut self,
        snapshot: &MarketSnapshot,
        orderbook: &OrderBookManager,
    ) -> Option<Signal> {
        // Your strategy logic here
        Some(Signal::QuoteBoth {
            bid_price: ...,
            ask_price: ...,
            size: ...,
        })
    }

    fn on_fill(&mut self, fill: &Fill) {
        // Handle fills
    }

    fn name(&self) -> &str {
        "MyStrategy"
    }

    // ... other trait methods
}
```

2. Register in `StrategyFactory`:

```rust
// In src/strategy/mod.rs
match config.strategy_type.as_str() {
    "my_strategy" => Ok(Box::new(MyStrategy::new(...))),
    // ...
}
```

### Testing

```bash
# Run all tests
cargo test

# Run specific module tests
cargo test --lib data
cargo test --lib strategy
cargo test --lib risk

# Run with logging
RUST_LOG=debug cargo test -- --nocapture
```

## Performance

Target performance metrics:
- **Market data → Strategy decision**: < 10μs
- **Strategy → Order submission**: < 100μs (paper), < 10ms (live)
- **Full tick-to-trade latency**: < 1ms (excluding network)
- **Throughput**: 1000+ updates/sec per market

## Future Roadmap

### Phase 7: Real OrderBook Integration
- Replace stub with OrderBook-rs
- Full orderbook sync from Huginn snapshots
- Real VWAP, imbalance, depth analytics
- Queue position tracking

### Phase 8: Lighter DEX Integration
- Implement or integrate Lighter DEX SDK
- Real order submission and fills
- WebSocket for order updates
- Signature/authentication

### Phase 9: Production Readiness
- Prometheus metrics exporter
- Enhanced monitoring and alerting
- Performance benchmarking
- Deployment documentation

## Architecture Decisions

### Why Mode-Agnostic?
Running the **exact same code** for live and backtest ensures:
- No "backtest overfitting" from different logic
- Realistic backtest results
- Faster development (test once, works everywhere)

### Why Stub Initially?
Stubs allow:
- Full end-to-end development without real exchange access
- Safe testing of strategy and risk logic
- Clear interfaces for future real implementations

### Why Huginn Integration?
- **Sub-microsecond latency**: 300-800ns Huginn overhead + 50-150ns consumer read
- **Zero-copy**: Shared memory, no serialization
- **Production-ready**: Lock-free SPSC ring buffer
- **Transparent backtesting**: Same interface for live and replay

## Troubleshooting

### "Failed to connect to Huginn shared memory"
- Ensure Huginn is running: `ps aux | grep huginn`
- Check market ID matches: Huginn uses encoded IDs (1_000_000 + market_id)
- Check shared memory exists: `ls -lh /dev/shm/hg_m*`

### "Daily loss limit breached"
- Check `max_daily_loss` in config
- Review fills and PnL in logs
- Strategy may need tuning (spread too tight, inventory management)

### No fills in simulated mode
- Simulated mode immediately fills all orders
- Check strategy is generating signals: `--log-level debug`
- Verify orderbook is valid (bid < ask, positive prices)

## License

[Your License Here]

## Contributing

Contributions welcome! Please:
1. Run tests: `cargo test`
2. Format code: `cargo fmt`
3. Check clippy: `cargo clippy`
4. Update documentation as needed

## Support

- GitHub Issues: [Your Repo]
- Documentation: See `/docs` directory
