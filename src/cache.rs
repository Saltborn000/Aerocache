use std::borrow::Borrow;
use std::hash::{BuildHasher, Hash};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::builder::CacheBuilder;
use crate::metrics::{AtomicMetrics, CacheMetrics};
use crate::shard::{CachePadded, Shard};
use crate::sweeper::SweeperHandle;

/// Shared internal structure for `Cache`.
pub(crate) struct CacheShared<K, V, S> {
    pub(crate) shards: Box<[CachePadded<Shard<K, V, S>>]>,
    pub(crate) shard_mask: usize,
    pub(crate) total_capacity: usize,
    pub(crate) hasher: S,
    pub(crate) default_ttl: Option<Duration>,
    pub(crate) epoch: Instant,
    pub(crate) metrics: AtomicMetrics,
}

/// An ultra-fast, concurrent, sharded in-memory cache.
///
/// Designed for low-latency, high-concurrency enterprise workloads.
/// Features zero lock contention across cores via cache-line padded shards,
/// sub-microsecond zero-copy lookups, configurable TTL, and atomic telemetry.
///
/// Cloning `Cache` is an `O(1)` atomic reference-count increment, making it
/// trivial to share across thread pools and async runtime tasks.
pub struct Cache<K, V, S = ahash::RandomState> {
    pub(crate) inner: Arc<CacheShared<K, V, S>>,
}

impl<K, V, S> Clone for Cache<K, V, S> {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<K, V> Cache<K, V, ahash::RandomState>
where
    K: Eq + Hash,
{
    /// Creates a new cache with default settings (10,000 capacity, 64 shards, AHash).
    pub fn new() -> Self {
        Self::builder().build()
    }

    /// Creates a cache bounded by a custom maximum capacity.
    ///
    /// # Example
    /// ```rust
    /// use aerocache::Cache;
    /// let cache: Cache<String, i32> = Cache::with_capacity(50_000);
    /// ```
    pub fn with_capacity(capacity: usize) -> Self {
        Self::builder().max_capacity(capacity).build()
    }

    /// Creates an unbounded cache with a default Time-To-Live (TTL).
    ///
    /// # Example
    /// ```rust
    /// use aerocache::Cache;
    /// use std::time::Duration;
    /// let cache: Cache<String, i32> = Cache::with_ttl(Duration::from_secs(60));
    /// ```
    pub fn with_ttl(ttl: Duration) -> Self {
        Self::builder().max_capacity(0).time_to_live(ttl).build()
    }

    /// Creates a bounded cache with a default Time-To-Live (TTL).
    ///
    /// # Example
    /// ```rust
    /// use aerocache::Cache;
    /// use std::time::Duration;
    /// let cache: Cache<String, i32> = Cache::with_capacity_and_ttl(10_000, Duration::from_secs(60));
    /// ```
    pub fn with_capacity_and_ttl(capacity: usize, ttl: Duration) -> Self {
        Self::builder()
            .max_capacity(capacity)
            .time_to_live(ttl)
            .build()
    }

    /// Returns a fluent builder to configure and construct an `AeroCache`.
    pub fn builder() -> CacheBuilder<K, V, ahash::RandomState> {
        CacheBuilder::new()
    }
}

impl<K, V> Default for Cache<K, V, ahash::RandomState>
where
    K: Eq + Hash,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<K, V, S> Cache<K, V, S>
where
    K: Eq + Hash,
    S: BuildHasher + Clone,
{
    #[inline]
    fn now_millis(&self) -> u64 {
        self.inner.epoch.elapsed().as_millis() as u64
    }

    #[inline]
    fn hash_key<Q>(&self, key: &Q) -> u64
    where
        Q: Hash + ?Sized,
    {
        self.inner.hasher.hash_one(key)
    }

    #[inline]
    fn select_shard<Q>(&self, key: &Q) -> &Shard<K, V, S>
    where
        Q: Hash + ?Sized,
    {
        let hash = self.hash_key(key);
        let index = (hash as usize) & self.inner.shard_mask;
        &self.inner.shards[index]
    }

    /// Retrieves a cloned value for the given key, if present and not expired.
    ///
    /// # Zero-Copy Alternative
    /// If `V` is expensive to clone, prefer [`Cache::with_value`] which borrows
    /// the value directly without allocation.
    pub fn get<Q>(&self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
        V: Clone,
    {
        let shard = self.select_shard(key);
        shard.get(key, self.now_millis(), &self.inner.metrics)
    }

    /// Accesses the cached value by reference without cloning, passing it to `f`.
    ///
    /// This is an ultra-fast zero-copy operation that only acquires a read lock.
    ///
    /// # Example
    /// ```rust
    /// use aerocache::Cache;
    ///
    /// let cache: Cache<String, Vec<u8>> = Cache::new();
    /// cache.insert("data".to_string(), vec![1, 2, 3, 4, 5]);
    ///
    /// let length = cache.with_value("data", |bytes| bytes.len());
    /// assert_eq!(length, Some(5));
    /// ```
    pub fn with_value<Q, R>(&self, key: &Q, f: impl FnOnce(&V) -> R) -> Option<R>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let shard = self.select_shard(key);
        shard.with_value(key, self.now_millis(), &self.inner.metrics, f)
    }

    /// Mutates the cached value in-place under an exclusive write lock.
    ///
    /// # Example
    /// ```rust
    /// use aerocache::Cache;
    ///
    /// let cache: Cache<String, u64> = Cache::new();
    /// cache.insert("counter".to_string(), 10);
    ///
    /// cache.with_value_mut("counter", |val| *val += 5);
    /// assert_eq!(cache.get("counter"), Some(15));
    /// ```
    pub fn with_value_mut<Q, R>(&self, key: &Q, f: impl FnOnce(&mut V) -> R) -> Option<R>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let shard = self.select_shard(key);
        shard.with_value_mut(key, self.now_millis(), &self.inner.metrics, f)
    }

