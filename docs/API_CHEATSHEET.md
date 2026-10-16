# 📖 AeroCache API Cheatsheet

A quick reference guide for common methods, constructors, and usage patterns.

---

## 🏗️ Constructors & Creation

| Method | Description | Example |
| :--- | :--- | :--- |
| `Cache::new()` | Default cache (10k items, 64 shards, AHash) | `let c: StringCache<i32> = Cache::new();` |
| `Cache::with_capacity(cap)` | Bounded cache with custom capacity | `let c = Cache::with_capacity(50_000);` |
| `Cache::with_ttl(ttl)` | Unbounded cache with default TTL | `let c = Cache::with_ttl(Duration::from_secs(60));` |
| `Cache::with_capacity_and_ttl(cap, ttl)` | Bounded cache with default TTL | `let c = Cache::with_capacity_and_ttl(10_000, ttl);` |
| `Cache::builder()` | Fluent builder for advanced config | `Cache::builder().num_shards(128).build()` |

---

## 🔍 Reading Data

| Method | Description | Allocation Cost |
| :--- | :--- | :--- |
| `cache.get(&key)` | Returns cloned value `Option<V>` | Clones `V` |
| `cache.with_value(&key, \|v\| ...)` | Inspects value by reference | **Zero heap allocations** |
| `cache.has(&key)` | Checks if key exists & is not expired | None |
| `cache.contains_key(&key)` | Alias for `.has()` | None |
| `cache.keys()` | Returns `Vec<K>` of all active keys | Allocates key vector |

---

## ✍️ Writing & Mutating

| Method | Description |
| :--- | :--- |
| `cache.set(key, val)` | Inserts or updates key; returns old value |
| `cache.insert(key, val)` | Alias for `.set()` |
| `cache.insert_with_ttl(key, val, ttl)` | Inserts key with a custom expiration duration |
| `cache.with_value_mut(&key, \|v\| ...)` | Mutates value in-place under an exclusive write lock |
| `cache.get_or_insert_with(key, \|\| ...)` | Compute-if-absent (read-through caching) |
| `cache.get_or_try_insert_with(key, \|\| ...)` | Fallible compute-if-absent returning `Result<V, E>` |

---

## 🗑️ Deletion & Retention

| Method | Description |
| :--- | :--- |
| `cache.delete(&key)` | Removes key and returns old value |
| `cache.remove(&key)` | Alias for `.delete()` |
| `cache.clear()` | Removes all entries from all shards |
| `cache.purge_expired()` | Immediately purges all expired keys across shards |
| `cache.retain(\|k, v\| ...)` | Retains only entries satisfying the predicate |

---

## ⏱️ TTL & Background Sweeper

```rust
use std::time::Duration;

// Automatically cleans expired keys in the background every 500ms
let sweeper = cache.start_sweeper(Duration::from_millis(500));

// Sweeper terminates automatically when dropped, or call explicitly:
// sweeper.stop();
```

---

## 📊 Telemetry & Observability

```rust
let stats = cache.stats(); // or cache.metrics();

println!("Hits: {}", stats.hits);
println!("Misses: {}", stats.misses);
println!("Hit Ratio: {:.2}%", stats.hit_ratio() * 100.0);
println!("Miss Ratio: {:.2}%", stats.miss_ratio() * 100.0);
println!("Total Lookups: {}", stats.total_lookups());
println!("Evictions: {}", stats.evictions);
println!("Expirations: {}", stats.expirations);

// Reset counters to zero
cache.reset_metrics();
```
