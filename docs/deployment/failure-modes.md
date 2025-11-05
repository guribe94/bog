# Failure Modes and Handling

## Overview

This document catalogs known failure modes in bog's HFT trading system, their impacts, detection methods, and mitigation strategies. The philosophy is **defense in depth**: prevent when possible, detect early, degrade gracefully, and fail loudly when necessary.

## Severity Levels

| Level | Impact | Action | Example |
|-------|--------|--------|---------|
| **Critical** | Trading halted | Immediate shutdown | Position overflow |
| **High** | Degraded accuracy | Continue with warnings | Queue near capacity |
| **Medium** | Performance impact | Log and monitor | Increased latency |
| **Low** | Informational | Log only | Occasional L3 cache miss |

---

## 1. Position Overflow

### Description

Arithmetic overflow when updating position quantity or PnL, causing wraparound (e.g., i64::MAX + 1 = i64::MIN).

### Impact

**Severity**: Critical
- Position becomes negative when it should be positive (or vice versa)
- Risk calculations completely wrong → overleveraging
- PnL reporting incorrect → wrong trading decisions
- Potential regulatory violations (position limits)

### Scenario

```rust
// Before overflow protection
let position = i64::MAX - 100;  // Near limit (9.2 billion BTC)
let delta = 200;                // Buy 200 BTC

// Wrapping add (BAD)
let new_position = position.wrapping_add(delta);
// new_position = i64::MIN + 99 (WRONG! Shows massive short instead of long)
```

### Detection

**Method 1**: Checked arithmetic (primary)
```rust
match position.update_quantity_checked(delta) {
    Ok(new_qty) => { /* continue */ },
    Err(OverflowError::QuantityOverflow { old, delta }) => {
        alert!("CRITICAL: Position overflow prevented: {} + {}", old, delta);
        return Err(RejectionReason::PositionOverflow);
    }
}
```

**Method 2**: Monitoring (secondary)
```yaml
alert: PositionOverflow
expr: rate(bog_overflow_errors_total{type="quantity"}[5m]) > 0
severity: critical
```

### Mitigation

**Prevention**:
- Use `update_quantity_checked()` in all trading code
- Reject orders that would cause overflow
- Position limits far below i64::MAX (e.g., max 1000 BTC)

**Recovery**:
- Halt trading immediately on overflow detection
- Manual intervention required to reconcile position
- Log full state for forensic analysis

**Code location**: `bog-core/src/core/types.rs:191-215`, `bog-core/src/core/errors.rs:10-22`

### Probability

**Realistic**: Near zero
- Would require position of >9 billion BTC (21M total supply)
- Or >$9 billion PnL (unrealistic for single bot)

**Theoretical**: Possible
- Prolonged bug causing position accumulation
- Precision loss in conversion (if using wrong scale)
- External position reconciliation error

---

## 2. Fixed-Point Conversion Errors

### Description

Invalid f64 values (NaN, infinity) or out-of-range prices causing conversion failures or silent precision loss.

### Impact

**Severity**: High
- NaN propagates through calculations → entire signal invalid
- Infinity causes overflow in fixed-point → panic or wrong prices
- Precision loss → orders at wrong prices → execution failures

### Scenario

```rust
// From market data feed (corrupted or flash crash)
let price: f64 = f64::NAN;  // or f64::INFINITY

// Without checking (BAD)
let fixed = fixed_point::from_f64(price);
// fixed = arbitrary garbage value

// Used in signal
let signal = Signal {
    bid_price: fixed,  // Garbage price sent to exchange!
    // ...
};
```

### Detection

**Method 1**: Checked conversion (primary)
```rust
match fixed_point::from_f64_checked(price) {
    Ok(fixed) => { /* use fixed */ },
    Err(ConversionError::NotANumber) => {
        error!("Received NaN price from market data");
        metrics.invalid_prices.inc();
        return Err(InvalidPrice);
    }
    Err(ConversionError::Infinite { positive }) => {
        error!("Received infinite price: {}", if positive { "+inf" } else { "-inf" });
        return Err(InvalidPrice);
    }
    Err(ConversionError::OutOfRange { value }) => {
        warn!("Price {} exceeds safe range (max: {})", value, fixed_point::MAX_SAFE_F64);
        return Err(InvalidPrice);
    }
}
```

