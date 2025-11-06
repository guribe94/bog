# Phase 4: Realistic Fill Simulation - Design Document

## Current State Analysis

### SimulatedExecutor Limitations

**File**: `bog-core/src/execution/simulated.rs`

**Current Behavior** (Lines 56-87):
```rust
fn simulate_fill(&mut self, order: &mut Order) -> Fill {
    let fill_size = order.remaining_size();  // ❌ Always fills completely
    let fill_price = order.price;             // ❌ No slippage
    // ... instant fill ...
}
```

**Critical Issues:**

1. **Instant Fills** ❌
   - Orders fill immediately on next tick
   - No queue waiting time
   - Unrealistic for backtesting

2. **Complete Fills** ❌
   - Always fills 100% of order size
   - Real markets have partial fills
   - Overstates strategy performance

3. **No Queue Position** ❌
   - Doesn't track position in FIFO queue
   - Can't model priority
   - Can't optimize cancel/replace decisions

4. **No Latency** ❌
   - Order placement → fill in 0ns
   - Real exchanges: 50-200μs round-trip
   - Affects high-frequency strategies

5. **No Adverse Selection** ❌
   - Equal fill probability regardless of market direction
   - Real markets: higher fills when price moves against you
   - Overestimates profitability

---

## Phase 4 Goals

### Primary Objective
Create realistic fill simulation that accurately models Lighter DEX orderbook dynamics for strategy validation and backtesting.

### Success Criteria
- [ ] Queue position tracking (FIFO)
- [ ] Probabilistic partial fills
- [ ] Latency modeling (~100μs exchange round-trip)
- [ ] Adverse selection simulation
- [ ] Configurable realism level (instant/realistic/conservative)

---

## Design: Realistic Fill Model

### 4.1 Queue Position Tracking

**Concept**: Track where our order sits in the FIFO queue at each price level.

```rust
struct QueuePosition {
    price_level: u64,           // Price in fixed-point
    our_size: u64,              // Our order size
    size_ahead: u64,            // Volume ahead of us in queue
    total_size: u64,            // Total size at this level
    timestamp: u64,             // When we joined queue
}
```

**Algorithm**:
1. When we place order at price P:
   - Check if P exists in orderbook
   - If yes: Add our size to back of queue (`size_ahead = current_size_at_P`)
   - If no: We're first in queue (`size_ahead = 0`)

2. Each tick:
   - Check if market traded through our level
   - Calculate volume traded: `traded = max(0, prev_size - current_size)`
   - Update queue: `size_ahead = max(0, size_ahead - traded)`
   - If `size_ahead == 0`, we start getting filled

**Example**:
```text
Market: Bid $50,000 (10 BTC)
We place: Bid $50,000 (0.1 BTC)

Initial state:
  size_ahead = 10.0 BTC
  our_size = 0.1 BTC
  position = 11th in line

After 5 BTC trades:
  size_ahead = 5.0 BTC
  position = 6th in line

After another 5.1 BTC trades:
  size_ahead = 0 BTC
  we start filling (0.1 BTC filled)
```

### 4.2 Partial Fill Logic

**Fill Probability Model**:

```rust
fn calculate_fill_probability(
    queue_pos: &QueuePosition,
    market_volume: u64,  // Volume that traded this tick
    market_direction: i8, // -1 (down), 0 (neutral), 1 (up)
) -> f64 {
    // Base fill rate from queue position
    let queue_utilization = queue_pos.size_ahead as f64 / queue_pos.total_size as f64;
    let base_fill_rate = if queue_utilization < 0.5 {
        0.8  // Front of queue: 80% fill rate
    } else {
        0.4  // Back of queue: 40% fill rate
    };

    // Adverse selection adjustment
    let adverse_selection_multiplier = match (queue_pos.side, market_direction) {
        (Side::Buy, -1) => 1.5,  // Market falling → easier to get filled on bids
        (Side::Sell, 1) => 1.5,  // Market rising → easier to get filled on asks
        _ => 1.0,
    };

    // Volume-based adjustment
    let volume_ratio = market_volume as f64 / queue_pos.total_size as f64;
    let volume_multiplier = volume_ratio.min(1.0);

    (base_fill_rate * adverse_selection_multiplier * volume_multiplier).min(1.0)
}
```

**Fill Size Calculation**:
```rust
let fill_probability = calculate_fill_probability(...);
let potential_fill = (queue_pos.our_size as f64 * fill_probability) as u64;
let actual_fill = potential_fill.min(market_volume);  // Can't fill more than traded
```

