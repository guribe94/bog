//! Benchmark: Order FSM State Transition Performance
//!
//! Purpose: Measure overhead of typestate order lifecycle transitions
//! Target: <10ns per state transition
//!
//! What's Measured:
//! - Pending → Open (acknowledge)
//! - Open → PartiallyFilled (partial fill)
//! - PartiallyFilled → Filled (final fill)
//! - Open → Cancelled (cancellation)
//! - Pending → Rejected (rejection)
//! - Multi-fill stress test (100 partial fills)
//!
//! Why This Matters:
//! Order state transitions happen frequently (every fill). Must be zero-overhead.
//! Typestate pattern should compile to simple data updates with no runtime cost.

use bog_core::core::order_fsm::*;
use bog_core::core::{OrderId, Side};
use criterion::{black_box, criterion_group, criterion_main, Criterion};

/// Benchmark: Pending → Open transition (acknowledge)
fn bench_acknowledge_transition(c: &mut Criterion) {
    let mut group = c.benchmark_group("order_fsm");
    group.significance_level(0.01).sample_size(10000);

    group.bench_function("pending_to_open", |b| {
        b.iter(|| {
            let order = OrderPending::new(
                OrderId::new_random(),
                Side::Buy,
                50_000_000_000_000, // $50,000
                1_000_000_000,      // 1.0 BTC
            );
            black_box(order.acknowledge());
        });
    });

    group.finish();
}

/// Benchmark: Open → Filled (single full fill)
fn bench_open_to_filled(c: &mut Criterion) {
    let mut group = c.benchmark_group("order_fsm");
    group.significance_level(0.01).sample_size(10000);

    group.bench_function("open_to_filled", |b| {
        b.iter(|| {
            let order = OrderPending::new(
                OrderId::new_random(),
                Side::Buy,
                50_000_000_000_000,
                1_000_000_000,
            );
            let order = order.acknowledge();
            black_box(order.fill(1_000_000_000, 50_000_000_000_000));
        });
    });

    group.finish();
}

/// Benchmark: Open → PartiallyFilled (partial fill)
fn bench_open_to_partially_filled(c: &mut Criterion) {
    let mut group = c.benchmark_group("order_fsm");
    group.significance_level(0.01).sample_size(10000);

    group.bench_function("open_to_partial", |b| {
        b.iter(|| {
            let order = OrderPending::new(
                OrderId::new_random(),
                Side::Buy,
                50_000_000_000_000,
                1_000_000_000,
            );
            let order = order.acknowledge();
            black_box(order.fill(500_000_000, 50_000_000_000_000));
        });
    });

    group.finish();
}

/// Benchmark: PartiallyFilled → Filled (complete the order)
fn bench_partial_to_filled(c: &mut Criterion) {
    let mut group = c.benchmark_group("order_fsm");
    group.significance_level(0.01).sample_size(10000);

    group.bench_function("partial_to_filled", |b| {
        b.iter(|| {
            let order = OrderPending::new(
                OrderId::new_random(),
                Side::Buy,
                50_000_000_000_000,
                1_000_000_000,
            );
            let order = order.acknowledge();
            // First partial fill
            let order = match order.fill(500_000_000, 50_000_000_000_000) {
                FillResultOrError::Ok(FillResult::PartiallyFilled(order)) => order,
                _ => panic!("Expected partial fill"),
            };
            // Final fill
            black_box(order.fill(500_000_000, 50_000_000_000_000));
        });
    });

    group.finish();
}

/// Benchmark: Open → Cancelled (cancellation)
fn bench_open_to_cancelled(c: &mut Criterion) {
    let mut group = c.benchmark_group("order_fsm");
    group.significance_level(0.01).sample_size(10000);

    group.bench_function("open_to_cancelled", |b| {
        b.iter(|| {
            let order = OrderPending::new(
                OrderId::new_random(),
                Side::Buy,
                50_000_000_000_000,
                1_000_000_000,
            );
            let order = order.acknowledge();
            black_box(order.cancel());
        });
    });

    group.finish();
}

/// Benchmark: Pending → Rejected (rejection)
fn bench_pending_to_rejected(c: &mut Criterion) {
    let mut group = c.benchmark_group("order_fsm");
    group.significance_level(0.01).sample_size(10000);

    group.bench_function("pending_to_rejected", |b| {
        b.iter(|| {
            let order = OrderPending::new(
                OrderId::new_random(),
                Side::Buy,
                50_000_000_000_000,
                1_000_000_000,
            );
            black_box(order.reject("Insufficient balance".to_string()));
        });
    });

    group.finish();
}

/// Benchmark: 100 partial fills stress test (worst case)
fn bench_100_partial_fills(c: &mut Criterion) {
    let mut group = c.benchmark_group("order_fsm");
    group.significance_level(0.01).sample_size(1000); // Lower sample for heavy test

    group.bench_function("100_partial_fills", |b| {
        b.iter(|| {
            let order = OrderPending::new(
                OrderId::new_random(),
                Side::Buy,
                50_000_000_000_000,
                1_000_000_000, // 1.0 BTC
            );
            let order = order.acknowledge();

            // First fill (Open → PartiallyFilled)
            let mut current = match order.fill(10_000_000, 50_000_000_000_000) {
                FillResultOrError::Ok(FillResult::PartiallyFilled(o)) => o,
                _ => panic!("Expected partial fill"),
            };

            // Apply 98 more partial fills
            for _ in 0..98 {
                current = match current.fill(10_000_000, 50_000_000_000_000) {
                    PartialFillResultOrError::Ok(FillResult::PartiallyFilled(o)) => o,
                    _ => panic!("Expected partial fill"),
                };
            }

            // Final fill (PartiallyFilled → Filled)
            black_box(current.fill(10_000_000, 50_000_000_000_000));
        });
    });

    group.finish();
}

/// Benchmark: Complete order lifecycle (Pending → Open → PartiallyFilled → Filled)
fn bench_complete_lifecycle(c: &mut Criterion) {
    let mut group = c.benchmark_group("order_fsm");
    group.significance_level(0.01).sample_size(10000);

    group.bench_function("complete_lifecycle", |b| {
        b.iter(|| {
            // Create pending order
            let order = OrderPending::new(
                OrderId::new_random(),
                Side::Buy,
                50_000_000_000_000,
                1_000_000_000,
            );

            // Acknowledge
            let order = order.acknowledge();

            // Partial fill 1
            let order = match order.fill(400_000_000, 50_000_000_000_000) {
                FillResultOrError::Ok(FillResult::PartiallyFilled(order)) => order,
                _ => panic!("Expected partial"),
            };

            // Partial fill 2
            let order = match order.fill(300_000_000, 50_000_000_000_000) {
                PartialFillResultOrError::Ok(FillResult::PartiallyFilled(order)) => order,
                _ => panic!("Expected partial"),
            };

            // Final fill
            black_box(order.fill(300_000_000, 50_000_000_000_000));
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_acknowledge_transition,
    bench_open_to_filled,
    bench_open_to_partially_filled,
    bench_partial_to_filled,
    bench_open_to_cancelled,
    bench_pending_to_rejected,
    bench_100_partial_fills,
    bench_complete_lifecycle,
);

criterion_main!(benches);