**Method 2**: Market data validation (secondary)
```rust
// In Huginn SHM reader
if market.bid <= 0 || market.ask <= 0 || market.ask <= market.bid {
    warn!("Invalid market state: bid={}, ask={}", market.bid, market.ask);
    return None;  // Skip this tick
}
```

### Mitigation

**Prevention**:
- Always use `from_f64_checked()` when converting external data
- Validate market data at ingress (Huginn)
- Sanity checks: bid > 0, ask > bid, spread < max_spread

**Recovery**:
- Skip invalid tick (don't generate signal)
- Continue processing next tick (transient error)
- Alert if sustained (>10 invalid ticks in 1 minute)

**Code location**: `bog-core/src/core/types.rs:263-312`, `bog-core/src/core/errors.rs:24-46`

### Probability

**Realistic**: Low
- Huginn validates market data before publishing
- Exchanges rarely send NaN/infinity in normal operation
- More likely during flash crashes or data feed issues

**Observed**: Occasional
- CEX API bugs during high volatility
- Network corruption (rare with TCP checksums)
- Stale data after reconnection

---

## 3. Fill Queue Overflow (Backpressure)

### Description

Executor generates fills faster than engine can process them, causing queue to fill and drop fills.

### Impact

**Severity**: High
- Dropped fills → position tracking incorrect
- Risk: Engine thinks position is X, reality is Y
- PnL calculations wrong
- Potential over-trading (sending orders while already at limit)

### Scenario

```rust
// During high-frequency trading burst
for tick in 0..10000 {
    // Generate signal
    let signal = strategy.generate_signal(&market, &position);

    // Submit order (immediately fills in simulation)
    let order_id = executor.submit_order(&signal);

    // Executor queues fill
    executor.pending_fills.push(fill);  // Queue: [fill1, fill2, ..., fill1024]

    // Engine polls fills once per tick
    let fills = executor.poll_fills();  // Drains queue

    // If tick rate > processing rate, queue fills up
}

// When queue full (1024 fills):
executor.pending_fills.push(new_fill);
// Error: queue full, fill dropped or oldest evicted
```

### Detection

**Method 1**: Queue depth monitoring (primary)
```rust
const QUEUE_DEPTH_WARNING_THRESHOLD: usize = 100;

let depth = executor.pending_fills.len();
if depth > QUEUE_DEPTH_WARNING_THRESHOLD {
    warn!("Fill queue depth high: {} (max: {})", depth, MAX_PENDING_FILLS);
    self.queue_warnings.inc();
}
```

**Method 2**: Dropped fill counter (secondary)
```rust
pub struct SimulatedExecutor {
    dropped_fills: u64,  // Incremented on overflow
}

// Alert if any fills dropped
if executor.dropped_fills > 0 {
    alert!("Dropped {} fills", executor.dropped_fills);
}
```

**Method 3**: Monitoring
```yaml
alert: HighQueueDepth
expr: bog_queue_depth > 100
severity: warning

alert: DroppedFills
expr: rate(bog_dropped_fills_total[5m]) > 0
severity: critical
```

### Mitigation

**Prevention**:
- Bounded queue (1024 fills max) prevents OOM
- Poll fills every tick (drain queue constantly)
- Backpressure: Slow down signal generation if queue full

**Recovery**:
- Drop oldest fill when full (FIFO eviction)
- Log dropped fills for manual reconciliation
- Halt trading if sustained queue pressure (>10 drops/minute)

**Graceful degradation**:
```rust
// Strategy: Skip signal generation if queue pressure high
if executor.pending_fills.len() > 900 {
    // Queue 90% full, stop generating new signals
    return None;
}
```

**Code location**: `bog-core/src/execution/simulated.rs:109-134`, `bog-core/src/engine/generic.rs:128-149`

### Probability

**Realistic**: Medium (in simulation)
- Backtesting with unrealistic fill assumptions (instant fills)
- Processing replay at higher speed than live rate
- Multiple markets processed sequentially

**Production**: Low
- Real exchanges have natural rate limiting (~100ms fill latency)
- Queue depth typically <10 fills

---

## 4. Flash Crash (Market Data Spike)

### Description

Extreme price movement in milliseconds (e.g., BTC drops from $50k to $0.01), causing strategy to generate invalid signals or execute at bad prices.

### Impact

**Severity**: High
- Strategy may generate signals at nonsensical prices
- Risk of fat-finger trades (buy at $100k when market is $50k)
- PnL swings wildly, triggering risk limits
- Exchange may reject orders (price >X% from mid)

### Scenario

```
Time   Bid      Ask      Spread
────────────────────────────────
T+0ms  $50,000  $50,010  10 bps  ← Normal
T+1ms  $50,000  $75,000  50%     ← Flash crash begins
T+2ms  $10,000  $75,000  750%    ← Extreme spread
T+3ms  $0.01    $100,000 ∞       ← Market broken
T+5ms  $49,995  $50,005  10 bps  ← Recovery
```

**Strategy behavior without protection**:
```rust
// At T+2ms:
let mid = (10_000 + 75_000) / 2 = $42,500  // "Mid" is nonsense
let our_bid = mid - spread = $42,500 - $100 = $42,400
let our_ask = mid + spread = $42,500 + $100 = $42,600

// Submit orders at terrible prices
// Risk: Buy at $42,600 (85% above true market $23k)
```

### Detection

**Method 1**: Spread filter (primary)
```rust
const MAX_SPREAD_BPS: u64 = 100;  // 1%

let spread_bps = (market.ask - market.bid) * 10000 / mid;
if spread_bps > MAX_SPREAD_BPS {
    warn!("Spread too wide: {} bps (max: {})", spread_bps, MAX_SPREAD_BPS);
    return None;  // Skip this tick
}
```

**Method 2**: Price change limit (secondary)
```rust
let price_change_pct = ((market.mid - self.last_mid).abs() * 100) / self.last_mid;
if price_change_pct > 10 {  // >10% move in one tick
    warn!("Extreme price move: {}% in one tick", price_change_pct);
    return None;  // Wait for market to stabilize
}
```

**Method 3**: Size validation (tertiary)
```rust
if market.bid_size < MIN_SIZE || market.ask_size < MIN_SIZE {
    warn!("Low liquidity: bid_size={}, ask_size={}", market.bid_size, market.ask_size);
    return None;  // Don't trade in thin markets
}
```

### Mitigation

**Prevention**:
- Always check spread before generating signal
- Reject ticks with >1% spread
- Validate price change vs previous tick (<10%)
- Require minimum size on both sides (e.g., >0.1 BTC)

**Recovery**:
- Skip ticks during volatility (wait for spread to normalize)
- Don't generate signals until market stable for N ticks (e.g., 10)
- Cancel all orders during flash crash (reduce exposure)

**Future enhancement** (Phase 6+):
```rust
pub struct VolatilityCircuitBreaker {
    consecutive_wide_spreads: u32,
    halt_threshold: u32,  // e.g., 100 wide spreads = halt
}

if circuit_breaker.consecutive_wide_spreads > circuit_breaker.halt_threshold {
    error!("Circuit breaker triggered: market unstable");
    engine.shutdown();
}
```

**Code location**: `bog-strategies/src/simple_spread.rs:58-65` (min_spread filter)

### Probability

**Realistic**: Medium
- Flash crashes occur 1-2 times per year on major exchanges
- More common on smaller exchanges or low-liquidity pairs
- Typically last <5 seconds

**Observed examples**:
- 2017-06-22: ETH flash crash on GDAX ($319 → $0.10 → $319 in seconds)
- 2021-05-19: BTC drop 30% in 1 hour (liquidation cascade)

---

## 5. Clock Desynchronization

### Description

System clock drifts or jumps, causing timestamp inconsistencies between market data, orders, and fills.

### Impact

**Severity**: Medium
- Stale market data used for decisions (latency appears negative!)
- Order timestamps wrong → exchange rejects orders
- PnL calculations use wrong timestamps → audit failures
- Latency measurements meaningless

### Scenario

```
Engine clock:   T=100ns (drifted ahead by 50ns)
Market data:    T=50ns  (Huginn uses NTP-synced clock)

// Engine sees "future" data
let latency = engine_time - market_time = 100 - 50 = 50ns
// Impossible! Data arrived before it was sent?

// Or worse: Clock jumps backward (NTP correction)
Engine clock:   T=1000ns
[NTP sync]
Engine clock:   T=500ns (jumped backward 500ns)

// Order timestamp now in future relative to market data
```

### Detection

**Method 1**: Monotonic clock (primary)
```rust
use std::time::Instant;  // Monotonic clock (never goes backward)

let start = Instant::now();
// ... process tick ...
let elapsed = start.elapsed().as_nanos();
```

**Method 2**: Timestamp validation (secondary)
```rust
let now_ns = get_system_time_ns();
if market.last_update_ns > now_ns + CLOCK_SKEW_THRESHOLD_NS {
    warn!("Market data timestamp in future: market={}, now={}",
          market.last_update_ns, now_ns);
    // Use current time instead
}
```

**Method 3**: Drift monitoring (tertiary)
```rust
// Periodically check system clock vs NTP
let system_time = SystemTime::now();
let ntp_time = query_ntp_server("pool.ntp.org");
let drift = (system_time - ntp_time).abs();

if drift > Duration::from_millis(100) {
    error!("Clock drift: {}ms (max: 100ms)", drift.as_millis());
    // Alert operations to sync clock
}
```

### Mitigation

**Prevention**:
- Use `Instant` (monotonic) for latency measurements
- Use `SystemTime` only for absolute timestamps (logging, audit)
- Run NTP daemon (chrony/ntpd) with frequent sync (every 60s)
- Set max clock adjustment rate (slew, not step)

**Recovery**:
- Reject market data with timestamps >1s in future or past
- Use engine time as fallback if market timestamp invalid
- Log warning but continue trading (clock skew rarely affects correctness)

**Production setup**:
```bash
# Install chrony (better than ntpd for HFT)
apt-get install chrony

# /etc/chrony/chrony.conf
server time.cloudflare.com iburst
server time.google.com iburst
makestep 0.1 3    # Allow step if offset >0.1s (max 3 times)
driftfile /var/lib/chrony/drift
rtcsync           # Sync RTC to system clock
```

**Code location**: `bog-core/src/engine/generic.rs:92-101` (uses `Instant` for latency)

### Probability

**Realistic**: Low
- NTP keeps clocks synced to <10ms typically
- Monotonic clocks (Instant) immune to backward jumps
- Timestamp validation catches most issues

**Observed**:
- Virtualized environments (VM clock drift)
- Leap second handling (rare, well-known)
- Manual clock changes (operator error)

---

## 6. Memory Exhaustion (OOM)

### Description

Unbounded memory growth causing out-of-memory (OOM) kill by OS.

### Impact

**Severity**: Critical
- Process killed by OOM killer → trading halted
- Lost state (positions, pending orders) unless persisted
- Downtime until restart and manual reconciliation

### Scenario

```rust
// Without bounded queue (BAD)
pub struct SimulatedExecutor {
    pending_fills: Vec<Fill>,  // Unbounded!
}

// During long backtest or sustained high volume
for tick in 0..1_000_000_000 {
    let fill = Fill { /* ... */ };
    executor.pending_fills.push(fill);  // Grows indefinitely
    // Memory usage: 1B fills × 64 bytes = 64 GB
}

// OOM killer: "bog-simple-spread-simulated was killed (out of memory)"
```

### Detection

**Method 1**: Bounded collections (primary - prevention)
```rust
use crossbeam::queue::ArrayQueue;

pub struct SimulatedExecutor {
    pending_fills: Arc<ArrayQueue<Fill>>,  // Bounded to 1024
}

// Memory usage: Fixed at 1024 × 64 bytes = 64 KB
```

**Method 2**: Memory monitoring (secondary)
```rust
use tikv_jemallocator::Jemalloc;

#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

// Periodically check heap usage
let heap_bytes = jemalloc_ctl::stats::allocated::read().unwrap();
if heap_bytes > MAX_HEAP_SIZE {
    error!("Heap usage high: {} MB", heap_bytes / 1_048_576);
}
```

**Method 3**: OS limits (tertiary)
```bash
# Set memory limit for process (systemd)
[Service]
MemoryMax=1G
MemoryHigh=800M  # Warn at 800MB
```

### Mitigation

**Prevention**:
- Use bounded collections everywhere (ArrayQueue, not Vec)
- Pre-allocate fixed-size buffers at startup
- Stack allocation preferred over heap (Position, Signal, Fill)
- Profile memory usage during long backtests

**Recovery**:
- OOM kill is unrecoverable (process terminated)
- Rely on process supervisor (systemd) to restart
- Persist positions to disk periodically (checkpoint)

**Production setup**:
```rust
// Checkpoint positions every 1000 ticks
if self.tick_count % 1000 == 0 {
    self.persist_position_to_disk()?;
}

// On restart:
fn restore_from_checkpoint() -> Position {
    // Read from disk
}
```

**Code location**: `bog-core/src/execution/simulated.rs:11` (uses ArrayQueue, not Vec)

### Probability

**Realistic**: Near zero (after Phase 2)
- All collections bounded
- Stack allocations dominate (Position, Signal, etc.)
- Typical memory usage: <50MB per market

**Before Phase 2**: High
- Unbounded Vec could grow to GB during backtests
- Multiple OOM kills observed during development

---

## 7. Network Failures (Production Executor)

### Description

Loss of network connectivity to exchange, causing order submission failures or fill reception delays.

### Impact

**Severity**: High (production), N/A (simulated)
- Orders not reaching exchange → missed opportunities
- Fills not received → position tracking incorrect
- Reconnection latency → stale market data
- Risk: "orphaned" orders (sent but not confirmed)

### Scenario

```
T+0ms: Engine submits order (buy 1 BTC)
T+1ms: TCP packet sent
[Network failure]
T+2ms: Packet dropped (no ack)
T+5ms: TCP timeout, retransmit
[Still no connectivity]
T+10ms: Connection marked dead

// Engine state: Order pending (waiting for confirmation)
// Exchange state: Order never received (or received but can't respond)
// Risk: Duplicate orders if reconnect and retry
```

### Detection

**Method 1**: Order timeout (primary)
```rust
const ORDER_TIMEOUT_MS: u64 = 100;  // 100ms

pub struct ProductionExecutor {
    pending_orders: HashMap<OrderId, (Signal, Instant)>,
}

// Periodically check for timeouts
for (order_id, (signal, submitted_at)) in &self.pending_orders {
    if submitted_at.elapsed() > Duration::from_millis(ORDER_TIMEOUT_MS) {
        error!("Order {} timed out after {}ms", order_id, ORDER_TIMEOUT_MS);
        // Retry or cancel
    }
}
```

**Method 2**: TCP socket state (secondary)
```rust
match self.exchange_client.send(fix_msg) {
    Ok(_) => { /* order sent */ },
    Err(std::io::ErrorKind::BrokenPipe) => {
        error!("Network connection broken");
        self.reconnect()?;
    }
    Err(e) => {
        error!("Send failed: {}", e);
    }
}
```

**Method 3**: Heartbeat monitoring (tertiary)
```rust
// FIX protocol heartbeat (every 30s)
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);

if self.last_heartbeat.elapsed() > HEARTBEAT_INTERVAL * 2 {
    error!("No heartbeat from exchange in {}s", HEARTBEAT_INTERVAL.as_secs() * 2);
    self.reconnect()?;
}
```

### Mitigation

**Prevention**:
- Redundant network paths (primary + backup connection)
- Monitor latency and packet loss (alert if >1%)
- Use TCP keepalive to detect dead connections

**Recovery**:
- Reconnect immediately on socket error
- Query exchange for order status (FIX "OrderStatusRequest")
- Cancel orphaned orders if status unknown
- Reconcile position with exchange after reconnect

**Idempotency**:
```rust
// Assign unique order ID before sending
let order_id = OrderId::new();  // UUID or timestamp-based

// If retry needed, use same ID
if self.pending_orders.contains_key(&order_id) {
    // Duplicate, exchange will reject (safe)
}
```

**Code location**: `bog-core/src/execution/production.rs:53-89`

### Probability

**Production**: Medium
- Network failures occur (fiber cut, switch failure)
- Typical: 1-2 incidents per month (99.9% uptime)
- Duration: Seconds to minutes

**Simulated**: Zero
- No network, no network failures

---

## 8. Race Conditions (Multi-Threading)

### Description

Concurrent access to shared mutable state without synchronization, causing data corruption or deadlock.

### Impact

**Severity**: Critical (if present)
- Position updates race → wrong position
- Orders race → duplicate or lost orders
- Deadlock → engine hangs
- Undefined behavior → crash or silent corruption

### Scenario (if multi-threaded, which bog is NOT)

```rust
// BAD: Two threads updating position
// Thread 1:
position.quantity += 100;  // Load, add, store

// Thread 2:
position.quantity += 200;  // Load, add, store

// Race: Both load same value, both store their result
// Expected: +300, Actual: +100 or +200 (one update lost)
```

### Detection

**Method 1**: Design (primary - prevention)
```rust
// bog is single-threaded per market
// Each market runs in its own process → no shared state
```

**Method 2**: Atomics (secondary - where needed)
```rust
// If multi-threaded, use AtomicI64
pub struct Position {
    pub quantity: AtomicI64,  // Thread-safe
}

position.quantity.fetch_add(100, Ordering::AcqRel);  // Atomic, no race
```

**Method 3**: Thread sanitizer (testing)
```bash
RUSTFLAGS="-Z sanitizer=thread" cargo test
# Detects races at runtime
```

### Mitigation

**Prevention**:
- Single-threaded design (bog's approach)
- Use atomics for multi-threaded access (Position does this)
- Minimize shared state (each market independent)

**If multi-threading needed**:
- Mutex for complex state (HashMap, Vec)
- Lock-free structures (crossbeam)
- Message passing (channels, not shared memory)

**Code location**: `bog-core/src/core/types.rs:137-176` (uses AtomicI64 defensively)

### Probability

**Realistic**: Zero
- Single-threaded design eliminates race conditions
- Atomic operations used defensively (future-proofing)

**If multi-threading added**: Medium
- Easy to introduce races without careful design

---

## 9. Strategy Logic Errors

### Description

Bug in strategy code causing incorrect signals, infinite loops, or panics.

### Impact

**Severity**: High
- Incorrect signals → bad trades → losses
- Panic → trading halted
- Infinite loop → engine hangs (no trades)

### Scenario

```rust
// Bug: Division by zero
impl Strategy for BrokenStrategy {
    fn generate_signal(&self, market: &MarketState, position: &Position)
        -> Option<Signal>
    {
        let mid = (market.bid + market.ask) / 2;
        let spread = market.ask - market.bid;

        // BUG: If spread is zero (halted market), division by zero!
        let ratio = mid / spread;  // PANIC!

        // ...
    }
}
```

### Detection

**Method 1**: Testing (primary)
```rust
#[test]
fn test_zero_spread() {
    let strategy = SimpleSpread;
    let market = MarketState {
        bid: 50_000_000_000_000,
        ask: 50_000_000_000_000,  // Same as bid (zero spread)
        // ...
    };
    let position = Position::new();

    // Should not panic
    let signal = strategy.generate_signal(&market, &position);
    assert!(signal.is_none());  // Or handle gracefully
}
```

**Method 2**: Panic handling (secondary)
```rust
use std::panic;

// Catch panics in strategy (don't crash engine)
let result = panic::catch_unwind(|| {
    strategy.generate_signal(&market, &position)
});

match result {
    Ok(signal) => { /* use signal */ },
    Err(e) => {
        error!("Strategy panicked: {:?}", e);
        // Skip this tick, continue trading
    }
}
```

**Method 3**: Fuzzing (tertiary)
```bash
# Fuzz test strategy with arbitrary inputs
cargo +nightly fuzz run fuzz_strategy
```

### Mitigation

**Prevention**:
- Comprehensive unit tests (edge cases: zero spread, negative prices, etc.)
- Property-based testing (proptest)
- Code review (second pair of eyes)
- Defensive programming (check preconditions)

**Recovery**:
- Catch panics and skip tick (don't crash)
- Circuit breaker: Halt after N consecutive panics (e.g., 10)
- Alert on first panic (investigate immediately)

**Code example** (defensive):
```rust
// Check for zero spread
if market.bid == market.ask {
    return None;  // Don't trade in halted market
}

// Check for negative prices (data corruption)
if market.bid <= 0 || market.ask <= 0 {
    error!("Invalid market state: bid={}, ask={}", market.bid, market.ask);
    return None;
}
```

**Code location**: All strategies (`bog-strategies/src/*.rs`)

### Probability

**Realistic**: Low (with testing)
- Simple strategies are easy to verify
- Unit tests catch most edge cases
- Fuzzing finds rare corner cases

**Without testing**: High
- Edge cases abound (zero spread, flash crashes, etc.)

---

## 10. Dependency Failures

### Description

External dependency (Huginn, exchange API, system library) fails or behaves unexpectedly.

### Impact

**Severity**: Varies
- Huginn down → No market data → Cannot trade
- Exchange API down → Cannot submit orders
- System library bug → Crash or undefined behavior

### Scenario

```
Huginn process crashes
→ SHM region still exists (/dev/shm/hg_m1)
→ bog continues reading stale data
→ Market data frozen at T-10s
→ bog generates signals based on old prices
→ Orders submitted at wrong prices
```

### Detection

**Method 1**: Sequence number validation (primary)
```rust
let market = shm_reader.read_market_state(1);

if market.seq == self.last_seq {
    warn!("Market data stale: seq={} (not updated)", market.seq);
    // Skip this tick
    return None;
}

self.last_seq = market.seq;
```

**Method 2**: Timestamp validation (secondary)
```rust
let now_ns = get_system_time_ns();
let age_ms = (now_ns - market.last_update_ns) / 1_000_000;

const MAX_AGE_MS: i64 = 1000;  // 1 second

if age_ms > MAX_AGE_MS {
    warn!("Market data stale: {}ms old (max: {}ms)", age_ms, MAX_AGE_MS);
    return None;
}
```

**Method 3**: Process monitoring (tertiary)
```bash
# Systemd watchdog for Huginn
[Service]
WatchdogSec=10s
Restart=always

# bog checks Huginn status
if ! pgrep huginn > /dev/null; then
    error "Huginn process not running"
    exit 1
fi
```

### Mitigation

**Prevention**:
- Monitor dependencies (Huginn, exchange)
- Validate data freshness (sequence numbers, timestamps)
- Fail fast if dependency unavailable

**Recovery**:
- Halt trading if Huginn down (don't use stale data)
- Retry exchange API calls (with exponential backoff)
- Alert on dependency failures

**Production setup**:
```yaml
# Monitoring
- alert: HuginnDown
  expr: up{job="huginn"} == 0
  severity: critical

- alert: StaleMarketData
  expr: time() - bog_last_market_update_timestamp > 5
  severity: warning
```

**Code location**: `bog-core/src/core/huginn_shm.rs:25-47`

### Probability

**Realistic**: Medium
- Huginn restarts occasionally (upgrades, crashes)
- Exchange APIs have downtime (maintenance, DDoS)
- Typical: <1% downtime for Huginn, <0.1% for major exchanges

---

## Summary Table

| Failure Mode | Severity | Probability | Mitigation Status | Phase |
|--------------|----------|-------------|-------------------|-------|
| Position overflow | Critical | Near zero | ✅ Protected (checked arithmetic) | 1 |
| Conversion errors | High | Low | ✅ Protected (checked conversion) | 1 |
| Fill queue overflow | High | Medium | ✅ Protected (bounded queue) | 2 |
| Flash crash | High | Medium | ⚠️ Partial (spread filter) | 5+ |
| Clock desync | Medium | Low | ✅ Protected (monotonic clocks) | N/A |
| Memory exhaustion | Critical | Near zero | ✅ Protected (bounded collections) | 2 |
| Network failures | High | Medium | ⚠️ Partial (needs reconnection logic) | 5+ |
| Race conditions | Critical | Zero | ✅ Protected (single-threaded) | N/A |
| Strategy errors | High | Low | ✅ Protected (testing + validation) | 3-4 |
| Dependency failures | Varies | Medium | ⚠️ Partial (needs monitoring) | 6 |

**Legend**:
- ✅ Protected: Mitigation implemented and tested
- ⚠️ Partial: Some mitigation, needs enhancement
- ❌ Unprotected: Known gap, needs work

---

## Incident Response

### On Overflow Detection

```bash
# Immediate actions:
1. Alert fires: "CRITICAL: Position overflow detected"
2. Engine halts trading automatically
3. Operator investigates:
   - Check position: `curl http://localhost:9090/metrics | grep position`
   - Check logs: `journalctl -u bog-simple-spread -n 1000`
   - Check exchange position: Query via API

4. Reconcile:
   - If bog position wrong: Update manually, restart
   - If exchange position wrong: Contact exchange support
   - If both wrong: Investigate root cause (data corruption?)

5. Resume:
   - Fix bug if found
   - Restart with correct position
   - Monitor closely for recurrence
```

### On Fill Queue Overflow

```bash
# Immediate actions:
1. Alert fires: "WARNING: High queue depth" or "CRITICAL: Fills dropped"
2. Check queue metrics:
   - `curl http://localhost:9090/metrics | grep queue_depth`
   - `curl http://localhost:9090/metrics | grep dropped_fills`

3. Investigate:
   - Is tick rate too high? (Backtesting at 1000x speed?)
   - Is processing slow? (CPU pinning lost?)
   - Is fill rate realistic? (Simulated executor too optimistic?)

4. Mitigate:
   - Slow down backtest (reduce tick rate)
   - Add backpressure (skip signal generation if queue full)
   - Increase queue size if needed (MAX_PENDING_FILLS)

5. Resume:
   - Continue with adjusted parameters
   - Monitor queue depth continuously
```

### On Flash Crash

```bash
# Immediate actions:
1. Strategy detects wide spread (>1%)
2. Skips tick (no signal generated)
3. Logs: "WARN: Spread too wide: 500 bps (max: 100)"

4. Monitor:
   - Watch for spread normalization
   - Cancel any open orders (reduce exposure)
   - Wait for N consecutive normal ticks (e.g., 10)

5. Resume:
   - Strategy automatically resumes when spread < 100 bps
   - No operator intervention needed (self-healing)
```

---

## References

- [Overflow Handling Architecture](../architecture/overflow-handling.md)
- [System Design](../architecture/system-design.md)
- [Latency Budget](../performance/latency-budget.md)
- [Monitoring](./monitoring.md) (future)
