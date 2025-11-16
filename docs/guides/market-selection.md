# Market Selection

Bot supports any market via `--market-id` CLI argument. Must match between Huginn and Bog.

## Examples

```bash
# Market 1 (recommended for testing)
./target/release/bog-simple-spread-simulated --market-id 1

# Market 5
./target/release/bog-simple-spread-simulated -m 5

# Any market with short form
./target/release/bog-simple-spread-simulated -m 42
```

## Shared Memory Path

Format: `/dev/shm/hg_m1{MARKET_ID:06}`

| Market | Path | Command |
|--------|------|---------|
| 1 | `/dev/shm/hg_m1000001` | `--market-id 1` |
| 2 | `/dev/shm/hg_m1000002` | `--market-id 2` |
| 42 | `/dev/shm/hg_m1000042` | `--market-id 42` |

## Deployment Pattern

**Single market (recommended for 24h test):**

```bash
# Terminal 1: Huginn
./target/release/huginn lighter start --market-id 1 --hft

# Terminal 2: Bot
./target/release/bog-simple-spread-simulated --market-id 1
```

**Multiple markets with multiple bots:**

```bash
# Terminal 1: Huginn (publishing markets 1, 2, 5)
./target/release/huginn lighter start --market-ids 1,2,5 --hft

# Terminal 2, 3, 4: Independent bot instances
./target/release/bog-simple-spread-simulated -m 1  # Market 1
./target/release/bog-simple-spread-simulated -m 2  # Market 2
./target/release/bog-simple-spread-simulated -m 5  # Market 5
```

## Verification

```bash
# Check Huginn is publishing
ls -lh /dev/shm/hg_m*

# Verify market has activity (optional)
huginn lighter markets
```

## Common Issue: Market ID Mismatch

```bash
# Huginn on market 1, but bot tries market 2
./target/release/huginn lighter start --market-id 1 --hft
./target/release/bog-simple-spread-simulated --market-id 2  # WRONG - will fail
```

**Solution:** Match market IDs between Huginn and bot.

## For Production

Test strategy on the intended market - different markets have different characteristics:
- Spread ranges
- Liquidity depth
- Volatility patterns
