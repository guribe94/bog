# Production Monitoring Guide

Critical metrics and alerts for HFT trading operations.

## Quick Reference

**Latency Targets:**
- Tick processing: <1μs
- Strategy calculation: <100ns
- Execution: <200ns
- End-to-end (tick-to-trade): <1μs

**Health Checks:**
- Feed latency: <100μs
- Position drift: <0.001 BTC
- Fill queue depth: <80% full
- Circuit breaker: CLOSED

---

## Critical Metrics

### 1. Latency Metrics

```rust
// Tick processing latency (CRITICAL)
histogram!(
    "bog.tick_processing_ns",
    tick_latency_ns as f64,
    "target" => "1000"  // 1μs target
);

// Strategy calculation latency
histogram!(
    "bog.strategy_calc_ns",
    calc_latency_ns as f64,
    "strategy" => strategy.name()
);

// Execution latency
histogram!(
    "bog.execution_ns",
    exec_latency_ns as f64,
);
```

**Alert Thresholds:**
- P50 > 1μs: WARNING
- P99 > 10μs: CRITICAL
- Any tick > 100μs: CRITICAL

### 2. Trading Activity

```rust
// Signals generated
counter!("bog.signals_total", "action" => "quote_both");
counter!("bog.signals_total", "action" => "no_action");

// Orders placed
counter!("bog.orders_placed", "side" => "bid");
counter!("bog.orders_placed", "side" => "ask");

// Fills received
counter!("bog.fills_received", "side" => side.to_string());

// Volume traded (USD)
histogram!("bog.volume_usd", volume_usd);
```

**Alert Thresholds:**
- No fills for >60s: WARNING
- No signals for >10s: WARNING
- Orders placed but no fills for >30s: WARNING

### 3. Position & PnL

```rust
// Current position
gauge!("bog.position_btc", position_btc);

// Realized PnL
gauge!("bog.realized_pnl_usd", realized_pnl_usd);

// Daily PnL
gauge!("bog.daily_pnl_usd", daily_pnl_usd);

// Trade count
counter!("bog.trades_total");
```

**Alert Thresholds:**
- |position| > 90% of MAX_POSITION: WARNING
- daily_pnl < -80% of MAX_DAILY_LOSS: WARNING
- daily_pnl < -100% of MAX_DAILY_LOSS: CRITICAL (should be circuit broken)

### 4. Data Quality

```rust
// Feed latency (exchange timestamp to local receipt)
histogram!(
    "bog.feed_latency_us",
    (snapshot.local_recv_ns - snapshot.exchange_timestamp_ns) / 1000
);

// Sequence gaps
counter!("bog.sequence_gaps_total");
histogram!("bog.sequence_gap_size", gap_size);

// Stale data incidents
counter!("bog.stale_data_total");
histogram!("bog.stale_data_age_us", age_us);
```

**Alert Thresholds:**
- Feed latency P99 > 1ms: WARNING
- Sequence gaps > 5/min: WARNING
- Stale data age > 100μs: WARNING

### 5. System Health

```rust
// Circuit breaker state
gauge!("bog.circuit_breaker_state", state as f64);
// 0 = Closed (normal), 1 = Half-Open, 2 = Open (tripped)

// Fill queue depth
gauge!("bog.fill_queue_depth", queue_len);
gauge!("bog.fill_queue_capacity", QUEUE_CAPACITY);

// Dropped fills (CRITICAL)
counter!("bog.fills_dropped_total");

// Position corrections
counter!("bog.position_corrections_total");
```

**Alert Thresholds:**
- circuit_breaker_state = 2: CRITICAL
- fill_queue_depth > 80%: WARNING
- fills_dropped > 0: CRITICAL
- position_corrections > 5/hour: WARNING

---

## Prometheus Configuration

### scrape_config.yml

```yaml
scrape_configs:
  - job_name: 'bog-trading'
    scrape_interval: 1s  # High frequency for HFT
    static_configs:
      - targets: ['localhost:9090']
        labels:
          instance: 'bog-prod-1'
          strategy: 'simple-spread'
```

### Recording Rules

```yaml
groups:
  - name: bog_latency
    interval: 10s
    rules:
      # P50 tick latency
      - record: bog:tick_processing_ns:p50
        expr: histogram_quantile(0.5, rate(bog_tick_processing_ns_bucket[1m]))

      # P99 tick latency
      - record: bog:tick_processing_ns:p99
        expr: histogram_quantile(0.99, rate(bog_tick_processing_ns_bucket[1m]))

      # Ticks per second
      - record: bog:ticks_per_second
        expr: rate(bog_ticks_total[1m])

  - name: bog_trading
    interval: 10s
    rules:
      # Fill rate
      - record: bog:fills_per_minute
        expr: rate(bog_fills_received[1m]) * 60

      # PnL velocity ($/min)
      - record: bog:pnl_velocity_usd_per_min
        expr: deriv(bog_daily_pnl_usd[5m]) * 60
```

