//! Benchmark: System Throughput Limits
//!
//! Purpose: Measure maximum sustained throughput for various operations
//! Target: >10,000 ticks/second sustained
//!
//! What's Measured:
//! - Maximum ticks/second (sustained rate)
//! - Maximum orders/second
//! - Maximum fills/second
//! - Queue saturation points
//! - System stability under load
//!
//! Why This Matters:
//! Understanding maximum throughput helps with capacity planning and
//! identifying bottlenecks before production deployment.

use bog_core::core::{Position, Signal};
use bog_core::data::MarketSnapshot;
use bog_core::engine::{Engine, Executor, SimulatedExecutor, Strategy};
use bog_core::execution::{Fill, OrderId, Side};
use bog_strategies::SimpleSpread;
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use rust_decimal::Decimal;
use std::time::{Duration, Instant};

/// Helper: Create market snapshot with sequence number
fn create_snapshot(sequence: u64) -> MarketSnapshot {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;

    MarketSnapshot {
        market_id: 1,
        sequence,
        exchange_timestamp_ns: now,
        local_recv_ns: now,
        local_publish_ns: now,
        best_bid_price: 50_000_000_000_000 + (sequence % 100) * 1_000_000_000,
        best_bid_size: 1_000_000_000,
        best_ask_price: 50_005_000_000_000 + (sequence % 100) * 1_000_000_000,
        best_ask_size: 1_000_000_000,
        bid_prices: [0; 10],
        bid_sizes: [0; 10],
        ask_prices: [0; 10],
        ask_sizes: [0; 10],
        snapshot_flags: 0,
        dex_type: 1,
        _padding: [0; 54],
    }
}

/// Benchmark: Sustained tick processing rate
fn bench_sustained_tick_rate(c: &mut Criterion) {
    let mut group = c.benchmark_group("throughput");
    group.significance_level(0.01).sample_size(100);

    for count in [1000, 5000, 10000].iter() {
        group.bench_with_input(
            BenchmarkId::new("sustained_ticks", count),
            count,
            |b, &count| {
                b.iter(|| {
                    let strategy = SimpleSpread;
                    let executor = SimulatedExecutor::new_default();
                    let mut engine = Engine::new(strategy, executor);

                    let start = Instant::now();
                    for seq in 0..count {
                        let snapshot = create_snapshot(seq);
                        engine.process_tick(&snapshot, true).unwrap();
                    }
                    let elapsed = start.elapsed();

                    black_box(elapsed);
                });
            },
        );
    }

    group.finish();
}

/// Benchmark: Maximum orders per second
fn bench_max_orders_per_second(c: &mut Criterion) {
    let mut group = c.benchmark_group("throughput");
    group.significance_level(0.01).sample_size(100);

    let mut executor = SimulatedExecutor::new_default();
    let position = Position::new();
    let signal = Signal::quote_both(49_995_000_000_000, 50_005_000_000_000, 100_000_000);

    group.bench_function("1000_orders", |b| {
        b.iter(|| {
            for _ in 0..1000 {
                executor.execute(signal, &position).unwrap();
                // Drain fills periodically to avoid queue overflow
                if let Ok(_) = executor.execute(signal, &position) {
                    let _ = executor.get_fills();
                }
            }
        });
    });

    group.finish();
}

/// Benchmark: Maximum fills per second
fn bench_max_fills_per_second(c: &mut Criterion) {
    let mut group = c.benchmark_group("throughput");
    group.significance_level(0.01).sample_size(100);

    let position = Position::new();

    group.bench_function("1000_fills_processing", |b| {
        b.iter(|| {
            // Simulate processing 1000 fills
            for i in 0..1000 {
                let quantity_i64 = if i % 2 == 0 {
                    100_000_000
                } else {
                    -100_000_000
                };
                black_box(position.update_quantity(quantity_i64));
            }
        });
    });

    group.finish();
}

/// Benchmark: Engine throughput with realistic fill rate
fn bench_realistic_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("throughput");
    group.significance_level(0.01).sample_size(50);

    group.bench_function("10000_ticks_realistic", |b| {
        b.iter(|| {
            let strategy = SimpleSpread;
            let executor = SimulatedExecutor::new_default();
            let mut engine = Engine::new(strategy, executor);

            for seq in 0..10000 {
                let snapshot = create_snapshot(seq);
                // Alternate between fresh and stale to simulate realistic conditions
                let data_fresh = seq % 10 != 0; // 90% fresh, 10% stale
                engine.process_tick(&snapshot, data_fresh).unwrap();
            }
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_sustained_tick_rate,
    bench_max_orders_per_second,
    bench_max_fills_per_second,
    bench_realistic_throughput,
);

criterion_main!(benches);
