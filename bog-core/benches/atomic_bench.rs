//! Atomic Operations Benchmarks
//!
//! Measures performance of atomic operations used in position tracking and order management.
//! Critical for understanding contention and memory ordering overhead.
//!
//! ## Operations Tested
//!
//! 1. **Position Updates** - Atomic i64 operations with different memory orderings
//! 2. **OrderId Generation** - Thread-local state + RNG
//! 3. **Checked vs Unchecked** - Overhead of overflow protection
//! 4. **Contention Scenarios** - Multi-threaded performance

use bog_core::core::{OrderId, Position};
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::sync::Arc;
use std::thread;

fn position_reads_bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("atomic/position_reads");
    group.significance_level(0.01).sample_size(10000);

    let position = Position::new();

    group.bench_function("get_quantity", |b| {
        b.iter(|| black_box(position.get_quantity()));
    });

    group.bench_function("get_realized_pnl", |b| {
        b.iter(|| black_box(position.get_realized_pnl()));
    });

    group.bench_function("get_daily_pnl", |b| {
        b.iter(|| black_box(position.get_daily_pnl()));
    });

    group.bench_function("get_trade_count", |b| {
        b.iter(|| black_box(position.get_trade_count()));
    });

    group.finish();
}

fn position_updates_unchecked_bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("atomic/position_updates_unchecked");
    group.significance_level(0.01).sample_size(10000);

    let position = Position::new();

    group.bench_function("update_quantity_small", |b| {
        b.iter(|| black_box(position.update_quantity(100_000_000))); // 0.1 BTC
    });

    group.bench_function("update_realized_pnl_small", |b| {
        b.iter(|| position.update_realized_pnl(black_box(1_000_000_000))); // $1
    });

    group.bench_function("update_daily_pnl_small", |b| {
        b.iter(|| position.update_daily_pnl(black_box(1_000_000_000)));
    });

    group.bench_function("increment_trades", |b| {
        b.iter(|| black_box(position.increment_trades()));
    });

    group.finish();
}

fn position_updates_checked_bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("atomic/position_updates_checked");
    group.significance_level(0.01).sample_size(10000);

    let position = Position::new();

    group.bench_function("update_quantity_checked_small", |b| {
        b.iter(|| {
            // Ignore result for benchmark (we're measuring overhead, not error handling)
            let _ = position.update_quantity_checked(black_box(100_000_000));
        });
    });

    group.bench_function("update_realized_pnl_checked_small", |b| {
        b.iter(|| {
            let _ = position.update_realized_pnl_checked(black_box(1_000_000_000));
        });
    });

    group.bench_function("update_daily_pnl_checked_small", |b| {
        b.iter(|| {
            let _ = position.update_daily_pnl_checked(black_box(1_000_000_000));
        });
    });

    group.finish();
}

fn orderid_generation_bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("atomic/orderid_generation");
    group.significance_level(0.01).sample_size(10000);

    group.bench_function("generate", |b| {
        b.iter(|| black_box(OrderId::generate()));
    });

    group.bench_function("new_random", |b| {
        b.iter(|| black_box(OrderId::new_random()));
    });

    // Test throughput
    group.bench_function("batch_100", |b| {
        b.iter(|| {
            for _ in 0..100 {
                black_box(OrderId::generate());
            }
        });
    });

    group.finish();
}

fn position_contention_bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("atomic/position_contention");
    group.significance_level(0.01).sample_size(1000); // Fewer samples for threaded tests

    let position = Arc::new(Position::new());

    // Single thread baseline
    group.bench_function("single_thread", |b| {
        let pos = position.clone();
        b.iter(|| {
            pos.update_quantity(black_box(100_000_000));
        });
    });

    // 2 threads contending
    group.bench_function("2_threads", |b| {
        b.iter(|| {
            let pos1 = position.clone();
            let pos2 = position.clone();

            let h1 = thread::spawn(move || {
                for _ in 0..100 {
                    pos1.update_quantity(1_000_000);
                }
            });

            let h2 = thread::spawn(move || {
                for _ in 0..100 {
                    pos2.update_quantity(-1_000_000);
                }
            });

            h1.join().unwrap();
            h2.join().unwrap();
        });
    });

    // 4 threads contending
    group.bench_function("4_threads", |b| {
        b.iter(|| {
            let handles: Vec<_> = (0..4)
                .map(|i| {
                    let pos = position.clone();
                    thread::spawn(move || {
                        let delta = if i % 2 == 0 { 1_000_000 } else { -1_000_000 };
                        for _ in 0..100 {
                            pos.update_quantity(delta);
                        }
                    })
                })
                .collect();

            for h in handles {
                h.join().unwrap();
            }
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    position_reads_bench,
    position_updates_unchecked_bench,
    position_updates_checked_bench,
    orderid_generation_bench,
    position_contention_bench
);
criterion_main!(benches);