---

## Alert Rules

### alerting_rules.yml

```yaml
groups:
  - name: bog_critical
    rules:
      # Circuit breaker tripped
      - alert: CircuitBreakerTripped
        expr: bog_circuit_breaker_state == 2
        for: 0s
        labels:
          severity: critical
        annotations:
          summary: "Trading halted - circuit breaker OPEN"
          description: "Circuit breaker tripped. Manual intervention required."

      # Fill queue overflow
      - alert: FillQueueOverflow
        expr: bog_fills_dropped_total > 0
        for: 0s
        labels:
          severity: critical
        annotations:
          summary: "Fills dropped - position tracking corrupted"
          description: "{{ $value }} fills dropped. Stop trading and reconcile position."

      # Extreme latency
      - alert: ExtremeLatency
        expr: bog:tick_processing_ns:p99 > 10000
        for: 30s
        labels:
          severity: critical
        annotations:
          summary: "P99 latency > 10μs"
          description: "Tick processing too slow: {{ $value }}ns"

  - name: bog_warning
    rules:
      # Position limit approaching
      - alert: PositionLimitNear
        expr: abs(bog_position_btc) > 0.9 * MAX_POSITION
        for: 10s
        labels:
          severity: warning
        annotations:
          summary: "Position near limit"
          description: "Position {{ $value }} BTC (limit: {{ MAX_POSITION }})"

      # Daily loss approaching limit
      - alert: DailyLossNear
        expr: bog_daily_pnl_usd < -0.8 * MAX_DAILY_LOSS
        for: 30s
        labels:
          severity: warning
        annotations:
          summary: "Daily loss near limit"
          description: "Daily PnL: ${{ $value }} (limit: -${{ MAX_DAILY_LOSS }})"

      # High sequence gap rate
      - alert: SequenceGapStorm
        expr: rate(bog_sequence_gaps_total[1m]) > 10
        for: 1m
        labels:
          severity: warning
        annotations:
          summary: "Frequent sequence gaps"
          description: "{{ $value }} gaps/sec. Check Huginn feed."

      # No trading activity
      - alert: NoFills
        expr: rate(bog_fills_received[1m]) == 0
        for: 60s
        labels:
          severity: warning
        annotations:
          summary: "No fills for 60s"
          description: "Strategy may not be generating signals or market is illiquid."
```

---

## Grafana Dashboards

### Main Trading Dashboard

**Panels:**

1. **Latency (Row 1)**
   - Tick processing: P50, P99, Max (line chart)
   - Strategy calc latency histogram
   - Execution latency histogram

2. **Trading Activity (Row 2)**
   - Signals/min by type (stacked area)
   - Orders placed/min (bar chart)
   - Fills/min (line chart)

3. **Position & PnL (Row 3)**
   - Current position (gauge, with limits)
   - Daily PnL (stat panel, color-coded)
   - Realized PnL (line chart over time)

4. **System Health (Row 4)**
   - Circuit breaker state (stat panel)
   - Fill queue depth (gauge, % of capacity)
   - Position corrections (counter)

5. **Data Quality (Row 5)**
   - Feed latency P99 (line chart)
   - Sequence gaps/min (line chart)
   - Stale data incidents/min (line chart)

### JSON Export

```json
{
  "dashboard": {
    "title": "Bog Trading - Production",
    "panels": [
      {
        "title": "Tick Processing Latency",
        "targets": [
          {
            "expr": "bog:tick_processing_ns:p50",
            "legendFormat": "P50"
          },
          {
            "expr": "bog:tick_processing_ns:p99",
            "legendFormat": "P99"
          }
        ],
        "yaxes": [
          {
            "label": "Latency (ns)",
            "format": "ns"
          }
        ],
        "alert": {
          "conditions": [
            {
              "evaluator": {
                "params": [1000],
                "type": "gt"
              },
              "query": {
                "params": ["A", "5m", "now"]
              },
              "reducer": {
                "type": "avg"
              },
              "type": "query"
            }
          ],
          "name": "Latency SLA Violation"
        }
      }
    ]
  }
}
```

---

## Health Check Endpoint

