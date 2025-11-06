use bog_core::orderbook::depth::*;
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use huginn::shm::MarketSnapshot;

fn create_test_snapshot() -> MarketSnapshot {
    MarketSnapshot {
        market_id: 1,
        sequence: 100,
        exchange_timestamp_ns: 0,
        local_recv_ns: 0,
        local_publish_ns: 0,
        best_bid_price: 50_000_000_000_000, // $50,000
        best_bid_size: 1_000_000_000,       // 1.0 BTC
        best_ask_price: 50_010_000_000_000, // $50,010
        best_ask_size: 1_000_000_000,
        bid_prices: [
            49_990_000_000_000,
            49_980_000_000_000,
            49_970_000_000_000,
            49_960_000_000_000,
            49_950_000_000_000,
            0, 0, 0, 0, 0,
        ],
        bid_sizes: [
            2_000_000_000,
            3_000_000_000,
            1_000_000_000,
            500_000_000,
            500_000_000,
            0, 0, 0, 0, 0,
        ],
        ask_prices: [
            50_020_000_000_000,
            50_030_000_000_000,
            50_040_000_000_000,
            0, 0, 0, 0, 0, 0, 0,
        ],
        ask_sizes: [
            1_500_000_000,
            1_000_000_000,
            500_000_000,
            0, 0, 0, 0, 0, 0, 0,
        ],
        dex_type: 1,
        ..Default::default()
    }
}

fn bench_vwap_calculation(c: &mut Criterion) {
    let snapshot = create_test_snapshot();

    c.bench_function("depth/vwap_bid_5_levels", |b| {
        b.iter(|| {
            black_box(calculate_vwap(
                black_box(&snapshot),
                black_box(true),
                black_box(5),
            ))
        })
    });

    c.bench_function("depth/vwap_ask_3_levels", |b| {
        b.iter(|| {
            black_box(calculate_vwap(
                black_box(&snapshot),
                black_box(false),
                black_box(3),
            ))
        })
    });
}

fn bench_imbalance_calculation(c: &mut Criterion) {
    let snapshot = create_test_snapshot();

    c.bench_function("depth/imbalance_5_levels", |b| {
        b.iter(|| black_box(calculate_imbalance(black_box(&snapshot), black_box(5))))
    });

    c.bench_function("depth/imbalance_10_levels", |b| {
        b.iter(|| black_box(calculate_imbalance(black_box(&snapshot), black_box(10))))
    });
}

fn bench_liquidity_calculation(c: &mut Criterion) {
    let snapshot = create_test_snapshot();

    c.bench_function("depth/liquidity_bid_5", |b| {
        b.iter(|| {
            black_box(calculate_liquidity(
                black_box(&snapshot),
                black_box(true),
                black_box(5),
            ))
        })
    });
}

fn bench_mid_and_spread(c: &mut Criterion) {
    let snapshot = create_test_snapshot();

    c.bench_function("depth/mid_price", |b| {
        b.iter(|| black_box(mid_price(black_box(&snapshot))))
    });

    c.bench_function("depth/spread_bps", |b| {
        b.iter(|| black_box(spread_bps(black_box(&snapshot))))
    });
}

criterion_group!(
    benches,
    bench_vwap_calculation,
    bench_imbalance_calculation,
    bench_liquidity_calculation,
    bench_mid_and_spread,
);
criterion_main!(benches);
