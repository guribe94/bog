//! Benchmark: Circuit Breaker Performance
//!
//! Purpose: Measure overhead of risk circuit breaker checks and state transitions
//! Target: <5ns check overhead (normal operation), <20ns state transition
//!
//! What's Measured:
//! - CircuitBreaker.check() in normal state (hot path)
//! - Trip detection (spread check, price spike)
//! - State transitions (Normal→Halted, Halted→Normal via reset)
//! - Binary breaker FSM operations
//! - Three-state breaker (connection resilience)
//!
//! Why This Matters:
//! Circuit breaker check runs on EVERY market tick. Must have near-zero overhead
//! in normal operation. Only trips in extreme conditions (flash crash).

use bog_core::data::MarketSnapshot;
use bog_core::risk::circuit_breaker::CircuitBreaker;
use criterion::{black_box, criterion_group, criterion_main, Criterion};

/// Helper: Create normal market snapshot
fn create_normal_snapshot() -> MarketSnapshot {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;

    MarketSnapshot {
        market_id: 1,
        sequence: 100,
        exchange_timestamp_ns: now,
        local_recv_ns: now,
        local_publish_ns: now,
        best_bid_price: 50_000_000_000_000,   // $50,000
        best_bid_size: 1_000_000_000,          // 1.0 BTC
        best_ask_price: 50_005_000_000_000,   // $50,005 (1bps spread)
        best_ask_size: 1_000_000_000,          // 1.0 BTC
        bid_prices: [0; 10],
        bid_sizes: [0; 10],
        ask_prices: [0; 10],
        ask_sizes: [0; 10],
        snapshot_flags: 0,
        dex_type: 1,
        _padding: [0; 54],
    }
}

/// Helper: Create snapshot with excessive spread (flash crash)
fn create_wide_spread_snapshot() -> MarketSnapshot {
    let mut snapshot = create_normal_snapshot();
    snapshot.best_ask_price = 50_600_000_000_000; // $50,600 (120bps spread > 100bps limit)
    snapshot
}

/// Helper: Create snapshot with price spike
fn create_price_spike_snapshot() -> MarketSnapshot {
    let mut snapshot = create_normal_snapshot();
    snapshot.best_bid_price = 55_000_000_000_000; // $55,000 (+10% spike)
    snapshot.best_ask_price = 55_005_000_000_000; // $55,005
    snapshot
}

/// Helper: Create snapshot with low liquidity
fn create_low_liquidity_snapshot() -> MarketSnapshot {
    let mut snapshot = create_normal_snapshot();
    snapshot.best_bid_size = 1_000_000; // 0.001 BTC (below 0.01 threshold)
    snapshot.best_ask_size = 1_000_000;
    snapshot
}

/// Benchmark: Normal operation check (hot path - no violations)
fn bench_check_normal_operation(c: &mut Criterion) {
    let mut group = c.benchmark_group("circuit_breaker");
    group.significance_level(0.01).sample_size(10000);

    let mut breaker = CircuitBreaker::new();
    let snapshot = create_normal_snapshot();

    group.bench_function("check_normal", |b| {
        b.iter(|| {
            black_box(breaker.check(black_box(&snapshot)));
        });
    });

    group.finish();
}

/// Benchmark: Spread detection (should detect and trip)
fn bench_spread_detection(c: &mut Criterion) {
    let mut group = c.benchmark_group("circuit_breaker");
    group.significance_level(0.01).sample_size(10000);

    let snapshot = create_wide_spread_snapshot();

    group.bench_function("spread_detection", |b| {
        b.iter(|| {
            // Create fresh breaker each iteration (since it trips)
            let mut breaker = CircuitBreaker::new();
            black_box(breaker.check(black_box(&snapshot)));
        });
    });

    group.finish();
}

/// Benchmark: Price spike detection
fn bench_price_spike_detection(c: &mut Criterion) {
    let mut group = c.benchmark_group("circuit_breaker");
    group.significance_level(0.01).sample_size(10000);

    let normal_snapshot = create_normal_snapshot();
    let spike_snapshot = create_price_spike_snapshot();

    group.bench_function("price_spike_detection", |b| {
        b.iter(|| {
            let mut breaker = CircuitBreaker::new();
            // Prime with normal snapshot
            breaker.check(&normal_snapshot);
            // Then check spike
            black_box(breaker.check(black_box(&spike_snapshot)));
        });
    });

    group.finish();
}

/// Benchmark: Low liquidity detection
fn bench_low_liquidity_check(c: &mut Criterion) {
    let mut group = c.benchmark_group("circuit_breaker");
    group.significance_level(0.01).sample_size(10000);

    let mut breaker = CircuitBreaker::new();
    let snapshot = create_low_liquidity_snapshot();

    group.bench_function("low_liquidity", |b| {
        b.iter(|| {
            black_box(breaker.check(black_box(&snapshot)));
        });
    });

    group.finish();
}

/// Benchmark: Reset operation (Halted→Normal transition)
fn bench_reset_from_halted(c: &mut Criterion) {
    let mut group = c.benchmark_group("circuit_breaker");
    group.significance_level(0.01).sample_size(10000);

    let wide_snapshot = create_wide_spread_snapshot();
    let normal_snapshot = create_normal_snapshot();

    group.bench_function("reset_transition", |b| {
        b.iter(|| {
            let mut breaker = CircuitBreaker::new();
            // Trip it
            breaker.check(&wide_snapshot);
            // Reset
            black_box(breaker.reset());
            // Verify normal
            black_box(breaker.check(&normal_snapshot));
        });
    });

    group.finish();
}

/// Benchmark: Consecutive violations counting
fn bench_consecutive_violations(c: &mut Criterion) {
    let mut group = c.benchmark_group("circuit_breaker");
    group.significance_level(0.01).sample_size(1000); // Lower sample since this is heavier

    let wide_snapshot = create_wide_spread_snapshot();
    let normal_snapshot = create_normal_snapshot();

    group.bench_function("consecutive_violations", |b| {
        b.iter(|| {
            let mut breaker = CircuitBreaker::new();
            // Send 2 violations (below 3 threshold - shouldn't halt yet)
            breaker.check(&wide_snapshot);
            breaker.check(&normal_snapshot); // Resets counter
            breaker.check(&wide_snapshot);
            breaker.check(&normal_snapshot); // Resets counter
            // Then 3 consecutive (should halt)
            breaker.check(&wide_snapshot);
            breaker.check(&wide_snapshot);
            black_box(breaker.check(&wide_snapshot)); // 3rd consecutive - trips
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_check_normal_operation,
    bench_spread_detection,
    bench_price_spike_detection,
    bench_low_liquidity_check,
    bench_reset_from_halted,
    bench_consecutive_violations,
);

criterion_main!(benches);
