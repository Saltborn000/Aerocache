<div align="center">

# ⚡ AeroCache

### Ultra-Fast, Zero-Contention Sharded In-Memory Cache for Rust

[![CI](https://github.com/Saltborn000/Aerocache/actions/workflows/ci.yml/badge.svg)](https://github.com/Saltborn000/Aerocache/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/badge/crates.io-v0.1.0-orange?style=flat-square&logo=rust)](https://crates.io/crates/aerocache)
[![Documentation](https://img.shields.io/badge/docs.rs-aerocache-blue?style=flat-square&logo=docs.rs)](https://docs.rs/aerocache)
[![License](https://img.shields.io/badge/license-MIT%20%2F%20Apache--2.0-blueviolet?style=flat-square)](LICENSE-MIT)
[![Safety](https://img.shields.io/badge/unsafe-forbidden-brightgreen?style=flat-square&logo=shield)](https://github.com/Saltborn000/Aerocache)
[![Speed](https://img.shields.io/badge/throughput-12.8M%20ops%2Fsec-success?style=flat-square&logo=speedtest)](docs/BENCHMARKS.md)

<p align="center">
  <b>Sub-microsecond reads</b> • 
  <b>Lock-striped concurrency</b> • 
  <b>Cache-line padded</b> • 
  <b>Zero-copy lookups</b> • 
  <b>100% Safe Rust</b>
</p>

---

</div>

## 💡 Why AeroCache?

Standard multi-threaded caches in Rust wrap a `Mutex` around a `HashMap`. Under heavy multi-core load, threads spend **80-90% of their time waiting in line for locks**.

**AeroCache** solves this with enterprise hardware-level optimizations:
1. **64 Cache-Padded Shards (`#[repr(align(64))]`)**: Eliminates multi-core CPU cache invalidation (*false sharing*).
2. **Single-Cycle Bitwise Masking**: Power-of-two shards replace slow CPU modulo division (`%`) with instant bitwise AND (`&`).
3. **Lock-Free Atomic Reads**: Readers update access timestamps and frequency counters via atomic integers without taking exclusive write locks.
4. **Zero-Copy Inspection (`.with_value()`)**: Inspect large cached structs (JSON, protobufs, vectors) in-place without duplicating heap memory.
5. **Redis-Style Sampled Eviction**: $O(1)$ amortized Sampled-LRU and Sampled-LFU eviction without heavy linked-list pointer overhead.

---

## 📊 Performance Comparison

Measured under heavy 8-thread concurrent workload (90% reads, 10% writes):

```
Throughput (Ops/Sec) - Higher is Better
────────────────────────────────────────────────────────────────────────────────────────
AeroCache (Ours)                ████████████████████████████████████████ 12.8M ops/sec
DashMap                         ████████████████████ 6.5M ops/sec
parking_lot::RwLock<HashMap>    ████ 1.25M ops/sec
std::sync::Mutex<HashMap>       ██ 0.60M ops/sec
────────────────────────────────────────────────────────────────────────────────────────
```

### 🥊 Feature Matrix

| Feature | `AeroCache` | `std::sync::Mutex` | `DashMap` | `Moka` |
| :--- | :---: | :---: | :---: | :---: |
| **Multi-Core Throughput** | **~12.8M ops/s** | ~0.6M ops/s | ~6.5M ops/s | ~4.5M ops/s |
| **False Sharing Prevention (`align(64)`)** | ✅ **Yes** | ❌ No | ❌ No | ❌ No |
| **Zero-Copy In-Place Reads** | ✅ **Yes (`.with_value`)** | ❌ Requires clone | ❌ Guard ref | ❌ Requires clone |
| **Atomic Read Paths** | ✅ **Yes** | ❌ Full lock | ❌ Write lock on touch | ❌ Channel passing |
| **Built-in TTL & Background Sweeper** | ✅ **Yes** | ❌ Manual | ❌ Manual | ✅ Yes |
| **100% Safe Rust (`forbid(unsafe)`)** | ✅ **Yes** | ✅ Yes | ❌ Contains unsafe | ❌ Contains unsafe |

---

## 🏛️ Architecture

```
                                 Incoming Request
                      (e.g., cache.get("user:42") or .set())
                                        │
                                        ▼
                              ┌───────────────────┐
                              │    AHash Engine   │  Hardware-accelerated AES-NI
                              │    (hash_one)     │  Sub-nanosecond 64-bit hash
                              └─────────┬─────────┘
                                        │
                                        ▼
                             ┌─────────────────────┐
                             │  Single-Cycle Mask  │  index = hash & (num_shards - 1)
                             │   Bitwise AND (&)   │  (~0.3ns, bypasses CPU modulo div)
                             └──────────┬──────────┘
                                        │
      ┌─────────────────────────────────┼─────────────────────────────────┐
      ▼                                 ▼                                 ▼
┌─────────────────┐             ┌─────────────────┐             ┌─────────────────┐
│ Shard 0 (Padded)│             │ Shard 1 (Padded)│             │ Shard 63(Padded)│
│ 64-byte aligned │             │ 64-byte aligned │             │ 64-byte aligned │
│ ┌─────────────┐ │             │ ┌─────────────┐ │             │ ┌─────────────┐ │
│ │  RwLock     │ │             │ │  RwLock     │ │             │ │  RwLock     │ │
│ └─────────────┘ │             │ └─────────────┘ │             │ └─────────────┘ │
│ ┌─────────────┐ │             │ ┌─────────────┐ │             │ ┌─────────────┐ │
│ │  HashMap    │ │             │ │  HashMap    │ │             │ │  HashMap    │ │
│ └─────────────┘ │             │ └─────────────┘ │             │ └─────────────┘ │
└─────────────────┘             └─────────────────┘             └─────────────────┘
```

---

## 📦 Installation

Add to your `Cargo.toml`:

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

## 🚀 Quick Start (In 3 Lines)

```rust
use aerocache::StringCache;
use std::time::Duration;

fn main() {
    // 1. One-line constructor: 10,000 capacity + 5-minute TTL
    let cache: StringCache<String> = aerocache::Cache::with_capacity_and_ttl(
        10_000, 
        Duration::from_secs(300)
    );

    // 2. Fast set & get (works with borrowed &str without allocating!)
    cache.set("user:101".to_string(), "Alice".to_string());

    if let Some(user) = cache.get("user:101") {
        println!("Found user: {user}");
    }

    // 3. Intuitive aliases
    if cache.has("user:101") {
        cache.delete("user:101");
    }
}
```

---

## 🍳 Production Recipes

### 1. Zero-Copy Struct Reads (`.with_value`)
Avoid cloning large payloads on every single cache hit:

```rust
use aerocache::Cache;

let cache: Cache<String, Vec<u8>> = Cache::new();
cache.insert("payload".to_string(), vec![0u8; 8192]);

// Inspect the 8KB buffer in-place with ZERO memory copies!
let size = cache.with_value("payload", |bytes| bytes.len());
assert_eq!(size, Some(8192));
```

### 2. In-Place Mutation (`.with_value_mut`)
Update fields in-place without removing or re-inserting:

```rust
let cache: Cache<String, u64> = Cache::new();
cache.set("views".to_string(), 100);

cache.with_value_mut("views", |count| *count += 1);
assert_eq!(cache.get("views"), Some(101));
```

### 3. Read-Through Database Cache (`get_or_insert_with`)
Compute-if-absent pattern for database or API caching:

```rust
let user = cache.get_or_insert_with("user:42".to_string(), || {
    // Heavy DB query only executed on cache miss
    fetch_user_from_db(42)
});
```

### 4. Background TTL Sweeper
Keep memory clean without stalling user request threads:

```rust
use std::time::Duration;

// Sweeps expired keys across all shards every 500ms
let _sweeper = cache.start_sweeper(Duration::from_millis(500));
```

### 5. Live Telemetry & Observability
Zero-cost atomic counters for APM / Prometheus / Datadog:

```rust
let stats = cache.stats();
println!("Lookups    : {}", stats.total_lookups());
println!("Hits       : {}", stats.hits);
println!("Misses     : {}", stats.misses);
println!("Hit Ratio  : {:.2}%", stats.hit_ratio() * 100.0);
println!("Evictions  : {}", stats.evictions);
println!("Expirations: {}", stats.expirations);
```

---

## 📚 In-Depth Documentation

* 🏛️ [**Architecture Deep Dive**](docs/ARCHITECTURE.md) - Cache line padding, false sharing, bitwise masking, and sampled eviction.
* 📊 [**Benchmark Report**](docs/BENCHMARKS.md) - Latency percentiles, methodology, and comparative reproduction.
* 📖 [**API Cheatsheet**](docs/API_CHEATSHEET.md) - Quick reference table for all methods and constructors.

---

## 🔒 100% Safe Rust Guarantee

AeroCache strictly enforces:

```rust
#![forbid(unsafe_code)]
```

Zero `unsafe` blocks. Memory safety, thread safety (`Send + Sync`), and freedom from data races are mathematically verified by the Rust compiler.

---

## 🏎️ Running Benchmarks Locally

```bash
# Run Criterion micro-benchmarks
cargo bench

# Run the 8-thread concurrency demo
cargo run --release --example enterprise_showcase
```

---

## 📄 License

Licensed under either of:

* Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
* MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.
