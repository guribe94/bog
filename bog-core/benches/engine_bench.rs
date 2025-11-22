//! Engine Performance Benchmarks
//!
//! Comprehensive benchmarks for HFT performance validation.
//! Target: <1μs (1000ns) tick-to-trade latency

use bog_core::core::{Position, Signal};
use bog_core::data::MarketSnapshot;
use bog_core::engine::{Engine, SimulatedExecutor, Strategy, Executor};
use bog_strategies::SimpleSpread;
use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};

/// Create a test market snapshot
fn create_market_snapshot(price_offset: u64) -> MarketSnapshot {
    MarketSnapshot {
        market_id: 1,
        sequence: 1,
        exchange_timestamp_ns: 0,
        local_recv_ns: 0,
        local_publish_ns: 0,
        best_bid_price: 50_000_000_000_000 + price_offset,
        best_bid_size: 1_000_000_000,
        best_ask_price: 50_010_000_000_000 + price_offset,
        best_ask_size: 1_000_000_000,
        bid_prices: [0; 10],
        bid_sizes: [0; 10],
        ask_prices: [0; 10],
        ask_sizes: [0; 10],
        snapshot_flags: 0,
        dex_type: 1,
        _padding: [0; 110],
    }
}

/// Benchmark: Complete tick processing (engine hot path)
fn bench_engine_tick_processing(c: &mut Criterion) {
    let mut group = c.benchmark_group("engine_tick_processing");

    // Target: <1μs (1000ns)
    group.significance_level(0.01).sample_size(10000);

    let strategy = SimpleSpread;
    let executor = SimulatedExecutor::new_default();
    let mut engine = Engine::new(strategy, executor);
    let snapshot = create_market_snapshot(0);

    group.bench_function("tick_processing", |b| {
        b.iter(|| {
            black_box(engine.process_tick(black_box(&snapshot), true).unwrap());
        });
    });

    group.finish();
}

/// Benchmark: Strategy calculation only
fn bench_strategy_calculation(c: &mut Criterion) {
    let mut group = c.benchmark_group("strategy_calculation");
    group.significance_level(0.01).sample_size(10000);

    let mut strategy = SimpleSpread;
    let snapshot = create_market_snapshot(0);
    let position = Position::new();

    group.bench_function("simple_spread", |b| {
        b.iter(|| {
            black_box(strategy.calculate(black_box(&snapshot), black_box(&position)));
        });
    });

    group.finish();
}

/// Benchmark: Risk validation
fn bench_risk_validation(c: &mut Criterion) {
    let mut group = c.benchmark_group("risk_validation");
    group.significance_level(0.01).sample_size(10000);

    let position = Position::new();
    let signal = Signal::quote_both(
        49_995_000_000_000,
        50_005_000_000_000,
        100_000_000,
    );

    group.bench_function("validate_signal", |b| {
        b.iter(|| {
            black_box(bog_core::engine::risk::validate_signal(
                black_box(&signal),
                black_box(&position),
            ))
            .unwrap();
        });
    });

    group.finish();
}

/// Benchmark: Executor execution
fn bench_executor(c: &mut Criterion) {
    let mut group = c.benchmark_group("executor");
    group.significance_level(0.01).sample_size(10000);

    let mut executor = SimulatedExecutor::new_default();
    let position = Position::new();
    let signal = Signal::quote_both(
        49_995_000_000_000,
        50_005_000_000_000,
        100_000_000,
    );

    let mut iteration = 0;
    group.bench_function("execute_signal", |b| {
        b.iter(|| {
            black_box(executor.execute(black_box(signal), black_box(&position)).unwrap());

            // Drain fills every 100 iterations to prevent queue overflow
            iteration += 1;
            if iteration % 100 == 0 {
                let _ = executor.get_fills();
            }
        });
    });

    group.finish();
}

/// Benchmark: Signal creation
fn bench_signal_creation(c: &mut Criterion) {
    let mut group = c.benchmark_group("signal_creation");
    group.significance_level(0.01).sample_size(10000);

    group.bench_function("quote_both", |b| {
        b.iter(|| {
            black_box(Signal::quote_both(
                black_box(49_995_000_000_000),
                black_box(50_005_000_000_000),
                black_box(100_000_000),
            ));
        });
    });

    group.finish();
}

