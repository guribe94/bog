# Huginn Requirements for Bog Integration

**For:** Huginn Maintainer
**From:** Bog Team
**Date:** 2025-11-11
**Priority:** HIGH (Real Money Trading)
**Status:** Issues Found During Security Audit

---

## EXECUTIVE SUMMARY

During a comprehensive security audit of the Bog market making bot, we identified **CRITICAL initialization issues** with the current Huginn integration that could result in **trading with empty or invalid orderbook data**.

While we've implemented **defensive workarounds** on the Bog side, the root cause is in Huginn's consumer initialization behavior. This document specifies what Huginn needs to provide for safe production trading.

---

## CRITICAL ISSUES FOUND

### Issue #1: ShmConsumer Skips Existing Data on Connect

**Severity:** CRITICAL - BLOCKS PRODUCTION DEPLOYMENT
**File:** `huginn/src/shm/consumer.rs:141-260`

**Current Behavior:**
```rust
pub fn new(market_id: u64) -> Result<Self> {
    // ... open shared memory ...

    // Initialize sequence to current producer position
    let producer_cursor = ring.producer_cursor.load(Ordering::Acquire);
    let last_seq = producer_cursor;  // ⚠️ SKIPS ALL EXISTING DATA

    Ok(Self {
        // ...
        last_seq,  // ⚠️ Consumer starts at HEAD, misses ring buffer contents
    })
}
```

**Problem:**
When a consumer connects, it sets `last_seq = producer_cursor`, which means `try_recv()` will return `None` until the producer writes a NEW message. Any snapshots already in the ring buffer are **skipped**.

**Impact on Bog:**
1. **Cold Start:** If Huginn just started, ring buffer is empty → Bog gets `None` → Exits immediately
2. **Warm Start:** If Huginn has data, but consumer starts at head → Bog gets `None` until next update → Could take seconds
3. **Real Money Risk:** Bog might timeout and exit before seeing data, or start trading milliseconds after connection without proper state

**Required Fix (in Huginn):**

**Option A: Add `peek_latest()` method** (RECOMMENDED)
```rust
impl ShmConsumer {
    /// Get the most recent snapshot without updating consumer cursor
    ///
    /// This is useful for initialization to get current market state.
    /// Returns None only if ring buffer has never been written to.
    pub fn peek_latest(&self) -> Option<MarketSnapshot> {
        let head = self.ring.producer_cursor.load(Ordering::Acquire);

        if head == 0 {
            return None;  // Ring buffer never written
        }

        // Read most recent slot (head - 1) without updating consumer cursor
        let latest_seq = head.wrapping_sub(1);
        let slot_index = (latest_seq & RING_MASK) as usize;

        std::sync::atomic::fence(Ordering::Acquire);

        unsafe {
            let slots = &*self.ring.slots.get();
            Some(std::ptr::read(&slots[slot_index]))
        }
    }
}
```

**Usage in Bog:**
```rust
let mut feed = MarketFeed::connect(1)?;

// Get current snapshot immediately (if available)
if let Some(current) = feed.peek_latest() {
    if is_valid_snapshot(&current) {
        // Start with current market state
        process_tick(&current)?;
    }
}

// Then enter normal loop
loop {
    if let Some(snapshot) = feed.try_recv() {
        process_tick(&snapshot)?;
    }
}
```

**Option B: Provide `get_current_snapshot()` method** (ALTERNATIVE)
```rust
impl ShmConsumer {
    /// Get current snapshot, blocking until available
    ///
    /// Returns immediately if ring buffer has data, otherwise waits.
    pub fn get_current_snapshot(&mut self, timeout: Duration) -> Result<MarketSnapshot> {
        // Try peek_latest() first
        if let Some(snapshot) = self.peek_latest() {
            return Ok(snapshot);
        }

        // Ring buffer empty, wait for first message
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if let Some(snapshot) = self.try_recv() {
                return Ok(snapshot);
            }
            std::thread::sleep(Duration::from_millis(10));
        }

        Err(anyhow!("Timeout waiting for first snapshot"))
    }
}
```

---