    /// Checks if a key exists and has not expired.
    pub fn contains_key<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let shard = self.select_shard(key);
        shard.contains_key(key, self.now_millis())
    }

    /// Inserts a key-value pair using the default cache TTL (if configured).
    ///
    /// Returns the previously stored value, if one existed.
    pub fn insert(&self, key: K, value: V) -> Option<V>
    where
        K: Clone,
    {
        let ttl = self.inner.default_ttl;
        let shard = self.select_shard(&key);
        shard.insert(key, value, ttl, self.now_millis(), &self.inner.metrics)
    }

    /// Inserts a key-value pair with a custom Time-To-Live.
    ///
    /// Returns the previously stored value, if one existed.
    pub fn insert_with_ttl(&self, key: K, value: V, ttl: Duration) -> Option<V>
    where
        K: Clone,
    {
        let shard = self.select_shard(&key);
        shard.insert(
            key,
            value,
            Some(ttl),
            self.now_millis(),
            &self.inner.metrics,
        )
    }

    /// Retrieves an entry or computes and inserts it if missing.
    ///
    /// Useful for compute-if-absent / read-through caching.
    pub fn get_or_insert_with<F>(&self, key: K, f: F) -> V
    where
        K: Clone,
        V: Clone,
        F: FnOnce() -> V,
    {
        if let Some(val) = self.get(&key) {
            return val;
        }
        let val = f();
        self.insert(key, val.clone());
        val
    }

    /// Retrieves an entry or computes and inserts it if missing, supporting fallible closures.
    pub fn get_or_try_insert_with<F, E>(&self, key: K, f: F) -> Result<V, E>
    where
        K: Clone,
        V: Clone,
        F: FnOnce() -> Result<V, E>,
    {
        if let Some(val) = self.get(&key) {
            return Ok(val);
        }
        let val = f()?;
        self.insert(key, val.clone());
        Ok(val)
    }

    /// Removes an entry by key, returning the value if present.
    pub fn remove<Q>(&self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let shard = self.select_shard(key);
        shard.remove(key)
    }

    /// Purges all expired entries across all shards immediately.
    ///
    /// Returns the total number of expired entries removed.
    pub fn purge_expired(&self) -> usize {
        let now = self.now_millis();
        let mut total = 0;
        for shard in self.inner.shards.iter() {
            total += shard.purge_expired(now, &self.inner.metrics);
        }
        total
    }

    /// Starts a background thread that periodically purges expired keys.
    ///
    /// The returned `SweeperHandle` terminates the sweeper thread when dropped.
    pub fn start_sweeper(&self, interval: Duration) -> SweeperHandle
    where
        K: 'static + Send + Sync + Clone,
        V: 'static + Send + Sync,
        S: 'static + Send + Sync,
    {
        let cache_clone = self.clone();
        SweeperHandle::start(interval, move || cache_clone.purge_expired())
    }

    /// Returns the total number of items currently stored across all shards.
    pub fn len(&self) -> usize {
        self.inner.shards.iter().map(|shard| shard.len()).sum()
    }

    /// Returns true if the cache contains no items.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Clears all entries from all shards.
    pub fn clear(&self) {
        for shard in self.inner.shards.iter() {
            shard.clear();
        }
    }

    /// Returns the maximum capacity configured for this cache.
    pub fn capacity(&self) -> usize {
        self.inner.total_capacity
    }

    /// Returns the number of internal partitions (shards).
    pub fn num_shards(&self) -> usize {
        self.inner.shards.len()
    }

    /// Convenience alias for [`Cache::insert`].
    ///
    /// Inserts a key-value pair and returns the previous value if it existed.
    #[inline]
    pub fn set(&self, key: K, value: V) -> Option<V>
    where
        K: Clone,
    {
        self.insert(key, value)
    }

    /// Convenience alias for [`Cache::remove`].
    ///
    /// Removes an entry by key and returns the value if it existed.
    #[inline]
    pub fn delete<Q>(&self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.remove(key)
    }

    /// Convenience alias for [`Cache::contains_key`].
    ///
    /// Returns true if the key exists and has not expired.
    #[inline]
    pub fn has<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.contains_key(key)
    }

    /// Convenience alias for [`Cache::metrics`].
    ///
    /// Returns a point-in-time snapshot of telemetry metrics.
    #[inline]
    pub fn stats(&self) -> CacheMetrics {
        self.metrics()
    }

    /// Refreshes the last-accessed timestamp of an entry without modifying its value.
    ///
    /// Returns true if the key existed and was not expired.
    pub fn touch<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let shard = self.select_shard(key);
        shard.touch(key, self.now_millis())
    }

    /// Returns a vector containing all non-expired keys currently in the cache.
    pub fn keys(&self) -> Vec<K>
    where
        K: Clone,
    {
        let now = self.now_millis();
        let mut all_keys = Vec::with_capacity(self.len());
        for shard in self.inner.shards.iter() {
            shard.collect_keys(now, &mut all_keys);
        }
        all_keys
    }

    /// Retains only the entries that satisfy the predicate `f`.
    ///
    /// Entries for which `f(&key, &val)` returns false will be removed.
    pub fn retain<F>(&self, mut f: F)
    where
        F: FnMut(&K, &V) -> bool,
    {
        let now = self.now_millis();
        for shard in self.inner.shards.iter() {
            shard.retain(now, &mut f);
        }
    }

    /// Returns a point-in-time snapshot of telemetry metrics (hits, misses, evictions, etc.).
    pub fn metrics(&self) -> CacheMetrics {
        self.inner.metrics.snapshot()
    }

    /// Resets all telemetry counters to zero.
    pub fn reset_metrics(&self) {
        self.inner.metrics.reset();
    }
}

impl<K, V, S> std::fmt::Display for Cache<K, V, S>
where
    K: Eq + Hash,
    S: BuildHasher + Clone,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "AeroCache(items: {}, capacity: {}, shards: {})",
            self.len(),
            self.capacity(),
            self.num_shards()
        )
    }
}

impl<K, V, S> std::fmt::Debug for Cache<K, V, S>
where
    K: Eq + Hash,
    S: BuildHasher + Clone,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AeroCache")
            .field("items", &self.len())
            .field("capacity", &self.capacity())
            .field("shards", &self.num_shards())
            .field("metrics", &self.metrics())
            .finish()
    }
}
