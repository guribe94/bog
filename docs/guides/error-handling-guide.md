# Error Handling Guide

Critical error handling patterns for production HFT trading. Focus: preventing money loss and ensuring system safety.

## Error Categories

### 1. **CRITICAL** - Stop Trading Immediately

These errors can lose money. Engine must halt.

```rust
// Position overflow - arithmetic wraparound could corrupt tracking
position.update_quantity_checked(delta)?;

// Daily loss limit breached
if position.get_daily_pnl() < -MAX_DAILY_LOSS {
    circuit_breaker.trip(BreakerReason::DailyLossLimit)?;
    return Err(anyhow!("Daily loss limit breached"));
}

// Fill queue overflow - position tracking may be wrong
if executor.dropped_fill_count() > 0 {
    circuit_breaker.trip(BreakerReason::FillQueueOverflow)?;
    return Err(anyhow!("Fills dropped - position uncertain"));
}
```

**Recovery:** Manual intervention required. Reconcile position with exchange.

### 2. **WARNING** - Continue with Caution

Non-fatal but needs attention.

```rust
// Sequence gap detected - may have missed market data
if let Some(gap) = check_sequence_gap(snapshot.sequence) {
    warn!("Gap detected: {} messages missed", gap);
    // Gap recovery will handle this automatically
}

// Stale data - old snapshot
let age_ns = now_ns - snapshot.local_recv_ns;
if age_ns > STALE_THRESHOLD_NS {
    warn!("Stale data: {}μs old", age_ns / 1000);
    // Post-stale validation will apply
}

// Crossed market - exchange issue
if snapshot.best_bid_price >= snapshot.best_ask_price {
    warn!("Crossed market detected");
    return Some(Signal::cancel_all());
}
```

**Recovery:** Automatic. System handles via gap recovery or cancels quotes.

### 3. **INFO** - Expected Behavior

Normal trading conditions.

```rust
// No action needed
if !market_changed {
    return Ok(()); // Fast path - common case
}

// Market spread too tight
if spread_bps < MIN_PROFITABLE_SPREAD_BPS {
    return Some(Signal::no_action());
}
```

**Recovery:** None needed.

---

## Critical Error Patterns

### Pattern 1: Position Overflow Protection

**Problem:** i64 wraparound corrupts position tracking.

```rust
// ❌ UNSAFE: Silent wraparound
let new_qty = position.quantity.fetch_add(delta, Ordering::AcqRel);
// If old=i64::MAX and delta=1, new_qty wraps to i64::MIN

// ✅ SAFE: Detect overflow
let new_qty = position.update_quantity_checked(delta)
    .context("Position quantity overflow")?;
```

**Detection:**
```rust
// Engine monitors this
if position.update_quantity_checked(delta).is_err() {
    circuit_breaker.trip(BreakerReason::PositionOverflow)?;
}
```

### Pattern 2: Fill Queue Overflow

**Problem:** Dropped fills = position tracking wrong.

```rust
// Check after every get_fills() call
let fills = executor.get_fills();
let dropped = executor.dropped_fill_count();

if dropped > 0 {
    error!("CRITICAL: {} fills dropped - position may be incorrect!", dropped);
    circuit_breaker.trip(BreakerReason::FillQueueOverflow)?;

    // STOP TRADING - manual reconciliation needed
    return Err(anyhow!("Position tracking corrupted"));
}
```

**Prevention:**
- Increase fill queue size (default: 1024)
- Process fills frequently
- Monitor queue depth: `executor.fill_queue_depth()`

### Pattern 3: Data Staleness

**Problem:** Trading on old data loses money.

```rust
const STALE_THRESHOLD_NS: u64 = 100_000; // 100μs

let age_ns = current_time_ns - snapshot.local_recv_ns;

if age_ns > STALE_THRESHOLD_NS {
    warn!("Stale data: {}μs old", age_ns / 1000);

    // Apply post-stale validation
    if was_recently_stale {
        let price_change_bps = calculate_price_change_bps(
            last_fresh_mid,
            current_mid,
        );

        if price_change_bps > MAX_POST_STALE_CHANGE_BPS {
            // Market moved too much while stale - cancel quotes
            return Some(Signal::cancel_all());
        }
    }
}
```

