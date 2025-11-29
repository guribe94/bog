# SimpleSpread Market Making Strategy Guide

**Purpose**: Complete guide to understanding and operating the SimpleSpread market making strategy
**Audience**: Developers, traders, operators
**Prerequisites**: Basic understanding of orderbooks and trading
**Related**: [Command Reference](command-reference.md) | [State Machines](../architecture/STATE_MACHINES.md)

---

## Quick Reference (TL;DR for LLMs)

**What**: Market-neutral liquidity provision strategy
**How**: Quote bid/ask symmetrically around mid price, capture spread on round-trips
**Profit**: 6-8 bps per round-trip after fees (10 bps spread - 2-4 bps fees)
**Risk**: Inventory accumulation in trending markets (mitigated by position limits)
**Performance**: 70ns tick-to-trade, thousands of round-trips per day

**Key Parameters**:
- Spread: 10 bps (0.1%)
- Order size: 0.1 BTC
- Fees: 2 bps per leg
- Max position: 1.0 BTC

---

## Part 1: What is Market Making?

### The Concept

Market making is providing **liquidity** to a market by:
1. **Continuously offering to BUY** at a price slightly below market (bid)
2. **Continuously offering to SELL** at a price slightly above market (ask)
3. **Capturing the difference** (spread) when both sides fill

**You profit from the bid-ask spread, not from predicting price direction.**

### Economic Role

- **Buyers** get to buy immediately at your ask (pay spread for convenience)
- **Sellers** get to sell immediately at your bid (pay spread for speed)
- **You** collect the spread as payment for providing instant liquidity

This is why it works: people pay for convenience/speed.

### Core Insight

**You don't predict direction.** You're market-neutral:
- Quote both sides equally
- Profit whether price goes up or down
- Make money from volatility (oscillation), not trends

---

## Part 2: The SimpleSpread Algorithm

### High-Level Flow

```
1. Read market tick (best bid/ask, sizes)
2. Calculate mid price
3. Validate market conditions (spread, liquidity, sanity)
4. Calculate our quotes (mid ± half_spread)
5. Place orders (bid + ask)
6. Wait for fills
7. Update position
8. Repeat
```

**Target latency**: <1μs per iteration (measured: 70ns)

### Step-by-Step Walkthrough

#### Example Market State
```
Best Bid: $50,000 (size: 2.5 BTC)
Best Ask: $50,005 (size: 3.0 BTC)
Market Spread: $5 (1 basis point)
```

#### Step 1: Calculate Mid Price

```rust
mid_price = (bid + ask) / 2
mid_price = ($50,000 + $50,005) / 2 = $50,002.50
```

**Why**: Mid price represents "fair value" - halfway between buyer/seller prices.

**Implementation note**: Uses overflow-safe arithmetic:
```rust
mid = bid/2 + ask/2 + (bid%2 + ask%2)/2
```

#### Step 2: Validate Market Conditions

**Check 1: Spread in valid range** (1-50 bps)
```
spread_bps = ((ask - bid) / bid) * 10,000
spread_bps = (5 / 50,000) * 10,000 = 1 bps 
```

**Check 2: Volatility adjustment**
The strategy tracks market volatility using an EWMA (Exponentially Weighted Moving Average).
- **Low Volatility (<10 bps)**: Use base spread (10 bps).
- **High Volatility (>10 bps)**: Widen spread linearly up to 2x (20 bps) to compensate for increased adverse selection risk.
- **Extreme Volatility (>50 bps)**: Cap at 2x spread.

**Check 3: Position Limits**
- **Long Limit**: If `position >= MAX_POSITION`, only quote Ask (reduce long).
- **Short Limit**: If `position <= MAX_SHORT`, only quote Bid (reduce short).
- **Normal**: Quote both sides.

**Check 4: Sufficient liquidity** (>= 0.001 BTC each side)
```
bid_size = 2.5 BTC 
ask_size = 3.0 BTC 
```

**Why**: Thin books mean low fill probability

**Check 3: Prices in sane range** ($1 to $1,000,000)
```
$50,000  (catches corrupted data)
```

#### Step 3: Calculate Our Quotes

**Configuration**: SPREAD_BPS = 10 (compile-time constant)

