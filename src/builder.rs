use std::hash::BuildHasher;
use std::marker::PhantomData;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::cache::{Cache, CacheShared};
use crate::eviction::EvictionStrategy;
use crate::metrics::AtomicMetrics;
use crate::shard::{CachePadded, Shard};

/// Fluent builder for constructing a high-performance `Cache`.
#[derive(Debug, Clone)]
pub struct CacheBuilder<K, V, S = ahash::RandomState> {
    max_capacity: usize,
    num_shards: usize,
    time_to_live: Option<Duration>,
    eviction_strategy: EvictionStrategy,
    hasher: S,
    _marker: PhantomData<(K, V)>,
}

impl<K, V> CacheBuilder<K, V, ahash::RandomState> {
    /// Creates a new builder with production-grade default settings:
    /// - Max capacity: 10,000 entries
    /// - Shards: 64 (power-of-2 aligned)
    /// - Hasher: AHash (hardware accelerated AES-NI / ultra-low latency)
    /// - Eviction: Sampled LRU (sample size 8)
    pub fn new() -> Self {
        Self {
            max_capacity: 10_000,
            num_shards: 64,
            time_to_live: None,
            eviction_strategy: EvictionStrategy::default(),
            hasher: ahash::RandomState::new(),
            _marker: PhantomData,
        }
    }
}

impl<K, V> Default for CacheBuilder<K, V, ahash::RandomState> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K, V, S> CacheBuilder<K, V, S> {
    /// Sets the total maximum capacity across all shards.
    ///
    /// Pass `0` to allow unbounded growth. Default is `10_000`.
    #[must_use]
    pub fn max_capacity(mut self, capacity: usize) -> Self {
        self.max_capacity = capacity;
        self
    }

    /// Sets the number of internal partitions (shards).
    ///
    /// The value will automatically be rounded up to the nearest power of 2
    /// to enable single-cycle bitwise AND shard hashing (`hash & (shards - 1)`).
    /// Default is `64`.
    #[must_use]
    pub fn num_shards(mut self, shards: usize) -> Self {
        self.num_shards = shards.max(1).next_power_of_two();
        self
    }

    /// Sets a default Time-To-Live (TTL) for all inserted entries.
    ///
    /// Entries older than this duration will be automatically expired.
    #[must_use]
    pub fn time_to_live(mut self, ttl: Duration) -> Self {
        self.time_to_live = Some(ttl);
        self
    }

    /// Configures the eviction strategy when a shard reaches capacity.
    ///
    /// Default is [`EvictionStrategy::SampledLRU`] with a sample size of 8.
    #[must_use]
    pub fn eviction_strategy(mut self, strategy: EvictionStrategy) -> Self {
        self.eviction_strategy = strategy;
        self
    }

    /// Configures a custom hash builder for key hashing.
    #[must_use]
    pub fn hasher<CustomHasher>(self, hasher: CustomHasher) -> CacheBuilder<K, V, CustomHasher> {
        CacheBuilder {
            max_capacity: self.max_capacity,
            num_shards: self.num_shards,
            time_to_live: self.time_to_live,
            eviction_strategy: self.eviction_strategy,
            hasher,
            _marker: PhantomData,
        }
    }

    /// Builds and returns the configured [`Cache`].
    pub fn build(self) -> Cache<K, V, S>
    where
        K: Eq + std::hash::Hash,
        S: BuildHasher + Clone,
    {
        let num_shards = self.num_shards.max(1).next_power_of_two();
        let shard_capacity = if self.max_capacity == 0 {
            0
        } else {
            self.max_capacity.div_ceil(num_shards)
        };

        let mut shards = Vec::with_capacity(num_shards);
        for _ in 0..num_shards {
            let shard = Shard::new(shard_capacity, self.eviction_strategy, self.hasher.clone());
            shards.push(CachePadded::new(shard));
        }

        let shared = CacheShared {
            shards: shards.into_boxed_slice(),
            shard_mask: num_shards - 1,
            total_capacity: self.max_capacity,
            hasher: self.hasher,
            default_ttl: self.time_to_live,
            epoch: Instant::now(),
            metrics: AtomicMetrics::new(),
        };

        Cache {
            inner: Arc::new(shared),
        }
    }
}