**Detection:** Engine tracks staleness automatically via `was_stale` flag.

### Pattern 4: Sequence Gaps

**Problem:** Missed market data can cause bad quotes.

```rust
// Gap detection
let expected_seq = self.last_sequence + 1;
if snapshot.sequence != expected_seq {
    let gap = snapshot.sequence - expected_seq;
    warn!("Sequence gap: {} messages missed", gap);

    // Automatic recovery
    gap_recovery_manager.handle_gap(gap, snapshot)?;
}

self.last_sequence = snapshot.sequence;
```

**Recovery:** Gap recovery manager handles automatically:
1. Cancel all outstanding orders
2. Request full snapshot
3. Resume trading when recovered

---

## Circuit Breaker Usage

The circuit breaker prevents runaway losses.

```rust
use bog_core::risk::circuit_breaker::{CircuitBreaker, BreakerState};

let mut circuit_breaker = CircuitBreaker::new();

// Check before every trade
match circuit_breaker.can_trade()? {
    BreakerState::Closed => {
        // Normal trading
        executor.execute(signal, position)?;
    }
    BreakerState::Open => {
        warn!("Circuit breaker OPEN - trading halted");
        return Ok(());
    }
    BreakerState::HalfOpen => {
        // Testing if we can resume
        info!("Circuit breaker HALF_OPEN - limited trading");
    }
}

// Trip on critical errors
if critical_condition {
    circuit_breaker.trip(BreakerReason::DailyLossLimit)?;
}
```

**Reasons to trip:**
- Daily loss limit breached
- Position overflow detected
- Fill queue overflow
- Repeated execution failures
- Extreme position size

**Recovery:** Automatic retry after cooldown (default: 30s).

---

## Position Reconciliation

Detect and fix position drift.

```rust
use bog_core::engine::position_reconciliation::{
    PositionReconciler, ReconciliationConfig
};

let config = ReconciliationConfig {
    check_interval_trades: 100,  // Check every 100 trades
    drift_threshold: 1_000_000,  // 0.001 BTC tolerance
    max_corrections: 10,
};

let mut reconciler = PositionReconciler::new(config);

// After processing fills
reconciler.check_and_reconcile(
    position,
    expected_qty,
    "After fill processing",
)?;

// Get stats
let stats = reconciler.stats();
if stats.corrections > 0 {
    warn!("Position corrections: {}", stats.corrections);
}
```

**When to reconcile:**
- Every N trades
- After sequence gaps
- After system restart
- Before daily settlement

---

## Error Handling Checklist

### Strategy Implementation

- [ ] Return `Signal::no_action()` on invalid market data
- [ ] Check spread >= `MIN_PROFITABLE_SPREAD_BPS`
- [ ] Validate bid < ask (detect crossed markets)
- [ ] Handle zero prices gracefully

### Executor Implementation