### Issue #2: No Way to Check Ring Buffer Status

**Severity:** HIGH
**File:** `huginn/src/shm/consumer.rs`

**Current Behavior:**
`try_recv()` returns `None` for two different conditions:
1. Ring buffer has never been written to (empty)
2. Consumer has caught up with producer (no new data)

**Problem:**
Bog cannot distinguish between "Huginn hasn't started yet" vs "Caught up, waiting for next tick". This makes proper initialization logic impossible.

**Required Fix:**

Add status method:
```rust
impl ShmConsumer {
    /// Get ring buffer status
    pub fn buffer_status(&self) -> BufferStatus {
        let head = self.ring.producer_cursor.load(Ordering::Acquire);
        let tail = self.last_seq;

        if head == 0 {
            BufferStatus::NeverWritten
        } else if tail >= head {
            BufferStatus::CaughtUp
        } else {
            BufferStatus::HasData { pending: head - tail }
        }
    }
}

pub enum BufferStatus {
    /// Ring buffer has never been written to (Huginn not receiving data)
    NeverWritten,
    /// Consumer has caught up with producer (real-time)
    CaughtUp,
    /// Data available to read
    HasData { pending: u64 },
}
```

**Usage in Bog:**
```rust
match feed.buffer_status() {
    BufferStatus::NeverWritten => {
        // Huginn not receiving data yet, keep waiting
    }
    BufferStatus::CaughtUp => {
        // Normal operation, wait for next update
    }
    BufferStatus::HasData { pending } => {
        // Read available data
    }
}
```

---

### Issue #3: Missing Initialization Documentation

**Severity:** MEDIUM
**File:** `huginn/docs/` (missing or incomplete)

**Problem:**
No documentation exists explaining:
- What happens when consumer connects to empty ring buffer
- Whether ring buffer is pre-populated on Huginn startup
- How long to wait before expecting data
- What to do if no data appears

**Required Fix:**

Add section to Huginn documentation:

**File:** `huginn/docs/core/INITIALIZATION.md` (NEW)

```markdown
# Consumer Initialization Guide

## Ring Buffer State on Connect

When a consumer connects via `ShmConsumer::new()`:

1. **If Huginn just started:**
   - Ring buffer exists but is empty (all zeros)
   - `producer_cursor = 0`
   - Consumer `try_recv()` returns `None`
   - Consumer should wait for first message from Lighter

2. **If Huginn has been running:**
   - Ring buffer contains recent snapshots
   - `producer_cursor = N` (number of messages published)
   - Consumer starts at cursor N (**skips existing data**)
   - Consumer `try_recv()` returns `None` until next message

## Recommended Initialization Pattern

```rust
let mut consumer = ShmConsumer::new(market_id)?;

// Option 1: Use peek_latest() to get current state
if let Some(current) = consumer.peek_latest() {
    println!("Current market state: {:?}", current);
}

// Option 2: Wait for first message with timeout
let timeout = Duration::from_secs(10);
let deadline = Instant::now() + timeout;

loop {
    if let Some(snapshot) = consumer.try_recv() {
        println!("First snapshot received!");
        break;
    }

    if Instant::now() > deadline {
        return Err("Timeout waiting for first snapshot");
    }

    thread::sleep(Duration::from_millis(100));
}
```

## Time to First Message

**Expected:** 100-500ms after Huginn connects to exchange
**Maximum:** 5 seconds (if Lighter WebSocket is slow)

If no message after 10 seconds:
- Check Huginn logs for connection errors
- Verify Lighter exchange is online
- Check network connectivity
```

---

### Issue #4: No is_ready() Method

**Severity:** MEDIUM
**File:** `huginn/src/shm/consumer.rs`

**Problem:**
No way to check if Huginn is actively receiving data from the exchange.

**Required Fix:**