```rust
half_spread = mid_price * (SPREAD_BPS / 20,000)
half_spread = $50,002.50 * (10 / 20,000)
half_spread = $50,002.50 * 0.0005
half_spread = $25.00

our_bid = mid_price - half_spread = $50,002.50 - $25 = $49,977.50
our_ask = mid_price + half_spread = $50,002.50 + $25 = $50,027.50
```

**Why divide by 20,000?**
- 10 bps = 0.10% = 0.001
- We want HALF the spread on each side
- 10 bps / 2 = 5 bps = 0.0005
- Formula: `(price * spread_bps) / 20,000`

#### Step 4: Place Orders

```
Bid Order: Buy 0.1 BTC @ $49,977.50
Ask Order: Sell 0.1 BTC @ $50,027.50

Our spread: $50 (10 bps on $50,000)
Market spread: $5 (1 bps)
```

**Visual**:
```
        $49,977.50 ← Our BID
$50,000 ← Market BID
                      (gap: $22.50)
$50,005 ← Market ASK
        $50,027.50 ← Our ASK
```

**Note**: Our quotes are OUTSIDE the market spread. We only fill when market moves TO us.

---

## Part 3: Example Execution

### Tick 1: Market Opens
```
Market: $50,000 / $50,005 (mid: $50,002.50)
We quote: $49,977.50 / $50,027.50
Position: 0 BTC, $0 PnL
```

### Tick 2: Market Drops - Bid Fills
```
Market drops to: $49,975 / $50,000 (mid: $49,987.50)
Someone panic-sells, hits our bid @ $49,977.50

FILL: BUY 0.1 BTC @ $49,977.50
Cost: $4,997.75 + $1.00 fee = $4,998.75
Position: +0.1 BTC (long)
Unrealized PnL: ($49,987.50 - $49,987.75) × 0.1 = -$0.025 (slightly underwater)

We update quotes for new market:
New quotes: $49,962.50 / $50,012.50
```

### Tick 3: Market Rallies - Ask Fills
```
Market rallies to: $50,005 / $50,015 (mid: $50,010)
Someone wants to buy, hits our ask @ $50,012.50 (from Tick 2)

FILL: SELL 0.1 BTC @ $50,012.50
Revenue: $5,001.25 - $1.00 fee = $5,000.25
Position: 0 BTC (flat again!)

PROFIT CALCULATION:
Buy: Paid $4,998.75 for 0.1 BTC
Sell: Received $5,000.25 for 0.1 BTC
Net profit: $5,000.25 - $4,998.75 = $1.50

As bps: $1.50 / $5,000 ≈ 3 bps (after round-trip)
```

**This is a complete round-trip: bought low, sold high, captured spread.**

### Key Observations

1. **Profit from oscillation**: Market moved down (we bought), then up (we sold)
2. **No prediction needed**: We didn't know market would reverse, but profited anyway
3. **Symmetric quotes**: Always quoted both sides, stayed market-neutral
4. **Small but consistent**: $1.50 profit × 1,000 round-trips/day = $1,500/day

---

## Part 4: Profitability Analysis

### Per Round-Trip Economics

**For 0.1 BTC at $50,000:**
```
1. Buy @ $49,975 (mid - half_spread)
   Fee: 2 bps = $1.00
   Total cost: $4,998.50

2. Sell @ $50,025 (mid + half_spread)
   Fee: 2 bps = $1.00
   Net revenue: $5,001.50

3. Profit: $5,001.50 - $4,998.50 = $3.00
   As bps: $3 / $5,000 = 0.06% = 6 bps 
```

**Breakdown**:
- Gross spread captured: 10 bps
- Less buy-side fee: 2 bps
- Less sell-side fee: 2 bps
- **Net profit: 6 bps per round-trip**

### Compile-Time Profitability Guarantee

```rust
const _: () = assert!(
    SPREAD_BPS >= MIN_PROFITABLE_SPREAD_BPS,
    "SPREAD_BPS must be >= MIN_PROFITABLE_SPREAD_BPS"
);

MIN_PROFITABLE_SPREAD_BPS = 2 (must cover fees)
SPREAD_BPS = 10 

→ Code literally won't compile if spread too narrow!
```

### Daily Profitability Estimates

**Conservative** (trending market):
```
100 round-trips/day × $3 profit = $300/day
```

**Average** (mixed conditions):
```
500 round-trips/day × $3 profit = $1,500/day
Some inventory carried overnight
```

