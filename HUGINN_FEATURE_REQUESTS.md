# Huginn Feature Requests from Bog Integration

This document outlines feature requests discovered during the integration audit of bog with huginn v0.4.1.

## Summary

Based on hands-on integration experience with bog (a production HFT trading system), the following features would improve the huginn API and developer experience for consumers.

---

## FR1: Health Check API (HIGH Priority)

### Problem
Consumers currently infer feed health by monitoring:
- Stale data (no updates for N seconds)
- Sequence gaps (missing messages)
- Offline status (connection lost)

This requires consumers to implement their own health state machines.

### Proposed Solution
```rust
pub enum HealthStatus {
    Healthy,
    Degraded { reason: String },
    Offline,
}

pub struct FeedHealth {
    pub status: HealthStatus,
    pub last_message_age_ms: u64,
    pub consecutive_gaps: u32,
    pub ingestor_alive: bool,
    pub last_status_change: SystemTime,
}

impl MarketFeed {
    pub fn health_check(&self) -> FeedHealth;
}
```

### Benefits
- Standardizes health monitoring across all consumers
- Single source of truth for feed status
- Simplifies production monitoring and alerting
- Enables automatic failover logic

### Difficulty
Low - mostly aggregating existing metrics

---

## FR2: Snapshot Request/Status API (MEDIUM Priority)

### Problem
The SNAPSHOT_PROTOCOL.md documents these APIs but they don't exist in v0.4.1:
- `request_snapshot(&mut self)`
- `snapshot_available(&self) -> bool`
- `snapshot_in_progress(&self) -> bool`

Current workaround: Use `fetch_snapshot()` which blocks, or manually poll for full snapshots in the message stream.

### Proposed Solution
```rust
pub fn request_snapshot(&mut self);

pub fn request_status(&self) -> RequestStatus {
    is_pending: bool,      // True if request sent but not fulfilled
    is_satisfied: bool,    // True if snapshot delivered since last request
    request_count: u32,    // Number of pending requests
}

pub fn clear_request_status(&mut self);
```

### Benefits
- Matches documented API in SNAPSHOT_PROTOCOL.md
- Enables non-blocking snapshot coordination
- Cleaner than polling message stream for full snapshots
- Better separation of concerns (request vs fetch)

### Difficulty
Medium - requires shared state for request flags

---

## FR3: Native Multi-Market Connection Pool (MEDIUM Priority)

### Problem
Current: Each market requires a separate `MarketFeed::connect()` call

```rust
let feed1 = MarketFeed::connect(1_000_001)?; // Lighter market 1
let feed2 = MarketFeed::connect(1_000_002)?; // Lighter market 2
// Each has separate shared memory mappings, separate consumer state
```

For multi-market strategies, this creates overhead:
- Multiple memory mappings
- Multiple consumer position tracking
- Multiple queue depth checks
- Duplicated statistics

### Proposed Solution
```rust
pub struct MultiMarketFeed {
    feeds: HashMap<u64, MarketFeed>,
}

impl MultiMarketFeed {
    pub fn connect(markets: &[(DexType, u64)]) -> Result<Self>;
    pub fn recv_any(&mut self) -> Option<(u64, MarketSnapshot)>; // Which market + snapshot
    pub fn recv_market(&mut self, market_id: u64) -> Option<MarketSnapshot>;
}
```

### Benefits
- Reduced memory overhead for multi-market strategies
- Unified statistics and monitoring
- Simpler round-robin polling of multiple markets
- Potential for kernel optimizations

### Difficulty
Medium - requires refactoring internal consumer management

---

## FR4: Graceful Degradation Mode (LOW Priority)

### Problem
When snapshot fetch fails:
- Current: Consumer is stuck waiting or must implement fallback
- Risk: Missing critical market data

### Proposed Solution
```rust
pub enum DegradationMode {
    Strict,      // Current behavior - fail on gaps
    BestEffort,  // Continue with warning flag on stale data
}

impl MarketFeed {
    pub fn set_degradation_mode(&mut self, mode: DegradationMode);
}
```

Consumers would get snapshots with a new flag:
```rust
pub struct MarketSnapshot {
    // ... existing fields ...
    pub degradation_warnings: u8, // Bit flags for quality issues
}
```

### Benefits
- Allows trading systems to decide their own risk tolerance
- Prevents trading halts during minor connectivity issues
- Better graceful degradation

### Difficulty
Low-Medium - mostly configuration + new flag field

---

## FR5: Metrics Export API (LOW Priority)

### Problem
Each consumer implements its own metrics collection and export:
- Inconsistent metric names
- Different export formats (Prometheus, statsd, stdout)
- Duplicated collection logic

### Proposed Solution
```rust
pub struct MetricsSnapshot {
    pub total_messages: u64,
    pub sequence_gaps: u64,
    pub queue_depth: usize,
    pub max_queue_depth: usize,
    pub epoch: u64,
    pub latency_p50_us: u64,
    pub latency_p99_us: u64,
}

impl MarketFeed {
    pub fn export_metrics(&self) -> MetricsSnapshot;
    pub fn export_prometheus(&self) -> String; // Prometheus text format
}
```

### Benefits
- Standardized monitoring across all consumers
- Native Prometheus integration for ops teams
- Reduces consumer complexity
- Enables benchmarking of huginn performance

### Difficulty
Low - mostly wrapping existing statistics

---

## FR6: Built-in Backpressure Signals (MEDIUM Priority)

### Problem
Consumers must implement backpressure handling:
```rust
// Consumer must do this:
if feed.queue_depth() > 1000 {
    // Apply custom throttling logic
    std::thread::sleep(Duration::from_millis(10));
}
```

### Proposed Solution
```rust
pub enum ReadResult {
    Data(MarketSnapshot),
    Backpressure {
        queue_depth: usize,
        suggested_delay_ms: u64,
    },
    NoData,
}

impl MarketFeed {
    pub fn try_recv_with_backpressure(&mut self) -> ReadResult;
}
```

### Benefits
- Standardized backpressure handling
- Huginn can calculate optimal throttle duration
- Reduces consumer-side complexity
- Better system stability

### Difficulty
Medium - requires understanding consumer capabilities

---

## Implementation Priority

1. **Must Have** (affects production stability):
   - FR1: Health Check API

2. **Should Have** (improves usability):
   - FR2: Snapshot Request/Status API
   - FR3: Multi-Market Connection Pool

3. **Nice to Have** (reduces complexity):
   - FR4: Graceful Degradation Mode
   - FR5: Metrics Export API
   - FR6: Built-in Backpressure Signals

---

## Testing Strategy

Each feature should include:
- Unit tests for new APIs
- Integration tests with existing features
- Performance benchmarks (ensure <50ns overhead)
- Documentation examples

---

## Notes

- All requests assume zero-allocation, lock-free implementation
- Latency targets: <100ns for new methods
- Must maintain backward compatibility with v0.4.1 API
- Consider feature flags for optional features

---

## Contact

Integration work by: bog project
Context: Production HFT trading system, Lighter DEX, sub-microsecond latency
Date: 2025-11-15