```rust
impl ShmConsumer {
    /// Check if Huginn is receiving data from exchange
    ///
    /// Returns true if:
    /// - Ring buffer has been written to at least once
    /// - Last message was received within threshold
    ///
    /// Returns false if:
    /// - Ring buffer never populated (Huginn not connected)
    /// - Last message too old (connection stale)
    pub fn is_ready(&self, max_age: Duration) -> bool {
        let head = self.ring.producer_cursor.load(Ordering::Acquire);

        if head == 0 {
            return false;  // Never written
        }

        // Get latest snapshot to check timestamp
        if let Some(latest) = self.peek_latest() {
            let now_ns = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64;

            let age_ns = now_ns.saturating_sub(latest.exchange_timestamp_ns);
            age_ns < max_age.as_nanos() as u64
        } else {
            false
        }
    }
}
```

---

### Issue #5: Sequence Number Confusion

**Severity:** LOW (Cosmetic)
**File:** `huginn/src/shm/consumer.rs`

**Problem:**
Consumer sequence (`last_seq`) and snapshot sequence (`snapshot.sequence`) are different:
- `last_seq`: Ring buffer cursor (wraps at RING_SIZE)
- `snapshot.sequence`: Monotonic message counter from Lighter

This can be confusing when debugging.

**Recommendation:**
Add documentation clarifying the difference, or provide both in stats:
```rust
pub struct ConsumerStats {
    pub ring_cursor: u64,        // Position in ring buffer
    pub last_message_seq: u64,   // Last snapshot.sequence seen
    // ...
}
```

---

## MUST-HAVE FEATURES (For Production)

### 1. peek_latest() Method ⭐⭐⭐

**Priority:** CRITICAL
**Effort:** 30 minutes

Returns most recent snapshot without consuming it. Essential for initialization.

**Signature:**
```rust
pub fn peek_latest(&self) -> Option<MarketSnapshot>
```

**Implementation:** See Issue #1 above

---

### 2. buffer_status() Method ⭐⭐⭐

**Priority:** CRITICAL
**Effort:** 20 minutes

Distinguishes "never written" from "caught up".

**Signature:**
```rust
pub fn buffer_status(&self) -> BufferStatus

pub enum BufferStatus {
    NeverWritten,
    CaughtUp,
    HasData { pending: u64 },
}
```

**Implementation:** See Issue #2 above

---

### 3. Initialization Documentation ⭐⭐

**Priority:** HIGH
**Effort:** 1 hour

Document expected behavior, timings, and recommended patterns.

**File:** `huginn/docs/core/INITIALIZATION.md`

**Contents:** See Issue #3 above

---

## NICE-TO-HAVE FEATURES

### 4. is_ready() Method ⭐

**Priority:** MEDIUM
**Effort:** 15 minutes

Health check that Huginn is receiving data.

**Signature:**
```rust
pub fn is_ready(&self, max_age: Duration) -> bool
```

---

### 5. get_current_snapshot() Blocking Method ⭐

**Priority:** MEDIUM
**Effort:** 30 minutes

Blocking call that returns current state or waits for first message.

**Signature:**
```rust
pub fn get_current_snapshot(&mut self, timeout: Duration) -> Result<MarketSnapshot>
```

---

## CURRENT BOG WORKAROUNDS

To mitigate these Huginn issues, we've implemented the following in Bog:

### 1. Engine Initialization Guard ✅

**File:** `bog-core/src/engine/generic.rs:340-415`

The engine now:
- Waits up to 10 seconds for first valid snapshot
- Validates every snapshot (non-zero, not crossed, reasonable spread)
- Logs clearly at each step
- Exits with helpful error message if timeout

**Code:**
```rust
// Wait for first valid snapshot (max 100 retries × 100ms = 10s)
for attempt in 0..100 {
    match feed_fn()? {
        Some(snapshot) if is_valid_snapshot(&snapshot) => {
            process_tick(&snapshot)?;  // Initialize orderbook
            break;  // Ready to trade
        }
        Some(snapshot) => {
            warn!("Invalid snapshot, waiting...");
        }
        None => {
            // Ring buffer empty, keep waiting
        }
    }
    thread::sleep(100ms);
}
```

### 2. Snapshot Validation Functions ✅

**File:** `bog-core/src/data/mod.rs:67-139`

