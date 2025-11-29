//! Benchmark: Multi-Tick Scenario Performance
//!
//! Purpose: Measure engine performance over realistic market sequences
//! Target: <100μs for 1000 ticks (100ns/tick average)
//!
//! What's Measured:
//! - Sequential tick processing (100, 1000 ticks)
//! - Oscillating market (buy/sell alternating)
//! - Trending market (accumulating inventory)
//! - Volatile market (large price swings)
//! - Strategy state evolution over time
//!
//! Why This Matters:
//! Real trading involves processing thousands of sequential ticks. This tests:
//! - Sustained performance (not just single-tick)
//! - Cache behavior over time
//! - Position accumulation handling
//! - Strategy adaptation

use bog_core::core::Position;
use bog_core::data::MarketSnapshot;
use bog_core::engine::{Engine, Executor, SimulatedExecutor, Strategy};
use bog_strategies::SimpleSpread;
use criterion::{black_box, criterion_group, criterion_main, Criterion};

/// Helper: Create market snapshot with varying price
fn create_market_snapshot(base_price_delta: i64, sequence: u64) -> MarketSnapshot {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;

    let base_price = 50_000_000_000_000u64; // $50,000
    let bid_price = if base_price_delta >= 0 {
        base_price.saturating_add(base_price_delta as u64)
    } else {
        base_price.saturating_sub((-base_price_delta) as u64)
    };
    let ask_price = bid_price + 5_000_000_000; // +$5 (1bps spread)

    MarketSnapshot {
        market_id: 1,
        sequence,
        exchange_timestamp_ns: now,
        local_recv_ns: now,
        local_publish_ns: now,
        best_bid_price: bid_price,
        best_bid_size: 1_000_000_000, // 1.0 BTC
        best_ask_price: ask_price,
        best_ask_size: 1_000_000_000, // 1.0 BTC
        bid_prices: [0; 10],
        bid_sizes: [0; 10],
        ask_prices: [0; 10],
        ask_sizes: [0; 10],
        snapshot_flags: 0,
        dex_type: 1,
        _padding: [0; 54],
    }
}

/// Benchmark: 100 sequential ticks (realistic short-term)
fn bench_100_sequential_ticks(c: &mut Criterion) {
    let mut group = c.benchmark_group("multi_tick");
    group.significance_level(0.01).sample_size(1000); // Lower sample size for heavier test

    group.bench_function("100_ticks_sequential", |b| {
        b.iter(|| {
            let strategy = SimpleSpread;
            let executor = SimulatedExecutor::new_default();
            let mut engine = Engine::new(strategy, executor);

            for tick in 0..100 {
                let snapshot = create_market_snapshot(0, tick);
                black_box(engine.process_tick(&snapshot, true).unwrap());
            }
        });
    });

    group.finish();
}

/// Benchmark: 1000 sequential ticks (stress test)
fn bench_1000_sequential_ticks(c: &mut Criterion) {
    let mut group = c.benchmark_group("multi_tick");
    group.significance_level(0.01).sample_size(100); // Even lower for heavy test

    group.bench_function("1000_ticks_sequential", |b| {
        b.iter(|| {
            let strategy = SimpleSpread;
            let executor = SimulatedExecutor::new_default();
            let mut engine = Engine::new(strategy, executor);

            for tick in 0..1000 {
                let snapshot = create_market_snapshot(0, tick);
                black_box(engine.process_tick(&snapshot, true).unwrap());
            }
        });
    });

    group.finish();
}

/// Benchmark: Oscillating market (price bounces up/down)
fn bench_oscillating_market(c: &mut Criterion) {
    let mut group = c.benchmark_group("multi_tick");
    group.significance_level(0.01).sample_size(1000);

    group.bench_function("1000_ticks_oscillating", |b| {
        b.iter(|| {
            let strategy = SimpleSpread;
            let executor = SimulatedExecutor::new_default();
            let mut engine = Engine::new(strategy, executor);

            for tick in 0..1000 {
                // Oscillate: +$10, -$10, +$10, -$10, ...
                let price_delta = if tick % 2 == 0 {
                    10_000_000_000
                } else {
                    -10_000_000_000
                };
                let snapshot = create_market_snapshot(price_delta, tick);
                black_box(engine.process_tick(&snapshot, true).unwrap());
            }
        });
    });

    group.finish();
}

