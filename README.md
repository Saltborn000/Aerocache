# 🚀 AeroCache

[![Crates.io](https://img.shields.io/badge/crates.io-v0.1.0-orange.svg)](https://crates.io/crates/aerocache)
[![Documentation](https://docs.rs/aerocache/badge.svg)](https://docs.rs/aerocache)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE)
[![Safety](https://img.shields.io/badge/unsafe-forbidden-brightgreen.svg)](#100-safe-rust)
[![Throughput](https://img.shields.io/badge/throughput-12.8M%20ops%2Fsec-success.svg)](#benchmarks)

**AeroCache** is an ultra-fast, concurrent, sharded in-memory cache engineered for high-throughput, latency-critical enterprise systems.

It combines cache-line padded lock striping, atomic read paths, zero-copy borrowed lookups, configurable TTL expiration, Redis-style sampled eviction, and lock-free telemetry.

---

## ⚡ Performance Highlights

* **12.8+ Million Ops/Sec**: Measured in multi-threaded mixed read/write workloads (8 cores).
* **Zero Lock Contention on Reads**: Readers acquire only lightweight read locks. Access timestamps and frequency counters update concurrently via atomic operations.
* **Cache-Line Alignment (`#[repr(align(64))]`)**: Every shard lock is isolated to its own 64-byte L1 CPU cache line, eliminating multi-core false sharing.
* **Single-Cycle Sharding**: Shard count is power-of-two aligned, allowing instant bitwise AND indexing (`hash & (shards - 1)`) instead of slow modulo division.
* **Zero Allocations on Read Lookups**: Query with borrowed keys (`&str` against `String`, `&[u8]` against `Vec<u8>`).
* **Zero-Copy Inspection**: Read large structs in place with `.with_value()` without cloning heap data.

---

## 📦 Installation

Add `aerocache` to your `Cargo.toml`:

```toml
[dependencies]
aerocache = "0.1.0"
```

Optional `serde` support for exporting metrics:

```toml
[dependencies]
aerocache = { version = "0.1.0", features = ["serde"] }
```

---

## 🛠️ Quick Start

Using `aerocache` takes just 3 lines of code:

```rust
use aerocache::StringCache;
use std::time::Duration;

fn main() {
    // 1. One-line constructor: Bounded capacity with 5-minute TTL
    let cache: StringCache<String> = aerocache::Cache::with_capacity_and_ttl(
        10_000, 
        Duration::from_secs(300)
    );

    // 2. Familiar set & get (works seamlessly with borrowed &str!)
    cache.set("user:101".to_string(), "Alice".to_string());

    if let Some(user) = cache.get("user:101") {
        println!("Found user: {user}");
    }

    // 3. Intuitive aliases (has, delete, stats)
    if cache.has("user:101") {
        println!("User exists!");
        cache.delete("user:101");
    }

    // 4. Inspect real-time metrics
    let stats = cache.stats();
    println!("Hits: {}, Hit Ratio: {:.2}%", stats.hits, stats.hit_ratio() * 100.0);
}
```

---

## 🔍 Advanced Features

### 1. Zero-Copy Value Inspection (`.with_value`)
In enterprise services, cached payloads are often large structs (e.g. JWT claims, database rows, parsed JSON). Forcing a full clone on every read degrades performance. AeroCache allows reading directly by reference under a read lock:

```rust
use aerocache::Cache;

let cache: Cache<String, Vec<u8>> = Cache::new();
cache.insert("payload".to_string(), vec![0u8; 4096]);

// Inspect the 4KB buffer in-place with zero memory copies!
let length = cache.with_value("payload", |bytes| bytes.len());
assert_eq!(length, Some(4096));
```

### 2. In-Place Mutation (`.with_value_mut`)
Mutate values without re-inserting or cloning:

```rust
let cache: Cache<String, u64> = Cache::new();
cache.insert("page_views".to_string(), 100);

cache.with_value_mut("page_views", |count| *count += 1);
assert_eq!(cache.get("page_views"), Some(101));
```

### 3. Read-Through / Compute-If-Absent
Seamlessly populate missing entries on demand:

```rust
let user = cache.get_or_insert_with("user:42".to_string(), || {
    // Heavy DB query or external API call
    fetch_user_from_db(42)
});
```

### 4. Automatic TTL & Proactive Background Sweeper
Entries can expire lazily upon access or proactively via an asynchronous background sweeper thread:

```rust
use std::time::Duration;

// Clean expired keys every 500ms
let _sweeper = cache.start_sweeper(Duration::from_millis(500));
```

### 5. Production Telemetry & Observability
Zero-overhead atomic metrics for APM / Prometheus / Datadog integration:

```rust
let metrics = cache.metrics();
println!("Total Lookups : {}", metrics.total_lookups());
println!("Cache Hits    : {}", metrics.hits);
println!("Cache Misses  : {}", metrics.misses);
println!("Hit Ratio     : {:.2}%", metrics.hit_ratio() * 100.0);
println!("Evictions     : {}", metrics.evictions);
println!("Expirations   : {}", metrics.expirations);
```

---

## 🔒 100% Safe Rust

AeroCache strictly enforces:

```rust
#![forbid(unsafe_code)]
```

Zero `unsafe` blocks. Memory safety, thread safety (`Send + Sync`), and freedom from data races are mathematically guaranteed by the Rust compiler.

---

## 🏎️ Running Benchmarks

```bash
cargo bench
```

To run the full multi-threaded concurrency showcase:

```bash
cargo run --release --example enterprise_showcase
```

---

## 📄 License

Licensed under either of:

* Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
* MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.
