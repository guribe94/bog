//! TLS Overhead Micro-Benchmark
//!
//! This benchmark isolates the overhead of the timestamp caching mechanism
//! to understand why the OrderId optimization hurt overall performance.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::cell::Cell;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

// Original implementation (for comparison)
fn orderid_original() -> u128 {
    thread_local! {
        static COUNTER: Cell<u32> = Cell::new(0);
    }

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| std::time::Duration::from_nanos(0))
        .as_nanos() as u64;

    let counter = COUNTER.with(|c| {
        let val = c.get();
        c.set(val.wrapping_add(1));
        val
    });

    ((timestamp as u128) << 64) | (counter as u128)
}

// Optimized implementation (with caching)
fn orderid_optimized() -> u128 {
    thread_local! {
        static COUNTER: Cell<u32> = Cell::new(0);
        static CACHED_TS: Cell<(u64, Instant)> = Cell::new((0, Instant::now()));
    }

    let timestamp = CACHED_TS.with(|cache| {
        let (cached_ts, cached_instant) = cache.get();

        if cached_ts == 0 || cached_instant.elapsed().as_millis() >= 1 {
            let new_ts = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_else(|_| std::time::Duration::from_nanos(0))
                .as_nanos() as u64;

            cache.set((new_ts, Instant::now()));
            new_ts
        } else {
            cached_ts
        }
    });

    let counter = COUNTER.with(|c| {
        let val = c.get();
        c.set(val.wrapping_add(1));
        val
    });

    ((timestamp as u128) << 64) | (counter as u128)
}

// Just measure the TLS + caching overhead
fn tls_cache_overhead_only() {
    thread_local! {
        static CACHED: Cell<(u64, Instant)> = Cell::new((0, Instant::now()));
    }

    CACHED.with(|cache| {
        let (ts, instant) = cache.get();
        if instant.elapsed().as_millis() >= 1 {
            cache.set((ts + 1, Instant::now()));
        }
    });
}

fn bench_orderid_original(c: &mut Criterion) {
    c.bench_function("orderid/original", |b| {
        b.iter(|| black_box(orderid_original()));
    });
}

fn bench_orderid_optimized(c: &mut Criterion) {
    c.bench_function("orderid/optimized", |b| {
        b.iter(|| black_box(orderid_optimized()));
    });
}

fn bench_tls_overhead(c: &mut Criterion) {
    c.bench_function("orderid/tls_overhead_only", |b| {
        b.iter(|| black_box(tls_cache_overhead_only()));
    });
}

// Measure just elapsed() call cost
fn bench_instant_elapsed(c: &mut Criterion) {
    let instant = Instant::now();
    c.bench_function("orderid/instant_elapsed", |b| {
        b.iter(|| black_box(instant.elapsed().as_millis()));
    });
}

// Measure Cell get/set cost
fn bench_cell_operations(c: &mut Criterion) {
    thread_local! {
        static TEST_CELL: Cell<(u64, Instant)> = Cell::new((0, Instant::now()));
    }

    c.bench_function("orderid/cell_get_set", |b| {
        b.iter(|| {
            TEST_CELL.with(|cell| {
                let val = black_box(cell.get());
                cell.set(val);
            });
        });
    });
}

criterion_group!(
    benches,
    bench_orderid_original,
    bench_orderid_optimized,
    bench_tls_overhead,
    bench_instant_elapsed,
    bench_cell_operations
);
criterion_main!(benches);
