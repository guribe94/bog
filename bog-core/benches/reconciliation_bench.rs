//! Benchmark: Position Reconciliation Performance
//!
//! Purpose: Measure overhead of position reconciliation checks
//! Target: <100ns per reconciliation
//!
//! What's Measured:
//! - Single position reconciliation (no drift)
//! - Reconciliation with small drift (<0.001 BTC)
//! - Reconciliation with large drift (>0.01 BTC)
//! - Batch reconciliation (100 positions)
//! - Drift tracking overhead
//!
//! Why This Matters:
//! Position reconciliation ensures accuracy between internal tracking
//! and executor state. Must be fast enough to run frequently without
//! impacting tick-to-trade latency.

use bog_core::engine::position_reconciliation::{PositionReconciler, ReconciliationConfig};
use criterion::{black_box, criterion_group, criterion_main, Criterion};

/// Benchmark: Single reconciliation with no drift
fn bench_reconcile_no_drift(c: &mut Criterion) {
    let mut group = c.benchmark_group("reconciliation");
    group.significance_level(0.01).sample_size(10000);

    let reconciler = PositionReconciler::new();
    let internal_pos = 1_000_000_000i64; // 1.0 BTC
    let executor_pos = 1_000_000_000i64; // 1.0 BTC (same - no drift)

    group.bench_function("no_drift", |b| {
        b.iter(|| {
            black_box(
                reconciler
                    .reconcile(black_box(internal_pos), black_box(executor_pos))
                    .unwrap(),
            );
        });
    });

    group.finish();
}

/// Benchmark: Reconciliation with small drift
fn bench_reconcile_small_drift(c: &mut Criterion) {
    let mut group = c.benchmark_group("reconciliation");
    group.significance_level(0.01).sample_size(10000);

    let reconciler = PositionReconciler::new();
    let internal_pos = 1_000_000_000i64; // 1.0 BTC
    let executor_pos = 1_000_500_000i64; // 1.0005 BTC (0.0005 BTC drift)

    group.bench_function("small_drift", |b| {
        b.iter(|| {
            black_box(
                reconciler
                    .reconcile(black_box(internal_pos), black_box(executor_pos))
                    .unwrap(),
            );
        });
    });

    group.finish();
}

/// Benchmark: Reconciliation with large drift (error condition)
fn bench_reconcile_large_drift(c: &mut Criterion) {
    let mut group = c.benchmark_group("reconciliation");
    group.significance_level(0.01).sample_size(10000);

    let reconciler = PositionReconciler::new();
    let internal_pos = 1_000_000_000i64; // 1.0 BTC
    let executor_pos = 1_050_000_000i64; // 1.05 BTC (0.05 BTC drift - large!)

    group.bench_function("large_drift", |b| {
        b.iter(|| {
            // This should fail due to excessive drift
            let result = reconciler.reconcile(black_box(internal_pos), black_box(executor_pos));
            black_box(result);
        });
    });

    group.finish();
}

/// Benchmark: Reconciliation with varying positions
fn bench_reconcile_varying_positions(c: &mut Criterion) {
    let mut group = c.benchmark_group("reconciliation");
    group.significance_level(0.01).sample_size(1000);

    let reconciler = PositionReconciler::new();

    group.bench_function("varying_positions", |b| {
        let mut i = 0u64;
        b.iter(|| {
            // Vary position size
            let internal_pos = ((i % 1000) as i64) * 1_000_000; // 0 to 0.001 BTC
            let executor_pos = internal_pos;
            i += 1;
            black_box(reconciler.reconcile(internal_pos, executor_pos).unwrap());
        });
    });

    group.finish();
}

/// Benchmark: Reconciliation with custom config
fn bench_reconcile_custom_config(c: &mut Criterion) {
    let mut group = c.benchmark_group("reconciliation");
    group.significance_level(0.01).sample_size(10000);

    let config = ReconciliationConfig {
        reconcile_every_n_fills: 500,
        max_position_mismatch: 500_000, // 0.0005 BTC
        halt_on_mismatch: false,
        auto_correct_threshold: 50_000, // 0.00005 BTC
    };
    let reconciler = PositionReconciler::with_config(config);
    let internal_pos = 1_000_000_000i64;
    let executor_pos = 1_000_000_000i64;

    group.bench_function("custom_config", |b| {
        b.iter(|| {
            black_box(
                reconciler
                    .reconcile(black_box(internal_pos), black_box(executor_pos))
                    .unwrap(),
            );
        });
    });

    group.finish();
}

/// Benchmark: Fill counter increment overhead
fn bench_on_fill_overhead(c: &mut Criterion) {
    let mut group = c.benchmark_group("reconciliation");
    group.significance_level(0.01).sample_size(10000);

    let reconciler = PositionReconciler::new();

    group.bench_function("on_fill_increment", |b| {
        b.iter(|| {
            black_box(reconciler.on_fill());
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_reconcile_no_drift,
    bench_reconcile_small_drift,
    bench_reconcile_large_drift,
    bench_reconcile_varying_positions,
    bench_reconcile_custom_config,
    bench_on_fill_overhead,
);

criterion_main!(benches);
