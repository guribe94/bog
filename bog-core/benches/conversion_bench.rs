//! Conversion Benchmarks
//!
//! Measures performance of all type conversion operations used in the trading system.
//! These conversions happen frequently and must be fast to maintain sub-microsecond latency.
//!
//! ## Conversions Tested
//!
//! 1. **Decimal ↔ u64** - Used in fills, position updates, order placement
//! 2. **f64 ↔ u64** - Used in config parsing, metrics, logging
//!
//! ## Test Scenarios
//!
//! - Small values (0.001)
//! - Medium values (1.0)
//! - Large values (1000.0)
//! - Edge cases (near limits)

use bog_core::data::conversions::*;
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rust_decimal_macros::dec;

fn decimal_to_u64_bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("conversion/decimal_to_u64");
    group.significance_level(0.01).sample_size(10000);

    // Small value (0.001 BTC - minimum order)
    group.bench_function("small_0.001", |b| {
        let value = dec!(0.001);
        b.iter(|| decimal_to_u64(black_box(value)));
    });

    // Medium value (1.0 BTC - typical order)
    group.bench_function("medium_1.0", |b| {
        let value = dec!(1.0);
        b.iter(|| decimal_to_u64(black_box(value)));
    });

    // Large value (100.0 BTC - large position)
    group.bench_function("large_100.0", |b| {
        let value = dec!(100.0);
        b.iter(|| decimal_to_u64(black_box(value)));
    });

    // Price value ($50,000)
    group.bench_function("price_50000", |b| {
        let value = dec!(50000.0);
        b.iter(|| decimal_to_u64(black_box(value)));
    });

    group.finish();
}

fn u64_to_decimal_bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("conversion/u64_to_decimal");
    group.significance_level(0.01).sample_size(10000);

    // Small quantity (0.001 BTC)
    group.bench_function("small_0.001", |b| {
        let value = 1_000_000u64; // 0.001 in fixed-point
        b.iter(|| u64_to_decimal(black_box(value)));
    });

    // Medium quantity (1.0 BTC)
    group.bench_function("medium_1.0", |b| {
        let value = 1_000_000_000u64;
        b.iter(|| u64_to_decimal(black_box(value)));
    });

    // Large quantity (100.0 BTC)
    group.bench_function("large_100.0", |b| {
        let value = 100_000_000_000u64;
        b.iter(|| u64_to_decimal(black_box(value)));
    });

    // Price ($50,000)
    group.bench_function("price_50000", |b| {
        let value = 50_000_000_000_000u64;
        b.iter(|| u64_to_decimal(black_box(value)));
    });

    group.finish();
}

fn f64_to_u64_bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("conversion/f64_to_u64");
    group.significance_level(0.01).sample_size(10000);

    group.bench_function("small_0.001", |b| {
        let value = 0.001f64;
        b.iter(|| f64_to_u64(black_box(value)));
    });

    group.bench_function("medium_1.0", |b| {
        let value = 1.0f64;
        b.iter(|| f64_to_u64(black_box(value)));
    });

    group.bench_function("large_1000.0", |b| {
        let value = 1000.0f64;
        b.iter(|| f64_to_u64(black_box(value)));
    });

    group.bench_function("price_50000", |b| {
        let value = 50000.0f64;
        b.iter(|| f64_to_u64(black_box(value)));
    });

    group.finish();
}

fn u64_to_f64_bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("conversion/u64_to_f64");
    group.significance_level(0.01).sample_size(10000);

    group.bench_function("small_0.001", |b| {
        let value = 1_000_000u64;
        b.iter(|| u64_to_f64(black_box(value)));
    });

    group.bench_function("medium_1.0", |b| {
        let value = 1_000_000_000u64;
        b.iter(|| u64_to_f64(black_box(value)));
    });

    group.bench_function("large_100.0", |b| {
        let value = 100_000_000_000u64;
        b.iter(|| u64_to_f64(black_box(value)));
    });

    group.bench_function("price_50000", |b| {
        let value = 50_000_000_000_000u64;
        b.iter(|| u64_to_f64(black_box(value)));
    });

    group.finish();
}

fn decimal_roundtrip_bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("conversion/decimal_roundtrip");
    group.significance_level(0.01).sample_size(10000);

    // Test precision loss in roundtrip conversion
    group.bench_function("roundtrip_1.0", |b| {
        let original = dec!(1.0);
        b.iter(|| {
            let u64_val = decimal_to_u64(black_box(original));
            u64_to_decimal(black_box(u64_val))
        });
    });

    group.bench_function("roundtrip_0.123456789", |b| {
        let original = dec!(0.123456789); // 9 decimal places (limit)
        b.iter(|| {
            let u64_val = decimal_to_u64(black_box(original));
            u64_to_decimal(black_box(u64_val))
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    decimal_to_u64_bench,
    u64_to_decimal_bench,
    f64_to_u64_bench,
    u64_to_f64_bench,
    decimal_roundtrip_bench
);
criterion_main!(benches);
