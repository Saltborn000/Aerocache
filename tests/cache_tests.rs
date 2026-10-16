use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;

use aerocache::{Cache, EvictionStrategy};

#[test]
fn test_basic_crud_operations() {
    let cache: Cache<String, i32> = Cache::new();

    assert_eq!(cache.len(), 0);
    assert!(cache.is_empty());

    // Insert
    assert_eq!(cache.insert("alpha".to_string(), 100), None);
    assert_eq!(cache.insert("beta".to_string(), 200), None);
    assert_eq!(cache.len(), 2);
    assert!(!cache.is_empty());

    // Update
    assert_eq!(cache.insert("alpha".to_string(), 150), Some(100));

    // Get
    assert_eq!(cache.get("alpha"), Some(150));
    assert_eq!(cache.get("beta"), Some(200));
    assert_eq!(cache.get("gamma"), None);

    // Contains
    assert!(cache.contains_key("alpha"));
    assert!(!cache.contains_key("gamma"));

    // Remove
    assert_eq!(cache.remove("alpha"), Some(150));
    assert_eq!(cache.get("alpha"), None);
    assert_eq!(cache.len(), 1);

    // Clear
    cache.clear();
    assert_eq!(cache.len(), 0);
    assert!(cache.is_empty());
}

#[test]
fn test_zero_copy_inspection_and_mutation() {
    let cache: Cache<String, Vec<u8>> = Cache::new();
    cache.insert("binary_payload".to_string(), vec![10, 20, 30, 40]);

    // Inspect by reference (zero-copy)
    let sum = cache.with_value("binary_payload", |vec| vec.iter().sum::<u8>());
    assert_eq!(sum, Some(100));

    // In-place mutation without reallocating
    cache.with_value_mut("binary_payload", |vec| vec.push(50));

    let len = cache.with_value("binary_payload", |vec| vec.len());
    assert_eq!(len, Some(5));

    let sum_after = cache.with_value("binary_payload", |vec| vec.iter().sum::<u8>());
    assert_eq!(sum_after, Some(150));
}

#[test]
fn test_get_or_insert_with() {
    let cache: Cache<String, String> = Cache::new();
    let compute_calls = Arc::new(AtomicUsize::new(0));

    let calls = Arc::clone(&compute_calls);
    let val1 = cache.get_or_insert_with("user:1".to_string(), || {
        calls.fetch_add(1, Ordering::SeqCst);
        "Alice".to_string()
    });
    assert_eq!(val1, "Alice");
    assert_eq!(compute_calls.load(Ordering::SeqCst), 1);

    // Second call should return cached value without invoking closure
    let calls = Arc::clone(&compute_calls);
    let val2 = cache.get_or_insert_with("user:1".to_string(), || {
        calls.fetch_add(1, Ordering::SeqCst);
        "Bob".to_string()
    });
    assert_eq!(val2, "Alice");
    assert_eq!(compute_calls.load(Ordering::SeqCst), 1);
}

#[test]
fn test_time_to_live_expiration() {
    let cache: Cache<String, String> = Cache::builder()
        .time_to_live(Duration::from_millis(50))
        .build();

    cache.insert("ephemeral".to_string(), "data".to_string());
    cache.insert_with_ttl(
        "custom_ttl".to_string(),
        "custom".to_string(),
        Duration::from_millis(150),
    );

    // Immediate check
    assert_eq!(cache.get("ephemeral"), Some("data".to_string()));
    assert_eq!(cache.get("custom_ttl"), Some("custom".to_string()));

    // Wait for ephemeral to expire
    thread::sleep(Duration::from_millis(60));
    assert_eq!(cache.get("ephemeral"), None);
    assert_eq!(cache.get("custom_ttl"), Some("custom".to_string()));

    // Wait for custom_ttl to expire
    thread::sleep(Duration::from_millis(100));
    assert_eq!(cache.get("custom_ttl"), None);

    let metrics = cache.metrics();
    assert!(metrics.expirations >= 2);
}

#[test]
fn test_purge_expired() {
    let cache: Cache<String, i32> = Cache::new();

    cache.insert_with_ttl("exp1".to_string(), 1, Duration::from_millis(30));
    cache.insert_with_ttl("exp2".to_string(), 2, Duration::from_millis(30));
    cache.insert_with_ttl("live".to_string(), 3, Duration::from_secs(60));

    assert_eq!(cache.len(), 3);
    thread::sleep(Duration::from_millis(40));

    let purged = cache.purge_expired();
    assert_eq!(purged, 2);
    assert_eq!(cache.len(), 1);
    assert_eq!(cache.get("live"), Some(3));
}

