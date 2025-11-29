//! Benchmark: Fill Processing Performance
//!
//! Purpose: Measure latency of fill handling and position updates
//! Target: <50ns per fill processing
//!
//! What's Measured:
//! - Single full fill processing
//! - Partial fill handling
//! - Position update from fill
//! - Multiple fills aggregation
//! - Fee calculation overhead
//!
//! Why This Matters:
//! Fill processing happens on every trade. Position updates must be atomic
//! and fast to maintain accurate PnL tracking.

use bog_core::core::Position;
use bog_core::execution::{Fill, OrderId, OrderStatus, Side};
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rust_decimal::Decimal;

/// Helper: Create a fill
fn create_fill(size: Decimal, price: Decimal, side: Side) -> Fill {
    Fill {
        order_id: OrderId::new_random(),
        side,
        price,
        size,
        timestamp: std::time::SystemTime::now(),
        fee: Some(Decimal::ZERO),
        fee_currency: Some("USD".to_string()),
    }
}

/// Benchmark: Single full fill processing
fn bench_single_fill_processing(c: &mut Criterion) {
    let mut group = c.benchmark_group("fill_processing");
    group.significance_level(0.01).sample_size(10000);

    let position = Position::new();
    let fill = create_fill(
        Decimal::from_str_exact("0.1").unwrap(),   // 0.1 BTC
        Decimal::from_str_exact("50000").unwrap(), // $50,000
        Side::Buy,
    );

    group.bench_function("single_fill", |b| {
        b.iter(|| {
            // Simulate fill processing: update position
            let quantity_i64 =
                (fill.size.mantissa() as i64) * if fill.side == Side::Buy { 1 } else { -1 };
            black_box(position.update_quantity(quantity_i64));
        });
    });

    group.finish();
}

/// Benchmark: Partial fill handling (multiple fills for one order)
fn bench_partial_fills(c: &mut Criterion) {
    let mut group = c.benchmark_group("fill_processing");
    group.significance_level(0.01).sample_size(10000);

    let position = Position::new();

    group.bench_function("10_partial_fills", |b| {
        b.iter(|| {
            // Simulate 10 partial fills
            for _ in 0..10 {
                let fill = create_fill(
                    Decimal::from_str_exact("0.01").unwrap(), // 0.01 BTC each
                    Decimal::from_str_exact("50000").unwrap(),
                    Side::Buy,
                );
                let quantity_i64 = (fill.size.mantissa() as i64);
                black_box(position.update_quantity(quantity_i64));
            }
        });
    });

    group.finish();
}

/// Benchmark: Fill aggregation (batch processing)
fn bench_fill_aggregation(c: &mut Criterion) {
    let mut group = c.benchmark_group("fill_processing");
    group.significance_level(0.01).sample_size(1000); // Lower sample for batch test

    let position = Position::new();

    group.bench_function("100_fills_batch", |b| {
        b.iter(|| {
            // Process 100 fills in batch
            for i in 0..100 {
                let fill = create_fill(
                    Decimal::from_str_exact("0.01").unwrap(),
                    Decimal::from_str_exact("50000").unwrap(),
                    if i % 2 == 0 { Side::Buy } else { Side::Sell },
                );
                let quantity_i64 =
                    (fill.size.mantissa() as i64) * if fill.side == Side::Buy { 1 } else { -1 };
                black_box(position.update_quantity(quantity_i64));
            }
        });
    });

    group.finish();
}

/// Benchmark: Position update with PnL calculation
fn bench_position_update_with_pnl(c: &mut Criterion) {
    let mut group = c.benchmark_group("fill_processing");
    group.significance_level(0.01).sample_size(10000);

    let position = Position::new();
    let quantity = 100_000_000i64; // 0.1 BTC in fixed-point
    let pnl = 5_000_000_000i64; // $5 profit

    group.bench_function("update_position_and_pnl", |b| {
        b.iter(|| {
            position.update_quantity(black_box(quantity));
            black_box(position.update_realized_pnl(black_box(pnl)));
        });
    });

    group.finish();
}

/// Benchmark: Fee calculation overhead
fn bench_fee_calculation(c: &mut Criterion) {
    let mut group = c.benchmark_group("fill_processing");
    group.significance_level(0.01).sample_size(10000);

    let fill_price = Decimal::from_str_exact("50000").unwrap();
    let fill_qty = Decimal::from_str_exact("0.1").unwrap();
    let fee_rate = Decimal::from_str_exact("0.0002").unwrap(); // 2bps

    group.bench_function("calculate_fee", |b| {
        b.iter(|| {
            let notional = black_box(fill_price * fill_qty);
            black_box(notional * fee_rate);
        });
    });

    group.finish();
}

/// Benchmark: Round-trip fill processing (buy then sell)
fn bench_round_trip_fills(c: &mut Criterion) {
    let mut group = c.benchmark_group("fill_processing");
    group.significance_level(0.01).sample_size(10000);

    let position = Position::new();

    group.bench_function("round_trip", |b| {
        b.iter(|| {
            // Buy fill
            let buy_fill = create_fill(
                Decimal::from_str_exact("0.1").unwrap(),
                Decimal::from_str_exact("50000").unwrap(),
                Side::Buy,
            );
            position.update_quantity(100_000_000); // +0.1 BTC

            // Sell fill (close position)
            let sell_fill = create_fill(
                Decimal::from_str_exact("0.1").unwrap(),
                Decimal::from_str_exact("50010").unwrap(),
                Side::Sell,
            );
            black_box(position.update_quantity(-100_000_000)); // -0.1 BTC
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_single_fill_processing,
    bench_partial_fills,
    bench_fill_aggregation,
    bench_position_update_with_pnl,
    bench_fee_calculation,
    bench_round_trip_fills,
);

criterion_main!(benches);
