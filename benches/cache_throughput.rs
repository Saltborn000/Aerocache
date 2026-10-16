use std::sync::Arc;
use std::thread;
use std::time::Duration;

use aerocache::Cache;
use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use std::hint::black_box;

fn bench_cache_reads(c: &mut Criterion) {
    let mut group = c.benchmark_group("cache_reads");

    let cache: Cache<String, u64> = Cache::builder()
        .max_capacity(100_000)
        .num_shards(64)
        .build();

    for i in 0..10_000 {
        cache.insert(format!("key:{i}"), i as u64);
    }

    group.throughput(Throughput::Elements(1));
    group.bench_function("single_thread_get", |b| {
        let mut i = 0;
        b.iter(|| {
            let key = format!("key:{}", i % 10_000);
            let val = cache.get(black_box(&key));
            i += 1;
            black_box(val)
        });
    });

    group.bench_function("zero_copy_with_value", |b| {
        let mut i = 0;
        b.iter(|| {
            let key = format!("key:{}", i % 10_000);
            let val = cache.with_value(black_box(&key), |v| *v);
            i += 1;
            black_box(val)
        });
    });

    group.finish();
}

fn bench_cache_concurrent_mixed(c: &mut Criterion) {
    let mut group = c.benchmark_group("concurrent_workload");

    for threads in [2, 4, 8] {
        group.bench_with_input(
            BenchmarkId::new("mixed_90_read_10_write", threads),
            &threads,
            |b, &num_threads| {
                let cache: Arc<Cache<u64, u64>> = Arc::new(
                    Cache::builder()
                        .max_capacity(100_000)
                        .num_shards(64)
                        .build(),
                );

                for i in 0..10_000 {
                    cache.insert(i, i * 2);
                }

                b.iter(|| {
                    let mut handles = Vec::with_capacity(num_threads);
                    for t in 0..num_threads {
                        let c = Arc::clone(&cache);
                        handles.push(thread::spawn(move || {
                            for i in 0..1_000 {
                                let key = (t as u64 * 1000 + i) % 10_000;
                                if i % 10 == 0 {
                                    c.insert(key, key + 1);
                                } else {
                                    black_box(c.get(&key));
                                }
                            }
                        }));
                    }
                    for h in handles {
                        h.join().unwrap();
                    }
                });
            },
        );
    }

    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default().warm_up_time(Duration::from_millis(500)).measurement_time(Duration::from_secs(1));
    targets = bench_cache_reads, bench_cache_concurrent_mixed
}
criterion_main!(benches);
