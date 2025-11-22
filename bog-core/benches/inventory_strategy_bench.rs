//! Benchmark: InventoryBased Strategy Performance
//!
//! Purpose: Measure latency of inventory-based market making strategy calculations
//! Target: <50ns per calculation (comparable to SimpleSpread at 15ns)
//!
//! What's Measured:
//! - Strategy.calculate() with various inventory levels
//! - Inventory skew calculation
//! - Optimal spread calculation
//! - Comparison with SimpleSpread baseline
//!
//! Why This Matters:
//! InventoryBased runs on every market tick. Must maintain sub-microsecond latency.
//! Even though it's more complex than SimpleSpread, zero-cost abstractions should
//! keep overhead minimal.

use bog_core::core::Position;
use bog_core::data::MarketSnapshot;
use bog_core::engine::Strategy;
use bog_strategies::InventoryBased;
use criterion::{black_box, criterion_group, criterion_main, Criterion};

/// Helper: Create realistic market snapshot
fn create_market_snapshot() -> MarketSnapshot {
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
        best_ask_price: 50_010_000_000_000,   // $50,010
        best_ask_size: 1_000_000_000,          // 1.0 BTC
        bid_prices: [0; 10],
        bid_sizes: [0; 10],
        ask_prices: [0; 10],
        ask_sizes: [0; 10],
        snapshot_flags: 0,
        dex_type: 1,
        _padding: [0; 110],
    }
}

/// Helper: Create position with specific inventory
fn create_position_with_inventory(quantity: i64) -> Position {
    let position = Position::new();
    // Set inventory using update_quantity
    position.update_quantity(quantity);
    position
}

/// Benchmark: Basic InventoryBased calculation (neutral inventory)
fn bench_inventory_neutral(c: &mut Criterion) {
    let mut group = c.benchmark_group("inventory_strategy");
    group.significance_level(0.01).sample_size(10000);

    let mut strategy = InventoryBased;
    let snapshot = create_market_snapshot();
    let position = Position::new(); // 0 inventory

    group.bench_function("neutral_inventory", |b| {
        b.iter(|| {
            black_box(strategy.calculate(black_box(&snapshot), black_box(&position)));
        });
    });

    group.finish();
}

/// Benchmark: InventoryBased with small positive inventory
fn bench_inventory_small_positive(c: &mut Criterion) {
    let mut group = c.benchmark_group("inventory_strategy");
    group.significance_level(0.01).sample_size(10000);

    let mut strategy = InventoryBased;
    let snapshot = create_market_snapshot();
    let position = create_position_with_inventory(50_000_000); // 0.05 BTC

    group.bench_function("small_positive", |b| {
        b.iter(|| {
            black_box(strategy.calculate(black_box(&snapshot), black_box(&position)));
        });
    });

    group.finish();
}

/// Benchmark: InventoryBased with large positive inventory
fn bench_inventory_large_positive(c: &mut Criterion) {
    let mut group = c.benchmark_group("inventory_strategy");
    group.significance_level(0.01).sample_size(10000);

    let mut strategy = InventoryBased;
    let snapshot = create_market_snapshot();
    let position = create_position_with_inventory(1_000_000_000); // 1.0 BTC (max limit)

    group.bench_function("large_positive", |b| {
        b.iter(|| {
            black_box(strategy.calculate(black_box(&snapshot), black_box(&position)));
        });
    });

    group.finish();
}

/// Benchmark: InventoryBased with small negative inventory
fn bench_inventory_small_negative(c: &mut Criterion) {
    let mut group = c.benchmark_group("inventory_strategy");
    group.significance_level(0.01).sample_size(10000);

    let mut strategy = InventoryBased;
    let snapshot = create_market_snapshot();
    let position = create_position_with_inventory(-50_000_000); // -0.05 BTC

    group.bench_function("small_negative", |b| {
        b.iter(|| {
            black_box(strategy.calculate(black_box(&snapshot), black_box(&position)));
        });
    });

    group.finish();
}

/// Benchmark: InventoryBased with large negative inventory
fn bench_inventory_large_negative(c: &mut Criterion) {
    let mut group = c.benchmark_group("inventory_strategy");
    group.significance_level(0.01).sample_size(10000);

    let mut strategy = InventoryBased;
    let snapshot = create_market_snapshot();
    let position = create_position_with_inventory(-1_000_000_000); // -1.0 BTC (max short)

    group.bench_function("large_negative", |b| {
        b.iter(|| {
            black_box(strategy.calculate(black_box(&snapshot), black_box(&position)));
        });
    });

    group.finish();
}

/// Benchmark: Compare InventoryBased vs SimpleSpread
fn bench_strategy_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("strategy_comparison");
    group.significance_level(0.01).sample_size(10000);

    let snapshot = create_market_snapshot();
    let position = Position::new();

    // Benchmark InventoryBased
    let mut inventory_strategy = InventoryBased;
    group.bench_function("inventory_based", |b| {
        b.iter(|| {
            black_box(inventory_strategy.calculate(black_box(&snapshot), black_box(&position)));
        });
    });

    // Benchmark SimpleSpread for comparison
    let mut simple_strategy = bog_strategies::SimpleSpread;
    group.bench_function("simple_spread", |b| {
        b.iter(|| {
            black_box(simple_strategy.calculate(black_box(&snapshot), black_box(&position)));
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_inventory_neutral,
    bench_inventory_small_positive,
    bench_inventory_large_positive,
    bench_inventory_small_negative,
    bench_inventory_large_negative,
    bench_strategy_comparison,
);

criterion_main!(benches);