Added comprehensive validation:
- `validate_snapshot()` - Returns detailed error
- `is_valid_snapshot()` - Bool wrapper
- `is_crossed()` - Check for crossed orderbook
- `is_locked()` - Check for locked orderbook
- `is_stale()` - Check timestamp age

**All snapshots are now validated before use!**

### 3. Defense in Depth ✅

**Validation at 3 layers:**
1. **Engine level** - Skips invalid snapshots
2. **Strategy level** - Already has validation (simple_spread.rs:291-312)
3. **Circuit breaker** - Halts on extreme conditions

**This prevents financial loss even if Huginn sends bad data.**

---

## TESTING REQUIREMENTS

### What Huginn Should Test

**Scenario 1: Cold Start**
```rust
#[test]
fn test_consumer_cold_start() {
    // 1. Create new shared memory (never written)
    let ring = create_ring_buffer();
    let consumer = ShmConsumer::new(1)?;

    // peek_latest() should return None
    assert!(consumer.peek_latest().is_none());

    // buffer_status() should be NeverWritten
    assert_eq!(consumer.buffer_status(), BufferStatus::NeverWritten);

    // try_recv() should return None
    assert!(consumer.try_recv().is_none());
}
```

**Scenario 2: Warm Start**
```rust
#[test]
fn test_consumer_warm_start() {
    // 1. Producer writes 10 snapshots
    let ring = create_ring_buffer();
    publish_snapshot(&ring, create_snapshot(1));
    publish_snapshot(&ring, create_snapshot(2));
    // ... up to sequence 10

    // 2. Consumer connects
    let mut consumer = ShmConsumer::new(1)?;

    // peek_latest() should return snapshot #10
    let latest = consumer.peek_latest().unwrap();
    assert_eq!(latest.sequence, 10);

    // buffer_status() should be CaughtUp (since we start at head)
    assert_eq!(consumer.buffer_status(), BufferStatus::CaughtUp);

    // try_recv() returns None (caught up)
    assert!(consumer.try_recv().is_none());

    // After producer writes #11, try_recv() returns it
    publish_snapshot(&ring, create_snapshot(11));
    assert_eq!(consumer.try_recv().unwrap().sequence, 11);
}
```

**Scenario 3: Catchup**
```rust
#[test]
fn test_consumer_catchup() {
    // 1. Producer is 1000 messages ahead
    // 2. Consumer connects and calls peek_latest()
    // 3. Should get most recent snapshot immediately
    // 4. try_recv() should drain ring buffer
}
```

---

## API SPECIFICATION

### Required Methods (Priority 1)

#### `peek_latest() -> Option<MarketSnapshot>`

