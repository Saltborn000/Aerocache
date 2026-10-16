//! # AeroCache
//!
//! `aerocache` is an ultra-fast, concurrent, sharded in-memory cache engineered for
//! latency-sensitive, high-throughput enterprise systems.
//!
//! ## Key Architecture Highlights
//!
//! - **Zero Lock Contention**: Lock-striped across power-of-two shards with single-cycle bitwise masking.
//! - **Cache-Line Padding (`#[repr(align(64))]`)**: Eliminates false sharing across CPU cores under heavy concurrent writes.
//! - **Zero-Copy Borrowed Lookups**: Look up `&str` on `String` keys without allocating. Inspect values with `.with_value()` without cloning large structs.
//! - **Atomic Read Paths**: Readers run concurrently without taking exclusive locks for timestamp updates.
//! - **Time-To-Live (TTL)**: Precise per-entry or default cache TTL with lazy and active background sweeping.
//! - **Sampled Eviction**: Redis-style Sampled-LRU and Sampled-LFU eviction achieving high hit ratios with $O(1)$ amortized overhead.
//! - **Enterprise Telemetry**: Lock-free atomic hit/miss/eviction counters with zero performance penalty.
//! - **100% Safe Rust**: `#![forbid(unsafe_code)]` for auditability and memory safety.
//!
//! ## Quick Start
//!
//! ```rust
//! use aerocache::Cache;
//! use std::time::Duration;
//!
//! // Create a cache with 100,000 max items and 5-minute TTL
//! let cache: Cache<String, String> = Cache::builder()
//!     .max_capacity(100_000)
//!     .num_shards(64)
//!     .time_to_live(Duration::from_secs(300))
//!     .build();
//!
//! // Insert
//! cache.insert("user:1001".to_string(), "Alice".to_string());
//!
//! // Zero-allocation lookup with borrowed &str key
//! if let Some(name) = cache.get("user:1001") {
//!     println!("Found user: {}", name);
//! }
//!
//! // Check metrics
//! let stats = cache.metrics();
//! println!("Hits: {}, Hit Ratio: {:.2}%", stats.hits, stats.hit_ratio() * 100.0);
//! ```
//!
//! ## Zero-Copy Value Inspection
//!
//! In high-throughput systems, cloning large cached objects on every read is costly.
//! Use [`Cache::with_value`] to inspect the cached value by reference:
//!
//! ```rust
//! use aerocache::Cache;
//!
//! let cache: Cache<String, Vec<u8>> = Cache::new();
//! cache.insert("payload".to_string(), vec![0u8; 1024]);
//!
//! // Access without cloning the 1KB vector!
//! let size = cache.with_value("payload", |bytes| bytes.len());
//! assert_eq!(size, Some(1024));
//! ```

#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// Fluent builder for constructing customized cache instances.
pub mod builder;
/// Core concurrent cache engine and public API.
pub mod cache;
/// Cache entry primitives and atomic timestamp metadata.
pub mod entry;
/// Eviction strategies including Sampled-LRU and Sampled-LFU.
pub mod eviction;
/// High-throughput atomic telemetry and performance snapshot metrics.
pub mod metrics;
/// Cache-line padded partitions and concurrent shard logic.
pub mod shard;
/// Background sweeper thread for proactive TTL expiration pruning.
pub mod sweeper;

pub use builder::CacheBuilder;
pub use cache::Cache;
pub use eviction::EvictionStrategy;
pub use metrics::CacheMetrics;
pub use sweeper::SweeperHandle;

/// Type alias for the common case where keys are `String`.
pub type StringCache<V> = Cache<String, V>;

/// Convenience constructor for creating a default [`Cache`].
pub fn new<K: Eq + std::hash::Hash, V>() -> Cache<K, V> {
    Cache::new()
}
