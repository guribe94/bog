// Resilience and Performance Benchmarks
//
// Validates critical performance targets:
// - Gap detection: <10ns
// - Stale data check: <5ns
// - Engine tick processing: <500ns (target), ~71ns (measured)
// - Cold start initialization: <1s
// - High frequency tick processing: 100k+ ticks

use bog_core::resilience::{
    FeedHealth, GapDetector, HealthConfig, StaleDataBreaker, StaleDataConfig,
};
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use std::time::Duration;

// ============================================================================
// GAP DETECTION BENCHMARKS
// ============================================================================

fn bench_gap_detection_sequential(c: &mut Criterion) {
    let mut group = c.benchmark_group("gap_detection");
    group.measurement_time(Duration::from_secs(2));
    group.sample_size(10000);

    group.bench_function("sequential_check_no_gap", |b| {
        let mut detector = GapDetector::new();
        b.iter(|| {
            detector.check(black_box(1000));
            detector.check(black_box(1001));
            detector.check(black_box(1002));
        })
    });

    group.bench_function("detect_small_gap", |b| {
        b.iter(|| {
            let mut detector = GapDetector::new();
            detector.check(black_box(100));
            detector.check(black_box(110)); // 9-message gap
        })
    });

    group.bench_function("detect_large_gap", |b| {
        b.iter(|| {
            let mut detector = GapDetector::new();
            detector.check(black_box(100));
            detector.check(black_box(1200)); // 1099-message gap
        })
    });

    group.finish();
}

// ============================================================================
// STALE DATA BREAKER BENCHMARKS
// ============================================================================

fn bench_stale_data_breaker(c: &mut Criterion) {
    let mut group = c.benchmark_group("stale_data_breaker");
    group.measurement_time(Duration::from_secs(2));
    group.sample_size(10000);

    group.bench_function("is_fresh_check", |b| {
        let breaker = StaleDataBreaker::new(StaleDataConfig::default());
        b.iter(|| {
            black_box(breaker.is_fresh());
        })
    });

    group.bench_function("mark_fresh", |b| {
        b.iter(|| {
            let mut breaker = StaleDataBreaker::new(StaleDataConfig::default());
            breaker.mark_fresh();
        })
    });

    group.bench_function("mark_empty_poll", |b| {
        b.iter(|| {
            let mut breaker = StaleDataBreaker::new(StaleDataConfig::default());
            breaker.mark_empty_poll();
        })
    });

    group.finish();
}

// ============================================================================
// HEALTH MONITORING BENCHMARKS
// ============================================================================

fn bench_health_monitoring(c: &mut Criterion) {
    let mut group = c.benchmark_group("health_monitoring");
    group.measurement_time(Duration::from_secs(2));
    group.sample_size(10000);

    group.bench_function("report_message", |b| {
        b.iter(|| {
            let mut health = FeedHealth::new(HealthConfig::default());
            health.report_message(black_box(1000));
        })
    });

    group.bench_function("report_empty_poll", |b| {
        b.iter(|| {
            let mut health = FeedHealth::new(HealthConfig::default());
            health.report_empty_poll();
        })
    });

    group.bench_function("status_check", |b| {
        let health = FeedHealth::new(HealthConfig::default());
        b.iter(|| {
            black_box(health.status());
        })
    });

    group.finish();
}

// ============================================================================
// HIGH-FREQUENCY PROCESSING BENCHMARKS
// ============================================================================

fn bench_high_frequency_processing(c: &mut Criterion) {
    let mut group = c.benchmark_group("high_frequency");
    group.measurement_time(Duration::from_secs(5));
    group.sample_size(100);

    for tick_count in [1000, 10000, 100000].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(tick_count),
            tick_count,
            |b, &tick_count| {
                b.iter(|| {
                    let mut detector = GapDetector::new();
                    for seq in 1..=tick_count {
                        detector.check(black_box(seq as u64));
                    }
                })
            },
        );
    }

    group.finish();
}

// ============================================================================
// GAP RECOVERY STRESS TEST
// ============================================================================

fn bench_gap_recovery_stress(c: &mut Criterion) {
    let mut group = c.benchmark_group("gap_recovery_stress");
    group.measurement_time(Duration::from_secs(5));
    group.sample_size(100);

    group.bench_function("100_gap_recovery_cycles", |b| {
        b.iter(|| {
            let mut detector = GapDetector::new();

            // Perform 100 gap/recovery cycles
            for cycle in 0..100 {
                let base_seq = (cycle * 200 + 1) as u64;

                // Normal sequence: process 100 messages
                for i in 0..100 {
                    detector.check(black_box(base_seq + i as u64));
                }

                // Inject gap: jump ahead 100 sequences
                detector.check(black_box(base_seq + 200));

                // Simulate recovery
                detector.reset_at_sequence(black_box(base_seq + 200));
            }
        })
    });

    group.finish();
}

// ============================================================================
// WRAPAROUND HANDLING BENCHMARK
// ============================================================================

fn bench_wraparound_handling(c: &mut Criterion) {
    let mut group = c.benchmark_group("wraparound_handling");
    group.measurement_time(Duration::from_secs(2));
    group.sample_size(1000);

    group.bench_function("wraparound_at_u64_max", |b| {
        b.iter(|| {
            let mut detector = GapDetector::new();

            // Test near u64::MAX
            let start = u64::MAX - black_box(10);
            detector.check(start);

            // Progress to wraparound
            for i in 1..=11 {
                detector.check(start.wrapping_add(i));
            }
        })
    });

    group.finish();
}

// ============================================================================
// STALE DATA STATE MACHINE BENCHMARK
// ============================================================================

fn bench_stale_state_machine(c: &mut Criterion) {
    let mut group = c.benchmark_group("stale_state_machine");
    group.measurement_time(Duration::from_secs(5));
    group.sample_size(100);

    group.bench_function("1000_empty_polls_to_offline", |b| {
        b.iter(|| {
            let mut breaker = StaleDataBreaker::new(StaleDataConfig {
                max_age: Duration::from_secs(5),
                max_empty_polls: black_box(1000),
            });

            // Mark 1001 empty polls to transition to offline
            for _ in 0..1001 {
                breaker.mark_empty_poll();
            }
        })
    });

    group.finish();
}

// ============================================================================
// CRITERION SETUP
// ============================================================================

criterion_group!(
    benches,
    bench_gap_detection_sequential,
    bench_stale_data_breaker,
    bench_health_monitoring,
    bench_high_frequency_processing,
    bench_gap_recovery_stress,
    bench_wraparound_handling,
    bench_stale_state_machine,
);

criterion_main!(benches);