**Purpose:** Get most recent snapshot without consuming
**Returns:**
- `Some(snapshot)` - Most recent snapshot in ring buffer
- `None` - Ring buffer never written to
**Side Effects:** None (doesn't update cursor)
**Thread Safety:** Safe (read-only operation)

---

#### `buffer_status() -> BufferStatus`

**Purpose:** Check ring buffer state
**Returns:** `NeverWritten | CaughtUp | HasData { pending: u64 }`
**Side Effects:** None
**Thread Safety:** Safe

---

### Nice-to-Have Methods (Priority 2)

#### `is_ready(max_age: Duration) -> bool`

**Purpose:** Health check for Huginn connection
**Returns:** `true` if receiving recent data, `false` otherwise

---

#### `get_current_snapshot(timeout: Duration) -> Result<MarketSnapshot>`

**Purpose:** Blocking call that waits for first snapshot
**Returns:** First valid snapshot or timeout error

---

## DOCUMENTATION REQUIREMENTS

### Must Document

1. **Initialization Behavior**
   - What happens on `ShmConsumer::new()`
   - Where consumer cursor starts
   - Whether existing ring data is accessible

2. **Ring Buffer Lifecycle**
   - When is ring buffer created?
   - How long does data persist?
   - What happens on Huginn restart?

3. **Consumer Patterns**
   - How to get current snapshot on connect
   - How to wait for first data
   - How to detect "Huginn not running"

4. **Timing Guarantees**
   - Expected time to first message
   - Maximum staleness before concern
   - Reconnection behavior

---

## BACKWARD COMPATIBILITY

All new methods (`peek_latest()`, `buffer_status()`, etc.) are **additive** and don't break existing consumers. Current bog code works but with the workarounds described above.

**Migration Path:**
1. Huginn adds new methods
2. Bog removes workarounds and uses new API
3. Cleaner, more efficient code

---

## EXAMPLES

### Consumer Initialization (Recommended Pattern)

```rust
use huginn::shm::{ShmConsumer, BufferStatus};

pub fn connect_safely(market_id: u64) -> Result<ShmConsumer> {
    let mut consumer = ShmConsumer::new(market_id)?;

    // Check if Huginn is ready
    match consumer.buffer_status() {
        BufferStatus::NeverWritten => {
            println!("⏳ Huginn not receiving data yet, waiting...");

            // Wait up to 10 seconds for first message
            for _ in 0..100 {
                if let Some(snapshot) = consumer.peek_latest() {
                    println!("✅ Huginn is now receiving data");
                    return Ok(consumer);
                }
                thread::sleep(Duration::from_millis(100));
            }

            return Err(anyhow!("Huginn never received data from exchange"));
        }
        BufferStatus::CaughtUp => {
            println!("✅ Huginn is running and caught up");
        }
        BufferStatus::HasData { pending } => {
            println!("✅ Huginn has {} pending messages", pending);
        }
    }

    Ok(consumer)
}
```

---

## IMPACT ASSESSMENT

### If Not Fixed

**Risk:** HIGH
- Bog might trade with invalid/empty orderbook
- Potential immediate financial loss
- Production deployment blocked

**Workaround:** Bog has defensive code but it's not ideal

### If Fixed

**Benefits:**
- Clean initialization code in Bog
- Faster startup (no 10s wait)
- Better error messages
- Safer production deployment
- Other Huginn consumers benefit too

**Effort:** ~2-3 hours of development + 1 hour testing

---

## TESTING CHECKLIST FOR HUGINN

After implementing fixes, verify:

- [ ] `peek_latest()` returns None when ring never written
- [ ] `peek_latest()` returns most recent snapshot when ring has data
- [ ] `buffer_status()` correctly identifies all 3 states
- [ ] Consumer can initialize even if producer ahead by 1000 messages
- [ ] No race conditions when producer writes during peek
- [ ] Memory barriers correct (Acquire/Release semantics)
- [ ] Works with multiple concurrent consumers
- [ ] Documentation updated with initialization patterns

---

## CONTACT

**For Questions:** Bog Team
**Timeline:** Needed within 2-3 weeks for production deployment
**Priority:** HIGH (blocking real money trading)

---

## APPENDIX: BOG'S CURRENT WORKAROUND CODE

### Engine Initialization (bog-core/src/engine/generic.rs:340-415)

```rust
// Wait for first valid snapshot (up to 10 seconds)
for attempt in 0..100 {
    match feed_fn()? {
        Some(snapshot) if is_valid_snapshot(&snapshot) => {
            // Got valid data, start trading
            process_tick(&snapshot)?;
            break;
        }
        Some(snapshot) => {
            // Invalid snapshot (zeros, crossed, etc.)
            warn!("Invalid snapshot received, waiting...");
        }
        None => {
            // Ring buffer empty or caught up
            // Can't distinguish which! (Issue #2)
        }
    }
    thread::sleep(100ms);
}
```

**This works but:**
- Wastes 10 seconds on cold start
- Can't distinguish root cause of None
- Relies on polling instead of direct state query

### Snapshot Validation (bog-core/src/data/mod.rs:67-105)

```rust
pub fn validate_snapshot(snapshot: &MarketSnapshot) -> Result<()> {
    if snapshot.best_bid_price == 0 { error!() }
    if snapshot.best_ask_price == 0 { error!() }
    if snapshot.best_bid_price >= snapshot.best_ask_price { error!() }
    // ... more checks
}
```

**This protects Bog but doesn't solve root cause.**

---

**END OF REQUIREMENTS**

Thank you for maintaining Huginn! These additions will make it safer for all production consumers.
