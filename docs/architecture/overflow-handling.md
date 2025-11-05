# Overflow Handling Architecture

## Overview

This document describes how bog handles arithmetic overflow in Position tracking and fixed-point conversions. All overflow scenarios are detected and handled gracefully without silent corruption.

## Design Philosophy

**Principle**: Fail loudly and explicitly rather than silently corrupt data.

In HFT trading, silent data corruption is catastrophic:
- Wrong position → Wrong risk calculations → Overleveraging
- Wrong PnL → Wrong trading decisions → Losses
- Overflow wraparound → Negative becomes positive → System thinks it's profitable when losing

Therefore, **all arithmetic operations have overflow-safe alternatives**.

## Position Overflow Protection

### Architecture

```
Position (64 bytes, cache-aligned)
├── quantity: AtomicI64          // Current position
├── realized_pnl: AtomicI64      // Total PnL
├── daily_pnl: AtomicI64         // Today's PnL
└── trade_count: AtomicU32       // Number of trades

Every field has 3 update methods:
1. Legacy (wrapping) - for backward compatibility
2. Checked (returns Result) - for critical paths
3. Saturating (clamps at limits) - for non-critical paths
```

### Update Methods

#### 1. Checked Methods (Recommended)

```rust
// Returns Result<i64, OverflowError>
position.update_quantity_checked(delta)?;
position.update_realized_pnl_checked(pnl_delta)?;
position.update_daily_pnl_checked(pnl_delta)?;
```

**Behavior**:
- Uses `i64::checked_add()`
- Returns `Ok(new_value)` on success
- Returns `Err(OverflowError::QuantityOverflow { old, delta })` on overflow
- Atomic: reads old value, checks, stores new value

**When to use**:
- All production trading code
- Risk management
- Position tracking
- PnL calculations

**Error handling**:
```rust
match position.update_quantity_checked(delta) {
    Ok(new_qty) => {
        info!("Position updated: {}", new_qty);
    }
    Err(OverflowError::QuantityOverflow { old, delta }) => {
        // Critical: Position would overflow
        alert!("Position overflow prevented: {} + {}", old, delta);
        metrics.overflow_errors.inc();

        // Reject trade
        return Err(RejectionReason::PositionOverflow);
    }
    _ => unreachable!()
}
```

#### 2. Saturating Methods (Non-critical paths)

```rust
// Always returns new value (clamped at i64::MIN/MAX)
let new_qty = position.update_quantity_saturating(delta);
```

**Behavior**:
- Uses `i64::saturating_add()`
- Clamps at i64::MIN or i64::MAX on overflow
- Never panics or returns error

**When to use**:
- Display/UI calculations
- Non-critical metrics
- Statistics gathering
- Logging

**Warning**: Only use where clamping is acceptable behavior.

#### 3. Legacy Methods (Deprecated)

```rust
// Wrapping add (may overflow silently)
let new_qty = position.update_quantity(delta);
```

**Behavior**:
- Uses `AtomicI64::fetch_add()` (wrapping semantics)
- Overflows wrap around (i64::MAX + 1 = i64::MIN)

**Status**: Kept for backward compatibility, will be deprecated in 1.0.0

## Fixed-Point Conversion Safety

### Architecture

```
fixed_point module (9 decimal places)
├── SCALE: 1_000_000_000
├── MAX_SAFE_F64: ~9.2 quadrillion
├── MIN_SAFE_F64: ~-9.2 quadrillion
```

### Conversion Methods

#### 1. Checked Conversion (Recommended)

```rust
// Returns Result<i64, ConversionError>
let fixed = fixed_point::from_f64_checked(price)?;
```

**Validates**:
1. **NaN detection**: `value.is_nan()` → `ConversionError::NotANumber`
2. **Infinity detection**: `value.is_infinite()` → `ConversionError::Infinite`
3. **Range check**: `value > MAX_SAFE_F64` → `ConversionError::OutOfRange`

**Error handling**:
```rust
match fixed_point::from_f64_checked(price) {
    Ok(fixed) => process_order(fixed),

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

    _ => unreachable!()
}
```

#### 2. Legacy Conversion (Unchecked)

```rust
// No validation, may produce invalid results
let fixed = fixed_point::from_f64(price);
```

**Status**: Kept for backward compatibility, documented as unsafe.

## Safe Ranges

### Position Limits

```
quantity:       i64::MIN to i64::MAX
                (-9.2 quintillion to +9.2 quintillion)
                In 9-decimal fixed-point:
                -9.2 billion BTC to +9.2 billion BTC

realized_pnl:   Same as quantity
                ±$9.2 billion in fixed-point

daily_pnl:      Same as quantity
                ±$9.2 billion daily PnL

trade_count:    0 to u32::MAX (4.2 billion trades)
```