/// Benchmark: Trending market (accumulating inventory)
fn bench_trending_market(c: &mut Criterion) {
    let mut group = c.benchmark_group("multi_tick");
    group.significance_level(0.01).sample_size(1000);

    group.bench_function("1000_ticks_trending_up", |b| {
        b.iter(|| {
            let strategy = SimpleSpread;
            let executor = SimulatedExecutor::new_default();
            let mut engine = Engine::new(strategy, executor);

            for tick in 0..1000 {
                // Trend up: +$1 per tick
                let price_delta = (tick as i64) * 1_000_000_000; // +$1 per tick
                let snapshot = create_market_snapshot(price_delta, tick);
                black_box(engine.process_tick(&snapshot, true).unwrap());
            }
        });
    });

    group.finish();
}

/// Benchmark: Volatile market (large price swings)
fn bench_volatile_market(c: &mut Criterion) {
    let mut group = c.benchmark_group("multi_tick");
    group.significance_level(0.01).sample_size(1000);

    group.bench_function("1000_ticks_volatile", |b| {
        b.iter(|| {
            let strategy = SimpleSpread;
            let executor = SimulatedExecutor::new_default();
            let mut engine = Engine::new(strategy, executor);

            for tick in 0..1000 {
                // Large swings: +$100, -$100, +$100, -$100, ...
                let price_delta = if tick % 2 == 0 {
                    100_000_000_000
                } else {
                    -100_000_000_000
                };
                let snapshot = create_market_snapshot(price_delta, tick);
                black_box(engine.process_tick(&snapshot, true).unwrap());
            }
        });
    });

    group.finish();
}

/// Benchmark: Flash crash scenario (rapid price drop + high volume)
fn bench_flash_crash(c: &mut Criterion) {
    let mut group = c.benchmark_group("multi_tick");
    group.significance_level(0.01).sample_size(1000);

    group.bench_function("flash_crash_scenario", |b| {
        b.iter(|| {
            let strategy = SimpleSpread;
            let executor = SimulatedExecutor::new_default();
            let mut engine = Engine::new(strategy, executor);

            // 1. Stable period (100 ticks)
            for tick in 0..100 {
                let snapshot = create_market_snapshot(0, tick);
                engine.process_tick(&snapshot, true).unwrap();
            }

            // 2. Crash: Price drops 10% in 50 ticks, Volume triples
            let mut price_delta = 0i64;
            for tick in 100..150 {
                price_delta -= 100_000_000_000; // -$100 per tick
                let mut snapshot = create_market_snapshot(price_delta, tick);
                snapshot.best_ask_size *= 3; // Panic selling volume
                snapshot.best_bid_size /= 2; // Liquidity drying up
                black_box(engine.process_tick(&snapshot, true).unwrap());
            }

            // 3. Recovery/High Volatility (100 ticks)
            for tick in 150..250 {
                // Extreme volatility: +/- $50 alternating
                if tick % 2 == 0 {
                    price_delta += 50_000_000_000;
                } else {
                    price_delta -= 50_000_000_000;
                }

                let snapshot = create_market_snapshot(price_delta, tick);
                black_box(engine.process_tick(&snapshot, true).unwrap());
            }
        });
    });

    group.finish();
}

/// Benchmark: Market with occasional gaps (realistic)
fn bench_market_with_gaps(c: &mut Criterion) {
    let mut group = c.benchmark_group("multi_tick");
    group.significance_level(0.01).sample_size(1000);

    group.bench_function("1000_ticks_with_gaps", |b| {
        b.iter(|| {
            let strategy = SimpleSpread;
            let executor = SimulatedExecutor::new_default();
            let mut engine = Engine::new(strategy, executor);

            for tick in 0..1000 {
                // Every 10th tick has no change (market unchanged)
                let price_delta = if tick % 10 == 0 {
                    0
                } else {
                    (tick as i64 % 5) * 1_000_000_000
                };
                let snapshot = create_market_snapshot(price_delta, tick);
                black_box(engine.process_tick(&snapshot, true).unwrap());
            }
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_100_sequential_ticks,
    bench_1000_sequential_ticks,
    bench_oscillating_market,
    bench_trending_market,
    bench_volatile_market,
    bench_flash_crash,
    bench_market_with_gaps,
);

criterion_main!(benches);
