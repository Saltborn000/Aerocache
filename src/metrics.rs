use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// High-throughput atomic telemetry counters for cache operations.
///
/// Uses relaxed and acquire-release atomics to ensure zero lock overhead
/// during hot-path instrumentation.
#[derive(Debug, Default)]
pub struct AtomicMetrics {
    hits: AtomicU64,
    misses: AtomicU64,
    insertions: AtomicU64,
    updates: AtomicU64,
    evictions: AtomicU64,
    expirations: AtomicU64,
}

impl AtomicMetrics {
    /// Creates a new metrics tracker with all counters set to zero.
    #[inline]
    pub const fn new() -> Self {
        Self {
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            insertions: AtomicU64::new(0),
            updates: AtomicU64::new(0),
            evictions: AtomicU64::new(0),
            expirations: AtomicU64::new(0),
        }
    }

    /// Records a cache hit.
    #[inline]
    pub fn record_hit(&self) {
        self.hits.fetch_add(1, Ordering::Relaxed);
    }

    /// Records a cache miss.
    #[inline]
    pub fn record_miss(&self) {
        self.misses.fetch_add(1, Ordering::Relaxed);
    }

    /// Records a key insertion.
    #[inline]
    pub fn record_insertion(&self) {
        self.insertions.fetch_add(1, Ordering::Relaxed);
    }

    /// Records an update of an existing key.
    #[inline]
    pub fn record_update(&self) {
        self.updates.fetch_add(1, Ordering::Relaxed);
    }

    /// Records an eviction due to capacity limits.
    #[inline]
    pub fn record_eviction(&self) {
        self.evictions.fetch_add(1, Ordering::Relaxed);
    }

    /// Records an entry expiration due to TTL.
    #[inline]
    pub fn record_expiration(&self) {
        self.expirations.fetch_add(1, Ordering::Relaxed);
    }

    /// Takes a consistent point-in-time snapshot of the cache metrics.
    pub fn snapshot(&self) -> CacheMetrics {
        CacheMetrics {
            hits: self.hits.load(Ordering::Relaxed),
            misses: self.misses.load(Ordering::Relaxed),
            insertions: self.insertions.load(Ordering::Relaxed),
            updates: self.updates.load(Ordering::Relaxed),
            evictions: self.evictions.load(Ordering::Relaxed),
            expirations: self.expirations.load(Ordering::Relaxed),
        }
    }

    /// Resets all counters to zero.
    pub fn reset(&self) {
        self.hits.store(0, Ordering::Relaxed);
        self.misses.store(0, Ordering::Relaxed);
        self.insertions.store(0, Ordering::Relaxed);
        self.updates.store(0, Ordering::Relaxed);
        self.evictions.store(0, Ordering::Relaxed);
        self.expirations.store(0, Ordering::Relaxed);
    }
}

/// An immutable, point-in-time snapshot of cache performance metrics.
///
/// Can be exported to monitoring systems (e.g. Prometheus, Datadog) or
/// serialized to JSON when the `serde` feature is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct CacheMetrics {
    /// Total number of successful cache hits.
    pub hits: u64,
    /// Total number of cache misses.
    pub misses: u64,
    /// Total number of newly inserted items.
    pub insertions: u64,
    /// Total number of updates to existing keys.
    pub updates: u64,
    /// Total number of items evicted to satisfy capacity limits.
    pub evictions: u64,
    /// Total number of items expired due to Time-To-Live (TTL).
    pub expirations: u64,
}

impl CacheMetrics {
    /// Returns the total number of read lookups (`hits + misses`).
    #[inline]
    pub fn total_lookups(&self) -> u64 {
        self.hits.saturating_add(self.misses)
    }

    /// Returns the cache hit ratio in the range `[0.0, 1.0]`.
    /// Returns `0.0` if no lookups have occurred.
    #[inline]
    pub fn hit_ratio(&self) -> f64 {
        let total = self.total_lookups();
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }

    /// Returns the cache miss ratio in the range `[0.0, 1.0]`.
    /// Returns `0.0` if no lookups have occurred.
    #[inline]
    pub fn miss_ratio(&self) -> f64 {
        let total = self.total_lookups();
        if total == 0 {
            0.0
        } else {
            self.misses as f64 / total as f64
        }
    }
}

impl fmt::Display for CacheMetrics {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "CacheMetrics {{ hits: {}, misses: {}, hit_ratio: {:.2}%, insertions: {}, updates: {}, evictions: {}, expirations: {} }}",
            self.hits,
            self.misses,
            self.hit_ratio() * 100.0,
            self.insertions,
            self.updates,
            self.evictions,
            self.expirations
        )
    }
}