- [ ] Use `update_quantity_checked()` for position updates
- [ ] Check `dropped_fill_count()` after every `get_fills()`
- [ ] Return errors for critical failures (don't swallow)
- [ ] Implement proper fill queue overflow handling

### Engine Integration

- [ ] Check circuit breaker state before trading
- [ ] Monitor data staleness via timestamp
- [ ] Track sequence gaps
- [ ] Enable position reconciliation
- [ ] Set appropriate risk limits via Cargo features

### Production Deployment

- [ ] Configure circuit breaker thresholds
- [ ] Set fill queue size based on trading frequency
- [ ] Enable gap recovery manager
- [ ] Configure alerts for critical errors
- [ ] Set up position reconciliation monitoring

---

## Common Mistakes

### ❌ Mistake 1: Ignoring Result Types

```rust
// BAD: Error silently ignored
let _ = position.update_quantity_checked(delta);

// GOOD: Error propagated
position.update_quantity_checked(delta)?;
```

### ❌ Mistake 2: Not Checking Dropped Fills

```rust
// BAD: Position may be wrong
let fills = executor.get_fills();
for fill in fills {
    position.process_fill_fixed_with_fee(...)?;
}

// GOOD: Detect corruption
let fills = executor.get_fills();
if executor.dropped_fill_count() > 0 {
    return Err(anyhow!("Fills dropped"));
}
for fill in fills {
    position.process_fill_fixed_with_fee(...)?;
}
```

### ❌ Mistake 3: Trading on Invalid Data

```rust
// BAD: No validation
let signal = strategy.calculate(snapshot, position);

// GOOD: Validate first
if snapshot.best_bid_price == 0 || snapshot.best_ask_price == 0 {
    return Some(Signal::no_action());
}
if snapshot.best_bid_price >= snapshot.best_ask_price {
    warn!("Crossed market");
    return Some(Signal::cancel_all());
}
let signal = strategy.calculate(snapshot, position);
```

### ❌ Mistake 4: Not Monitoring Limits

```rust
// BAD: No limit checks
executor.execute(signal, position)?;

// GOOD: Pre-trade validation
if position.get_quantity().abs() >= MAX_POSITION {
    return Err(anyhow!("Position limit would be exceeded"));
}
if position.get_daily_pnl() < -MAX_DAILY_LOSS {
    circuit_breaker.trip(BreakerReason::DailyLossLimit)?;
}
executor.execute(signal, position)?;
```

---

## Monitoring Critical Errors

### Metrics to Track

```rust
// Circuit breaker trips
metrics.circuit_breaker_trips.inc();

// Fill queue overflows
if dropped > 0 {
    metrics.fill_queue_overflows.inc();
}

// Position reconciliation corrections
if corrected {
    metrics.position_corrections.inc();
}

// Sequence gaps
metrics.sequence_gaps.observe(gap_size);

// Data staleness
metrics.stale_data_age_us.observe(age_us);
```

### Alerts to Configure

1. **CRITICAL** (page immediately):
   - Circuit breaker tripped
   - Fill queue overflow
   - Position reconciliation failed

2. **WARNING** (review within 5min):
   - Repeated sequence gaps (>10/min)
   - Persistent data staleness (>100μs)
   - Position corrections (>5/hour)

3. **INFO** (review daily):
   - Circuit breaker half-open attempts
   - Gap recovery invocations
   - Stale data incidents

---

## Recovery Procedures

### Position Mismatch Detected

```bash
# 1. Stop trading immediately
pkill -f bog-simple-spread

# 2. Query exchange position
curl https://api.lighter.xyz/positions

# 3. Compare with local state
grep "position_quantity" data/execution.jsonl | tail -1

# 4. Manual reconciliation if needed
# Update position in code or adjust via exchange

# 5. Restart with clean state
./bog-simple-spread-live --reset-position
```

### Fill Queue Overflow

```bash
# 1. System has stopped automatically (circuit breaker)

# 2. Check logs for dropped fill count
grep "fills dropped" logs/bog.log

# 3. Reconcile position with exchange
# (see Position Mismatch procedure)

# 4. Increase fill queue size
# Edit Cargo.toml or executor config
# FILL_QUEUE_SIZE=2048

# 5. Restart
./bog-simple-spread-live
```

### Sequence Gap Storm

```bash
# 1. Check if Huginn feed is healthy
systemctl status huginn

# 2. Check network connectivity
ping -c 10 lighter-api.xyz

# 3. Review gap recovery stats
grep "gap_recovery" logs/bog.log

# 4. If gaps persist, restart Huginn
systemctl restart huginn

# 5. Monitor for stability
watch -n 1 'grep "Sequence gap" logs/bog.log | tail -5'
```

---

## Further Reading

- [Circuit Breaker Implementation](../../bog-core/src/risk/circuit_breaker.rs)
- [Position Reconciliation](../../bog-core/src/engine/position_reconciliation.rs)
- [Gap Recovery Manager](../../bog-core/src/engine/gap_recovery.rs)
- [Risk Management](../../bog-core/src/risk/mod.rs)
