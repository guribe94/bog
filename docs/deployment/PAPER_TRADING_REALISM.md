# Paper Trading Realism

## What IS Simulated

| Feature | Status | Details |
|---------|--------|---------|
| **Market Data** | Real | From Huginn → Lighter DEX |
| **Fee Accounting** | Yes | 2 bps per fill (Lighter DEX rate) |
| **Partial Fills** | Yes | 40-80% fill probability (back-of-queue assumption) |
| **Slippage** | Yes | 2 bps on all fills |
| **Queue Position** | Basic | FIFO, assumes back-of-queue |
| **Network Latency** | Yes | 4ms round-trip (measured production) |
| **Exchange Latency** | Yes | 3ms matching/fill generation |

**Realism Score:** 8.5/10

## What's NOT Simulated

1. **Market Impact** - Large orders don't move the market. Slippage is fixed regardless of order size.
2. **Order Book Depth** - Doesn't consume order book, no depth simulation, simplified queue modeling.
3. **Latency Variance** - Uses fixed delays, not variable/jitter-based.
4. **Rejections** - Orders never rejected (no margin checks, rate limits, invalid params).
5. **Maker/Taker Distinction** - All fills treated as taker (2 bps fee). Maker fills would be cheaper (0.2 bps).

## Configuration Modes

### Instant Mode (Development/Debug)
- 100% fill rate, no partial fills, no slippage, no queue modeling
- Fee accounting: Still enabled (2 bps)
- Use for: Development, unit testing, performance benchmarking

### Realistic Mode (Paper Trading) - CURRENT
- 40-80% fill rate based on queue position
- Partial fills enabled
- 2 bps slippage
- 2 bps fee accounting
- Latency: 4ms network + 3ms exchange = 7ms per order, ~14ms per tick
- Use for: 24-hour paper trading, strategy development

### Conservative Mode (Stress Testing)
- 20-60% fill rate (lower than realistic)
- 5 bps slippage (higher than realistic)
- Same fee accounting, queue modeling
- Use for: Worst-case stress testing, risk validation

## Expected Results in 24-Hour Run

**Fill Rates:** ~60% average (40-80% range)
- Some orders won't fill at all
- Fewer fills than instant mode but more realistic

**Profitability Math:**
- Spread captured: 10 bps
- Fees: 2 bps per round-trip
- Slippage: 2 bps per round-trip
- **Expected profit: 6 bps per successful round-trip**

**Position Building:**
- Slower accumulation than instant mode
- More realistic inventory management
- Position should respect 1 BTC limits

## Comparison to Live Trading

Paper Trading Results Are:
- **Conservative on fill rates** (back-of-queue assumption)
- **Optimistic on latency** (app code only, no real network)
- **Reasonable on fees** (2 bps matches Lighter DEX)
- **Simplified on market dynamics** (no order book interaction)

Live Trading Will Have:
- **Higher latency** (add 1-50ms real network + exchange variability)
- **Lower fill rates** (more competition, realistic queue position)
- **Variable slippage** (market impact on large orders)
- **Order rejections** (margin, rate limits, invalid params)
- **Better maker fills** (0.2 bps instead of 2 bps if positioned well)

## Monitoring During 24-Hour Run

1. **Fill Ratio** - Should be 40-80%, average ~60%
2. **Fee Impact** - Should see ~2 bps deducted from volume
3. **Position Accumulation** - Should be slower than instant mode, respect 1 BTC limit
4. **Profitability** - Should be positive if 10 bps spread > 4 bps costs

## Limitations to Be Aware Of

- **No market impact modeling** - Slippage is fixed, doesn't increase with size
- **Simplified queue** - Always assumes back-of-queue, doesn't adapt to actual depth
- **Fixed latency** - No jitter or variance in delays
- **No rejections** - Doesn't test rejection handling
- **Taker-only fees** - Doesn't model maker fills (which would have 0.2 bps fee)