#[test]
fn test_background_sweeper() {
    let cache: Cache<String, i32> = Cache::new();

    cache.insert_with_ttl("key1".to_string(), 10, Duration::from_millis(40));
    cache.insert_with_ttl("key2".to_string(), 20, Duration::from_millis(40));

    // Start background sweeper polling every 20ms
    let _sweeper = cache.start_sweeper(Duration::from_millis(20));

    assert_eq!(cache.len(), 2);
    thread::sleep(Duration::from_millis(120));

    // Background sweeper should have proactively cleaned them up
    assert_eq!(cache.len(), 0);
}

#[test]
fn test_capacity_and_eviction() {
    // 4 shards with total capacity 16 => 4 items per shard
    let cache: Cache<u64, u64> = Cache::builder()
        .max_capacity(16)
        .num_shards(4)
        .eviction_strategy(EvictionStrategy::SampledLRU { sample_size: 4 })
        .build();

    for i in 0..100 {
        cache.insert(i, i * 10);
    }

    // Cache should not exceed max capacity
    assert!(cache.len() <= 16);

    let metrics = cache.metrics();
    assert!(metrics.evictions > 0);
    assert_eq!(metrics.insertions, 100);
}

#[test]
fn test_concurrent_stress_torture() {
    let cache: Arc<Cache<usize, usize>> =
        Arc::new(Cache::builder().max_capacity(50_000).num_shards(32).build());

    let num_threads = 16;
    let ops_per_thread = 2_000;
    let mut handles = Vec::new();

    for thread_id in 0..num_threads {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..ops_per_thread {
                let key = (thread_id * 100 + i) % 1_000;
                match i % 3 {
                    0 => {
                        c.insert(key, i);
                    }
                    1 => {
                        let _ = c.get(&key);
                    }
                    _ => {
                        let _ = c.with_value(&key, |v| *v);
                    }
                }
            }
        }));
    }

    for handle in handles {
        handle.join().unwrap();
    }

    let metrics = cache.metrics();
    assert!(metrics.total_lookups() > 0);
    assert!(metrics.insertions > 0);
    println!(
        "Stress test finished successfully! Telemetry: {}",
        cache.metrics()
    );
}

#[test]
fn test_telemetry_metrics_tracking() {
    let cache: Cache<String, i32> = Cache::new();

    cache.insert("k1".to_string(), 10);
    assert_eq!(cache.get("k1"), Some(10)); // Hit 1
    assert_eq!(cache.get("k1"), Some(10)); // Hit 2
    assert_eq!(cache.get("missing"), None); // Miss 1

    let metrics = cache.metrics();
    assert_eq!(metrics.hits, 2);
    assert_eq!(metrics.misses, 1);
    assert_eq!(metrics.total_lookups(), 3);
    assert!((metrics.hit_ratio() - 2.0 / 3.0).abs() < 0.001);
    assert!((metrics.miss_ratio() - 1.0 / 3.0).abs() < 0.001);

    cache.reset_metrics();
    let reset_m = cache.metrics();
    assert_eq!(reset_m.hits, 0);
    assert_eq!(reset_m.misses, 0);
    assert_eq!(reset_m.total_lookups(), 0);
}

#[test]
fn test_convenience_methods_and_aliases() {
    let cache: aerocache::StringCache<i32> = aerocache::new();

    // set / has / delete / stats
    assert_eq!(cache.set("item_1".to_string(), 42), None);
    assert!(cache.has("item_1"));
    assert_eq!(cache.get("item_1"), Some(42));

    // touch
    assert!(cache.touch("item_1"));
    assert!(!cache.touch("missing_item"));

    // stats
    let stats = cache.stats();
    assert_eq!(stats.hits, 1);

    // keys()
    cache.set("item_2".to_string(), 84);
    let mut keys = cache.keys();
    keys.sort();
    assert_eq!(keys, vec!["item_1".to_string(), "item_2".to_string()]);

    // retain()
    cache.retain(|_k, v| *v > 50);
    assert!(!cache.has("item_1"));
    assert!(cache.has("item_2"));

    // delete
    assert_eq!(cache.delete("item_2"), Some(84));
    assert!(!cache.has("item_2"));

    // constructors
    let bounded = aerocache::Cache::<String, i32>::with_capacity(500);
    assert_eq!(bounded.capacity(), 500);

    let ttl_cache = aerocache::Cache::<String, i32>::with_ttl(Duration::from_secs(10));
    assert_eq!(ttl_cache.capacity(), 0);

    let bounded_ttl =
        aerocache::Cache::<String, i32>::with_capacity_and_ttl(1_000, Duration::from_secs(30));
    assert_eq!(bounded_ttl.capacity(), 1_000);

    // Display & Debug
    let display_str = format!("{cache}");
    assert!(display_str.contains("AeroCache"));
}