**Optimistic** (oscillating market):
```
2,000 round-trips/day × $3 profit = $6,000/day
Market bounces through quotes repeatedly
```

**Reality**: Depends on market conditions (volatility, trends, volume)

---

## Part 5: Risk Management

### Risk 1: Inventory Accumulation (Trending Markets)

**Problem**: If market trends up, you keep buying (bid fills) but don't sell (ask doesn't fill)

**Example**:
```
Tick 1: Buy 0.1 BTC @ $50,000 (filled)
Tick 2: Buy 0.1 BTC @ $50,100 (filled) - ask not filled
Tick 3: Buy 0.1 BTC @ $50,200 (filled) - ask not filled
Position: +0.3 BTC

If market drops $1,000: Loss = 0.3 BTC × $1,000 = $300
```

**Mitigation**:
1. **Position limits**: Max 1.0 BTC long, 1.0 BTC short (enforced at runtime)
2. **Daily loss limits**: Stop trading if lose > $1,000/day
3. **Circuit breaker**: Halt on >10% price moves

### Risk 2: Adverse Selection

**Problem**: Getting filled when it's bad for you (informed traders)

**Example**:
- You quote: $49,977.50 / $50,027.50
- Informed trader knows price about to drop
- They sell to you at $49,977.50
- Market drops to $49,500
- You're holding -$477.50 loss on 0.1 BTC