```rust
use axum::{Router, routing::get, Json};
use serde::Serialize;

#[derive(Serialize)]
struct HealthStatus {
    status: &'static str,
    tick_latency_p99_ns: u64,
    circuit_breaker: &'static str,
    fill_queue_depth: f64,
    position_drift: f64,
}

async fn health_check() -> Json<HealthStatus> {
    let latency_ok = TICK_LATENCY_P99.load() < 10_000;
    let breaker_ok = CIRCUIT_BREAKER_STATE.load() == 0;
    let queue_ok = FILL_QUEUE_DEPTH.load() < 0.8;
    let position_ok = POSITION_DRIFT.load() < 0.001;

    let status = if latency_ok && breaker_ok && queue_ok && position_ok {
        "healthy"
    } else {
        "degraded"
    };

    Json(HealthStatus {
        status,
        tick_latency_p99_ns: TICK_LATENCY_P99.load(),
        circuit_breaker: match CIRCUIT_BREAKER_STATE.load() {
            0 => "CLOSED",
            1 => "HALF_OPEN",
            2 => "OPEN",
            _ => "UNKNOWN",
        },
        fill_queue_depth: FILL_QUEUE_DEPTH.load() as f64 / FILL_QUEUE_CAPACITY as f64,
        position_drift: POSITION_DRIFT.load(),
    })
}

let app = Router::new().route("/health", get(health_check));
```

**Query:**
```bash
curl http://localhost:9090/health

{
  "status": "healthy",
  "tick_latency_p99_ns": 450,
  "circuit_breaker": "CLOSED",
  "fill_queue_depth": 0.12,
  "position_drift": 0.0001
}
```

---

## Log Monitoring

### Critical Log Patterns

```bash
# Circuit breaker trips
grep "Circuit breaker" logs/bog.log

# Fill queue overflows
grep "fills dropped" logs/bog.log

# Position corrections
grep "Position correction" logs/bog.log

# Sequence gaps
grep "Sequence gap" logs/bog.log

# Stale data warnings
grep "Stale data" logs/bog.log
```

### Log Aggregation (journald)

```bash
# Follow trading logs
journalctl -u bog-trading -f

# Filter by severity
journalctl -u bog-trading -p err

# Filter by time range
journalctl -u bog-trading --since "10 minutes ago"

# Search for pattern
journalctl -u bog-trading | grep "CRITICAL"
```

---

## Performance Baseline

### Healthy System (5-minute average)

```
Latency:
  tick_processing_ns P50: 380ns
  tick_processing_ns P99: 550ns
  strategy_calc_ns P50: 5ns
  execution_ns P50: 50ns

Trading:
  ticks_per_second: 100-500
  signals_per_minute: 60-300
  fills_per_minute: 10-50
  volume_usd_per_hour: $10k-$100k

Data Quality:
  feed_latency_us P99: 200μs
  sequence_gaps_per_hour: <5
  stale_data_per_hour: <10

Health:
  circuit_breaker: CLOSED
  fill_queue_depth: 5-20%
  position_corrections_per_day: 0-2
```

### Degraded System (investigate immediately)

```
Latency:
  tick_processing_ns P99: >2μs
  Any tick >10μs

Trading:
  No fills for >60s
  No signals for >30s

Data Quality:
  feed_latency_us P99: >1ms
  sequence_gaps_per_minute: >5
  stale_data age: >100μs frequently

Health:
  circuit_breaker: OPEN
  fill_queue_depth: >80%
  fills_dropped: >0
  position_corrections: >5/hour
```

---

## Runbook

### Latency Spike (P99 > 10μs)

```bash
# 1. Check CPU pinning
taskset -cp $(pgrep bog)

# 2. Check system load
uptime
top -b -n 1 | head -20

# 3. Check for CPU throttling
cat /proc/cpuinfo | grep MHz

# 4. Review recent code changes
git log --oneline -10

# 5. Profile the hot path
perf record -g -p $(pgrep bog) -- sleep 10
perf report
```

### No Fills for >60s

```bash
# 1. Check if signals are generating
grep "Signal::" logs/bog.log | tail -20

# 2. Check market spread
curl http://localhost:9090/metrics | grep bog_spread_bps

# 3. Check position limits
curl http://localhost:9090/metrics | grep bog_position

# 4. Check circuit breaker
curl http://localhost:9090/health

# 5. Review strategy parameters
grep "SPREAD_BPS\|MIN_SPREAD" Cargo.toml
```

### Fill Queue Depth >80%

```bash
# 1. Check fill processing rate
curl http://localhost:9090/metrics | grep bog_fills_received

# 2. Check for fills dropped
curl http://localhost:9090/metrics | grep bog_fills_dropped

# 3. Increase queue size (requires restart)
# Edit executor config: FILL_QUEUE_SIZE=2048

# 4. Reduce trading frequency (temporary)
# Adjust strategy parameters

# 5. Restart with larger queue
systemctl restart bog-trading
```

---

## Further Reading

- [MetricsRegistry Implementation](../../bog-core/src/monitoring/metrics.rs)
- [Alert Rules](../../bog-core/src/monitoring/alert_rules.rs)
- [Error Handling Guide](./error-handling-guide.md)
- [Prometheus Best Practices](https://prometheus.io/docs/practices/)