### 4.3 Latency Simulation

**Latency Model**:

```rust
struct LatencySimulator {
    placement_latency_ns: u64,  // Time to place order
    fill_latency_ns: u64,       // Time to receive fill notification
    cancel_latency_ns: u64,     // Time to cancel order
}

impl Default for LatencySimulator {
    fn default() -> Self {
        // Lighter DEX typical latencies
        Self {
            placement_latency_ns: 50_000,   // 50μs
            fill_latency_ns: 100_000,        // 100μs
            cancel_latency_ns: 30_000,       // 30μs
        }
    }
}
```

**Order State Machine**:
```text
New Order
   │
   ├─(placement_latency)─→ Pending on Exchange
   │                            │
   │                            ├─(matched)─→ Filled (not yet known)
   │                            │                 │
   │                            │                 └─(fill_latency)─→ Fill Received
   │                            │
   │                            └─(cancel)─→ Cancel Requested
   │                                             │
   │                                             └─(cancel_latency)─→ Cancelled
```

**Implementation**:
```rust
struct PendingOrder {
    order: Order,
    state: OrderState,
    placed_at: u64,      // Nanosecond timestamp
    filled_at: Option<u64>,
    fill_notified_at: Option<u64>,
}

enum OrderState {
    Placing,           // Sending to exchange
    Active,            // On book
    Filling,           // Matched but not yet notified
    Filled,            // Fill received
    Cancelling,        // Cancel requested
    Cancelled,         // Cancel confirmed
}
```

### 4.4 Adverse Selection Model

**Concept**: You're more likely to get filled when the market moves against you.

**Model**:
```rust
fn calculate_adverse_selection_boost(
    order_side: Side,
    price_movement_bps: i32,  // Positive = price rising
) -> f64 {
    match order_side {
        Side::Buy => {
            // If price is falling, easier to get filled on buy orders
            if price_movement_bps < 0 {
                1.0 + ((-price_movement_bps as f64) / 100.0).min(0.5)
            } else {
                // Price rising, harder to get filled
                1.0 / (1.0 + (price_movement_bps as f64 / 100.0).min(0.3))
            }
        },
        Side::Sell => {
            // If price is rising, easier to get filled on sell orders
            if price_movement_bps > 0 {
                1.0 + ((price_movement_bps as f64) / 100.0).min(0.5)
            } else {
                1.0 / (1.0 + ((-price_movement_bps as f64) / 100.0).min(0.3))
            }
        },
    }
}
```

**Example**:
- Bid at $50,000 with base 50% fill rate
- Price drops 10 bps → adverse selection boost: 1.1x
- **Effective fill rate: 55%**

This models reality: Market makers get "adversely selected" - picked off when they're wrong.

---

## Implementation Plan

### Phase 4.1: Queue Position Tracking ⏱️ 4 hours

**Files to modify**:
- `bog-core/src/execution/simulated.rs`

**New structures**:
```rust
struct QueueTracker {
    positions: HashMap<OrderId, QueuePosition>,
    market_state: MarketSnapshot,
}

impl QueueTracker {
    fn add_order(&mut self, order: &Order, snapshot: &MarketSnapshot);
    fn update_from_market(&mut self, snapshot: &MarketSnapshot);
    fn get_fillable_size(&self, order_id: &OrderId) -> u64;
}
```

**Testing**:
- Unit test: Order joins queue at back
- Unit test: Volume trades through level
- Unit test: Multiple orders at same level

### Phase 4.2: Partial Fill Logic ⏱️ 6 hours

**Add to `SimulatedExecutor`**:
```rust
struct RealisticFillConfig {
    enable_queue_modeling: bool,
    enable_partial_fills: bool,
    enable_adverse_selection: bool,
    front_of_queue_fill_rate: f64,  // Default: 0.8
    back_of_queue_fill_rate: f64,   // Default: 0.4
}

impl SimulatedExecutor {
    fn new_realistic(config: RealisticFillConfig) -> Self;

    fn calculate_realistic_fill(
        &self,
        order: &Order,
        queue_pos: &QueuePosition,
        market_snapshot: &MarketSnapshot,
    ) -> Option<Fill>;
}
```

**Testing**:
- Unit test: Front of queue fills faster
- Unit test: Back of queue fills slower
- Integration test: Compare instant vs realistic over 1000 ticks

### Phase 4.3: Latency Modeling ⏱️ 4 hours