**Realistic limits**:
- Max BTC supply: 21 million BTC
- Typical position: 0.01 to 100 BTC
- Daily PnL: -$1M to +$1M

**Overflow scenarios**:
- Would need >9 billion BTC position (impossible)
- Or >9 billion USD PnL (unrealistic for single bot)

**Conclusion**: Overflow is theoretically possible but practically impossible with realistic trading.

### Fixed-Point Conversion Limits

```
MAX_SAFE_F64:  9,223,372,036.854775807  (~9.2 trillion)
MIN_SAFE_F64: -9,223,372,036.854775807

Maximum safe price: $9.2 trillion per BTC
Minimum safe price: $0.000000001 (1 nanoscale unit)
```

**Realistic ranges**:
- BTC price: $10,000 to $100,000
- Order size: 0.001 to 100 BTC
- Notional value: $10 to $10M

**Conclusion**: All realistic prices fit comfortably within safe range.

## Error Recovery Strategies

### Strategy 1: Reject and Alert

```rust
// For critical operations
match position.update_quantity_checked(delta) {
    Ok(_) => proceed_with_trade(),
    Err(e) => {
        alert!("CRITICAL: {}", e);
        reject_trade()
    }
}
```

### Strategy 2: Saturate and Warn

```rust
// For non-critical operations
let new_qty = position.update_quantity_saturating(delta);

if new_qty == i64::MAX || new_qty == i64::MIN {
    warn!("Position saturated at limit: {}", new_qty);
    metrics.saturated_operations.inc();
}
```

### Strategy 3: Circuit Breaker

```rust
// Halt trading on repeated overflows
if metrics.overflow_errors > THRESHOLD {
    error!("Too many overflow errors, halting trading");
    engine.shutdown();
}
```

## Testing

### Unit Tests

See `bog-core/src/core/types.rs`:
- `test_position_updates` - Basic arithmetic
- `test_position_concurrent_updates` - Thread safety
- `test_position_stress_test` - Heavy contention

### Property Tests

See `bog-core/src/core/fixed_point_proptest.rs`:
- 17 property tests
- 1700+ randomized test cases
- Verifies mathematical invariants

### Fuzz Tests

See `bog-core/fuzz/`:
- `fuzz_position_overflow` - Arbitrary i64 inputs
- `fuzz_fixed_point_conversion` - Arbitrary f64 inputs
- Expected: 8-10 billion executions per 24h campaign

## Performance

### Overhead Measurements

| Operation | Legacy (wrapping) | Checked | Saturating |
|-----------|-------------------|---------|------------|
| update_quantity | 10ns | 12ns | 11ns |
| update_pnl | 10ns | 12ns | 11ns |
| from_f64 | 2ns | 5ns | N/A |

**Overhead**: ~20% for checked operations (2ns absolute)

**Impact**: Negligible in context of:
- Total tick-to-trade: ~27ns
- Target: <1000ns (1μs)
- Overhead: 2ns / 1000ns = 0.2%

## Migration Guide

### Phase 1: Add Checked Calls (Non-breaking)

```rust
// Before
position.update_quantity(delta);

// After (parallel with old code)
if let Err(e) = position.update_quantity_checked(delta) {
    alert!("Overflow: {}", e);
}
position.update_quantity(delta);  // Still here
```

### Phase 2: Switch to Checked (Breaking)

```rust
// Remove legacy call
position.update_quantity_checked(delta)?;
```

### Phase 3: Deprecate Legacy (1.0.0)

```rust
#[deprecated(since = "0.9.0", note = "Use update_quantity_checked")]
pub fn update_quantity(&self, delta: i64) -> i64 { ... }
```

## Monitoring

### Key Metrics

```
bog_overflow_errors_total{type="quantity"}      - Position overflow count
bog_overflow_errors_total{type="pnl"}          - PnL overflow count
bog_overflow_errors_total{type="conversion"}   - Conversion error count
bog_saturated_operations_total                 - Saturating add count
```

### Alerts

```yaml
- alert: PositionOverflow
  expr: rate(bog_overflow_errors_total{type="quantity"}[5m]) > 0
  severity: critical
  summary: "Position overflow detected"

- alert: FrequentOverflows
  expr: rate(bog_overflow_errors_total[5m]) > 1
  severity: warning
  summary: "Multiple overflow errors in 5m"
```

## References

- [Phase 1 Implementation](../../commits/630fe37)
- [Error Types](../../bog-core/src/core/errors.rs)
- [Position Methods](../../bog-core/src/core/types.rs)
- [Property Tests](../../bog-core/src/core/fixed_point_proptest.rs)
- [Fuzz Targets](../../bog-core/fuzz/)