/// Benchmark: Position operations
fn bench_position_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("position_operations");
    group.significance_level(0.01).sample_size(10000);

    let position = Position::new();

    group.bench_function("get_quantity", |b| {
        b.iter(|| {
            black_box(position.get_quantity());
        });
    });

    group.bench_function("get_realized_pnl", |b| {
        b.iter(|| {
            black_box(position.get_realized_pnl());
        });
    });

    group.finish();
}

/// Benchmark: Market change detection
fn bench_market_change_detection(c: &mut Criterion) {
    let mut group = c.benchmark_group("market_change");
    group.significance_level(0.01).sample_size(10000);

    let strategy = SimpleSpread;
    let executor = SimulatedExecutor::new_default();
    let mut engine = Engine::new(strategy, executor);
    let snapshot = create_market_snapshot(0);

    // Prime the engine with first tick
    engine.process_tick(&snapshot, true).unwrap();

    group.bench_function("same_market", |b| {
        b.iter(|| {
            // Should skip due to market change detection
            black_box(engine.process_tick(black_box(&snapshot), true).unwrap());
        });
    });

    group.finish();
}

/// Benchmark: Varying order sizes
fn bench_varying_order_sizes(c: &mut Criterion) {
    let mut group = c.benchmark_group("varying_order_sizes");
    group.significance_level(0.01).sample_size(1000);

    let position = Position::new();

    for size in [10_000_000, 100_000_000, 500_000_000].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            let signal = Signal::quote_both(
                49_995_000_000_000,
                50_005_000_000_000,
                size,
            );
            let mut executor = SimulatedExecutor::new_default();
            let mut iteration = 0;

            b.iter(|| {
                black_box(executor.execute(black_box(signal), black_box(&position)).unwrap());

                // Drain fills every 100 iterations
                iteration += 1;
                if iteration % 100 == 0 {
                    let _ = executor.get_fills();
                }
            });
        });
    }

    group.finish();
}

/// Benchmark: Full tick-to-trade pipeline
fn bench_tick_to_trade_pipeline(c: &mut Criterion) {
    let mut group = c.benchmark_group("tick_to_trade_pipeline");

    // This is the critical benchmark - measures complete latency
    group.significance_level(0.01).sample_size(10000);

    let strategy = SimpleSpread;
    let executor = SimulatedExecutor::new_default();
    let mut engine = Engine::new(strategy, executor);

    group.bench_function("complete_pipeline", |b| {
        let mut tick_num = 0u64;

        b.iter(|| {
            // Create varying market data
            let snapshot = create_market_snapshot(tick_num * 1_000_000_000);
            tick_num += 1;

            // Complete tick-to-trade pipeline
            black_box(engine.process_tick(black_box(&snapshot), true).unwrap());
        });
    });

    group.finish();
}

/// Benchmark: Stale data path (data_fresh = false)
fn bench_stale_data_path(c: &mut Criterion) {
    let mut group = c.benchmark_group("error_paths");
    group.significance_level(0.01).sample_size(10000);

    let strategy = SimpleSpread;
    let executor = SimulatedExecutor::new_default();
    let mut engine = Engine::new(strategy, executor);
    let snapshot = create_market_snapshot(0);

    group.bench_function("stale_data_skip", |b| {
        b.iter(|| {
            // Pass data_fresh = false to trigger early exit
            black_box(engine.process_tick(black_box(&snapshot), false).unwrap());
        });
    });

    group.finish();
}

/// Benchmark: Invalid market data (crossed book)
fn bench_invalid_market_data(c: &mut Criterion) {
    let mut group = c.benchmark_group("error_paths");
    group.significance_level(0.01).sample_size(10000);

    let mut strategy = SimpleSpread;
    let position = Position::new();

    // Create crossed book (bid > ask - invalid)
    let mut snapshot = create_market_snapshot(0);
    snapshot.best_bid_price = 50_010_000_000_000;
    snapshot.best_ask_price = 50_000_000_000_000; // Crossed!

    group.bench_function("invalid_market", |b| {
        b.iter(|| {
            // Strategy should return None for invalid data
            black_box(strategy.calculate(black_box(&snapshot), black_box(&position)));
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_engine_tick_processing,
    bench_strategy_calculation,
    bench_risk_validation,
    bench_executor,
    bench_signal_creation,
    bench_position_operations,
    bench_market_change_detection,
    bench_varying_order_sizes,
    bench_tick_to_trade_pipeline,
    bench_stale_data_path,
    bench_invalid_market_data,
);

criterion_main!(benches);