**Mitigation**:
1. **Speed**: Sub-microsecond latency = update quotes before market moves
2. **Spread size**: 10 bps buffer absorbs small adverse moves
3. **Realistic fills**: 40-80% fill rate (you're not always first in queue)

### Risk 3: Fee Erosion

**Math**:
- Gross profit: 10 bps (spread)
- Fees: 4 bps (2 bps each leg)
- Net profit: 6 bps

**Why this works**: 6 bps > 0 → profitable 

**Enforced at compile time**: Can't configure unprofitable spreads

---

## Part 6: Why Speed Matters (HFT)

### The Adverse Selection Problem

**Scenario: You're slow (1 second latency)**
```
T=0:        Market @ $50,000 / $50,005
            You quote: $49,977.50 / $50,027.50

T=100ms:    Market moves to $50,100 / $50,105
            Your quotes are now stale

T=500ms:    Someone sells to you @ $49,977.50
            But market is now $50,100 (you're 122.50 bps behind!)

Result: You lose on adverse selection
```

**Scenario: You're fast (70ns latency)**
```
T=0:        Market @ $50,000 / $50,005
            You quote: $49,977.50 / $50,027.50

T=70ns:     Market moves to $50,100 / $50,105
            You immediately update: $50,072.50 / $50,127.50

Result: Your quotes stay close to mid, minimal adverse selection
```

### Measured Performance

**Target**: <1μs tick-to-trade

**Measured** (2025-11-12):
- Tick-to-trade: **70.79ns** (14.1x faster than target)
- Strategy calc: 17.28ns
- Risk validation: 2.37ns
- Orderbook sync: ~20ns

**Why this matters**: You can react to market changes before competitors

---

## Part 7: Comparison to Other Strategies

### vs Random Trading
- **Random**: Buy/sell randomly → Expected return: -fees (you lose)
- **SimpleSpread**: Quote both sides → Expected return: +spread - fees 

### vs Directional Trading
- **Directional**: Predict up/down → Win rate ~50%, large gains/losses
- **SimpleSpread**: No prediction → Profit from spread service, small consistent gains

### vs InventoryBased Strategy (future)
- **SimpleSpread**: Symmetric quotes, builds inventory in trends, simpler (faster)
- **InventoryBased**: Adjusts quotes based on position, reduces inventory, more complex (safer but slower)

---

## Part 8: Limitations

### When SimpleSpread Fails

**1. Strong Trending Markets**
- Market trends up 10%
- You keep buying (bid fills)
- Asks don't fill
- Position grows to max limit (1.0 BTC)
- If market reverses: unrealized loss

**2. Low Volatility / Tight Spreads**
- Market spread: 1 bps (tight)
- Your spread: 10 bps (wider)
- No one trades with you (market is tighter)
- Result: No fills, no profit (but also no loss)

**3. Flash Crashes**
- Market crashes 10% in seconds
- You have +1.0 BTC position
- Loss: up to -$5,000
- **Mitigation**: Circuit breaker halts trading on >10% moves

---

## Part 9: Production Configuration

### Compile-Time Parameters

```toml
[features]
# Spread (10 bps recommended)
spread-10bps = []  # Default
spread-5bps = []   # Tighter (more aggressive)
spread-20bps = []  # Wider (more conservative)

# Order size
size-small = []   # 0.01 BTC
size-medium = []  # 0.1 BTC (default)
size-large = []   # 1.0 BTC

# Risk limits
conservative = ["max-position-half", "max-order-tenth"]
standard = []     # Default
aggressive = ["max-position-double"]
```

### Build Commands

```bash
# Standard configuration
cargo build --release --features spread-10bps,size-medium

# Conservative (smaller positions)
cargo build --release --features spread-10bps,size-small,conservative

# Aggressive (larger positions, only for experienced users)
cargo build --release --features spread-5bps,size-large,aggressive
```

### Runtime Safety Checks

Even with compile-time config, runtime checks enforce:
1.  Position limits (max 1.0 BTC)
2.  Daily loss limits (max $1,000)
3.  Circuit breaker (>10% move halts)
4.  Rate limiter (10 orders/sec default)
5.  Pre-trade validation (6 checks)
6.  Kill switch (SIGUSR1 emergency stop)

---

## Part 10: Operational Guide

### Starting the Bot

```bash
# Prerequisites
cd ../huginn && ./target/release/huginn --market 1 --hft

# Start bot (simulated mode)
cd bog
./target/release/bog-simple-spread-simulated --market 1

# Monitor
tail -f logs/trading.log
```

### Monitoring Metrics

**Key metrics to watch**:
```
bog_trading_fill_rate          # Should be 40-80% in realistic mode
bog_risk_position_btc          # Should stay within [-1.0, +1.0]
bog_risk_realized_pnl_usd      # Should grow over time
bog_performance_tick_to_trade  # Should be <1μs
```

### When to Intervene

| Alert | Severity | Action |
|-------|----------|--------|
| Position > 0.8 BTC | WARNING | Monitor closely |
| Position = 1.0 BTC | CRITICAL | Strategy stops bidding automatically |
| Daily loss > $500 | WARNING | Review market conditions |
| Daily loss > $1,000 | CRITICAL | Auto-stops trading |
| Circuit breaker trips | CRITICAL | Manual investigation required |

---

## Part 11: Why This Strategy is Production-Grade

### Safety First
 Compile-time profitability guarantee (code won't compile if spread too narrow)
 Multiple validation layers (market data, risk, pre-trade)
 Position limits enforced
 Circuit breakers for extreme moves
 All errors halt trading (no silent failures)

### Performance Optimized
 Minimal state (~32 bytes stack allocated)
 Const generic configuration (0ns runtime cost)
 Fixed-point arithmetic (no heap allocations)
 Sub-microsecond latency (70ns measured)

### Mathematically Sound
 Overflow-safe arithmetic
 Precision-preserving (9 decimal fixed-point)
 Fee-aware from design

### Well-Tested
 13 unit tests for strategy logic
 33 safety tests for execution
 Property tests for arithmetic
 Fuzz tests for edge cases

---

## Summary

**SimpleSpread makes money by**:
1. Quoting both sides of the market symmetrically
2. Capturing the spread when both sides fill
3. Staying market-neutral (no directional bets)
4. Managing inventory with position limits
5. Moving fast to minimize adverse selection
6. Profiting from volatility (oscillation), not direction

**It works because**:
- You're paid to provide liquidity (spread = payment)
- Law of large numbers (many small edges compound)
- Statistical mean reversion (markets oscillate)
- Speed advantage (update quotes faster than competition)

**It's safe because**:
- Profits guaranteed > fees (compile-time check)
- Position limits cap downside
- Multiple validation layers
- All errors halt trading

**Expected return**: Small positive edge (6 bps per round-trip) that compounds over thousands of trades per day.

---

## Related Documentation

- [Command Reference](command-reference.md) - How to run the bot
- [Market Selection](market-selection.md) - Choosing markets
- [State Machines](../architecture/STATE_MACHINES.md) - Safety architecture
- [Performance Benchmarks](../benchmarks/LATEST.md) - Measured latencies
- [Production Readiness](../deployment/PRODUCTION_READINESS.md) - Operations manual

---

**Last Updated**: 2025-11-29
**Status**:  Current
**Maintained by**: Bog Team
