use std::sync::Arc;
use std::thread;
use std::time::Duration;

use aerocache::{Cache, EvictionStrategy};

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct UserSession {
    user_id: u64,
    username: String,
    roles: Vec<String>,
    token: String,
}

fn main() {
    println!("============================================================");
    println!("     🚀 AeroCache - Enterprise Showcase & Concurrency Demo  ");
    println!("============================================================");

    // 1. Configure an enterprise-grade cache
    let cache: Cache<String, UserSession> = Cache::builder()
        .max_capacity(50_000)
        .num_shards(64) // Power-of-2 cache-line padded shards
        .time_to_live(Duration::from_millis(500)) // 500ms session TTL
        .eviction_strategy(EvictionStrategy::SampledLRU { sample_size: 8 })
        .build();

    // 2. Start proactive background sweeper (cleans expired keys every 100ms)
    let _sweeper = cache.start_sweeper(Duration::from_millis(100));

    // 3. Pre-populate some initial session entries
    for i in 0..1_000 {
        cache.insert(
            format!("session:{i}"),
            UserSession {
                user_id: i,
                username: format!("enterprise_user_{i}"),
                roles: vec!["member".to_string(), "api_consumer".to_string()],
                token: format!("jwt.ey_signature_{i}"),
            },
        );
    }

    println!(
        "Initialized cache with {} items across {} shards.",
        cache.len(),
        cache.num_shards()
    );

    // 4. Zero-copy inspection (No memory clone of UserSession!)
    println!("\n[Zero-Copy Read Inspection]");
    let has_api_role = cache.with_value("session:42", |session| {
        println!("  Inspecting session for user ID: {}", session.user_id);
        session.roles.contains(&"api_consumer".to_string())
    });
    println!("  User 42 has 'api_consumer' role? {:?}", has_api_role);

    // 5. In-place mutation (Add admin role without reallocating session)
    cache.with_value_mut("session:42", |session| {
        session.roles.push("admin".to_string());
    });

    let roles = cache.with_value("session:42", |s| s.roles.clone());
    println!("  Updated roles for user 42: {:?}", roles);

    // 6. Simulate high-concurrency enterprise traffic (8 concurrent workers)
    println!("\n[Simulating 8 Concurrent Worker Threads]");
    let shared_cache = Arc::new(cache);
    let mut workers = Vec::new();

    let start = std::time::Instant::now();
    for thread_id in 0..8 {
        let c = Arc::clone(&shared_cache);
        workers.push(thread::spawn(move || {
            for i in 0..5_000 {
                let session_id = format!("session:{}", (thread_id * 500 + i) % 2_000);

                if i % 10 == 0 {
                    // 10% writes (updates / new sessions)
                    c.insert(
                        session_id,
                        UserSession {
                            user_id: i as u64,
                            username: format!("worker_{thread_id}_user_{i}"),
                            roles: vec!["active".to_string()],
                            token: "refreshed_token".to_string(),
                        },
                    );
                } else {
                    // 90% zero-copy reads
                    let _ = c.with_value(&session_id, |s| s.user_id);
                }
            }
        }));
    }

    for worker in workers {
        worker.join().expect("Worker thread panicked");
    }
    let elapsed = start.elapsed();

    println!(
        "Completed 40,000 multi-threaded operations in {:?}",
        elapsed
    );
    let ops_per_sec = (40_000.0 / elapsed.as_secs_f64()) as u64;
    println!("Throughput: {} ops/sec", ops_per_sec);

    // 7. Telemetry & Metrics Snapshot
    let metrics = shared_cache.metrics();
    println!("\n[Enterprise Telemetry Snapshot]");
    println!("  Total Lookups : {}", metrics.total_lookups());
    println!("  Cache Hits    : {}", metrics.hits);
    println!("  Cache Misses  : {}", metrics.misses);
    println!("  Hit Ratio     : {:.2}%", metrics.hit_ratio() * 100.0);
    println!("  Insertions    : {}", metrics.insertions);
    println!("  Updates       : {}", metrics.updates);
    println!("  Evictions     : {}", metrics.evictions);
    println!("  Expirations   : {}", metrics.expirations);
    println!("  Active Items  : {}", shared_cache.len());

    println!("\n✅ AeroCache enterprise showcase successfully finished!");
}