**Add `LatencySimulator`**:
```rust
struct LatencySimulator {
    pending_orders: Vec<PendingOrder>,
    pending_fills: Vec<(Fill, u64)>,  // (fill, notify_at_ns)
    current_time_ns: u64,
}

impl LatencySimulator {
    fn place_order(&mut self, order: Order) -> OrderId;
    fn tick(&mut self, current_time_ns: u64) -> Vec<Event>;
    fn get_visible_state(&self) -> Vec<Order>;
}

enum Event {
    OrderActive(OrderId),
    OrderFilled(Fill),
    OrderCancelled(OrderId),
}
```

**Testing**:
- Unit test: Order placement delayed by 50μs
- Unit test: Fill notification delayed by 100μs
- Integration test: Latency affects cancel/replace strategies

### Phase 4.4: Integration & Testing ⏱️ 6 hours

**Backtesting Comparison**:
```rust
#[test]
fn test_instant_vs_realistic_fills() {
    let market_data = load_historical_data("BTC-USD-2024-01-01.csv");

    // Run with instant fills
    let instant_results = run_backtest(
        SimpleSpread,
        SimulatedExecutor::new(),
        market_data.clone()
    );

    // Run with realistic fills
    let realistic_results = run_backtest(
        SimpleSpread,
        SimulatedExecutor::new_realistic(RealisticFillConfig::default()),
        market_data
    );

    // Realistic should show:
    // - Lower fill rate
    // - More adverse selection
    // - More realistic PnL
    assert!(realistic_results.fill_rate < instant_results.fill_rate);
    assert!(realistic_results.pnl < instant_results.pnl);
}
```

**Metrics to Track**:
- Fill rate: Instant vs Realistic
- Average fill latency
- Queue position statistics
- Adverse selection cost
- PnL comparison

---

## Configuration Levels

### Level 1: Instant (Current) - Development/Debug
```rust
SimulatedExecutor::new()  // Instant fills, perfect execution
```

### Level 2: Realistic - Backtesting
```rust
SimulatedExecutor::new_realistic(RealisticFillConfig {
    enable_queue_modeling: true,
    enable_partial_fills: true,
    enable_adverse_selection: true,
    front_of_queue_fill_rate: 0.8,
    back_of_queue_fill_rate: 0.4,
})
```

### Level 3: Conservative - Stress Testing
```rust
SimulatedExecutor::new_realistic(RealisticFillConfig {
    enable_queue_modeling: true,
    enable_partial_fills: true,
    enable_adverse_selection: true,
    front_of_queue_fill_rate: 0.6,  // Harder to fill
    back_of_queue_fill_rate: 0.2,
})
```

---

## Expected Impact

### Before (Instant Fills):
- Fill rate: ~90-95%
- Latency: 0ns
- PnL: Optimistic
- **Risk**: Strategy fails in production

### After (Realistic Fills):
- Fill rate: ~40-60% (realistic for market making)
- Latency: 50-200μs
- PnL: Conservative but accurate
- **Benefit**: Confident production deployment

---

## Performance Considerations

**Computational Cost**:
- Queue tracking: +10-20ns per order
- Fill probability: +5-10ns per calculation
- Latency simulation: +5ns per tick
- **Total overhead**: ~30-40ns per order

**Acceptability**:
- Current tick-to-trade: 59.5ns
- With realistic fills: ~90-100ns
- **Still under 100ns target** ✅

---

## References

- **Papers**:
  - Avellaneda & Stoikov (2008) - High-frequency trading models
  - Huang & Rosenbaum (2017) - Queue position in LOB

- **Industry Practice**:
  - Market makers assume 40-60% fill rate
  - Front-of-queue premium: 1.5-2x fill probability
  - Adverse selection cost: 2-5 bps per trade

---

## Implementation Priority

1. **Phase 4.1** (MUST HAVE): Queue position tracking
2. **Phase 4.2** (MUST HAVE): Partial fill logic
3. **Phase 4.3** (NICE TO HAVE): Latency modeling
4. **Phase 4.4** (MUST HAVE): Testing & validation

**Total Estimated Time**: 20 hours (2.5 days)

---

## Success Metrics

- [ ] Backtest shows 40-60% fill rate (realistic)
- [ ] Adverse selection measurable in PnL
- [ ] Strategy still profitable after realistic fills
- [ ] Performance overhead <40ns
- [ ] Comprehensive test coverage (>80%)
