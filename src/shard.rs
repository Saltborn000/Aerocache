use std::borrow::Borrow;
use std::collections::HashMap;
use std::hash::{BuildHasher, Hash};
use std::ops::{Deref, DerefMut};
use std::time::Duration;

use parking_lot::RwLock;

use crate::entry::CacheEntry;
use crate::eviction::EvictionStrategy;
use crate::metrics::AtomicMetrics;

/// Wrapper that aligns the wrapped type to a 64-byte boundary.
///
/// Prevents false sharing across CPU cores when multiple threads access
/// adjacent shards simultaneously.
#[repr(align(64))]
pub struct CachePadded<T> {
    value: T,
}

impl<T> CachePadded<T> {
    /// Constructs a new cache-padded value.
    pub const fn new(value: T) -> Self {
        Self { value }
    }
}

impl<T> Deref for CachePadded<T> {
    type Target = T;
    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T> DerefMut for CachePadded<T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}

/// Internal data structure protected by each shard's lock.
pub struct ShardInner<K, V, S> {
    pub(crate) map: HashMap<K, CacheEntry<V>, S>,
    pub(crate) capacity: usize,
    pub(crate) eviction_strategy: EvictionStrategy,
}

/// A single cache partition (shard).
pub struct Shard<K, V, S> {
    inner: RwLock<ShardInner<K, V, S>>,
}

impl<K, V, S> Shard<K, V, S>
where
    K: Eq + Hash,
    S: BuildHasher + Clone,
{
    /// Creates a new shard with the given capacity, eviction strategy, and hasher.
    pub fn new(capacity: usize, eviction_strategy: EvictionStrategy, hasher: S) -> Self {
        Self {
            inner: RwLock::new(ShardInner {
                map: HashMap::with_capacity_and_hasher(capacity.min(1024), hasher),
                capacity,
                eviction_strategy,
            }),
        }
    }

    /// Retrieves a cloned copy of the value associated with the key.
    pub fn get<Q>(&self, key: &Q, now_millis: u64, metrics: &AtomicMetrics) -> Option<V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
        V: Clone,
    {
        // 1. Read-lock phase
        {
            let guard = self.inner.read();
            if let Some(entry) = guard.map.get(key) {
                if !entry.is_expired(now_millis) {
                    entry.touch(now_millis);
                    metrics.record_hit();
                    return Some(entry.value.clone());
                }
            } else {
                metrics.record_miss();
                return None;
            }
        }

        // 2. Expired entry found, upgrade to write lock to prune
        let mut guard = self.inner.write();
        if let Some(entry) = guard.map.get(key) {
            if entry.is_expired(now_millis) {
                guard.map.remove(key);
                metrics.record_expiration();
                metrics.record_miss();
                return None;
            } else {
                entry.touch(now_millis);
                metrics.record_hit();
                return Some(entry.value.clone());
            }
        }

        metrics.record_miss();
        None
    }

    /// Inspects the value by reference without cloning, passing it to closure `f`.
    pub fn with_value<Q, R>(
        &self,
        key: &Q,
        now_millis: u64,
        metrics: &AtomicMetrics,
        f: impl FnOnce(&V) -> R,
    ) -> Option<R>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        // Read-lock phase
        {
            let guard = self.inner.read();
            if let Some(entry) = guard.map.get(key) {
                if !entry.is_expired(now_millis) {
                    entry.touch(now_millis);
                    metrics.record_hit();
                    return Some(f(&entry.value));
                }
            } else {
                metrics.record_miss();
                return None;
            }
        }

        // Expired cleanup phase
        let mut guard = self.inner.write();
        if let Some(entry) = guard.map.get(key) {
            if entry.is_expired(now_millis) {
                guard.map.remove(key);
                metrics.record_expiration();
                metrics.record_miss();
                return None;
            } else {
                entry.touch(now_millis);
                metrics.record_hit();
                return Some(f(&entry.value));
            }
        }

        metrics.record_miss();
        None
    }

    /// Mutates the value in-place via closure `f` under a write lock.
    pub fn with_value_mut<Q, R>(
        &self,
        key: &Q,
        now_millis: u64,
        metrics: &AtomicMetrics,
        f: impl FnOnce(&mut V) -> R,
    ) -> Option<R>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let mut guard = self.inner.write();
        if let Some(entry) = guard.map.get_mut(key) {
            if entry.is_expired(now_millis) {
                guard.map.remove(key);
                metrics.record_expiration();
                metrics.record_miss();
                None
            } else {
                entry.touch(now_millis);
                metrics.record_hit();
                Some(f(&mut entry.value))
            }
        } else {
            metrics.record_miss();
            None
        }
    }

    /// Checks if a non-expired entry exists for the key.
    pub fn contains_key<Q>(&self, key: &Q, now_millis: u64) -> bool
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let guard = self.inner.read();
        if let Some(entry) = guard.map.get(key) {
            !entry.is_expired(now_millis)
        } else {
            false
        }
    }

    /// Inserts a key-value pair, evicting an entry if capacity is exceeded.
    pub fn insert(
        &self,
        key: K,
        value: V,
        ttl: Option<Duration>,
        now_millis: u64,
        metrics: &AtomicMetrics,
    ) -> Option<V>
    where
        K: Clone,
    {
        let mut guard = self.inner.write();
        let new_entry = CacheEntry::new(value, ttl, now_millis);

        if let Some(old_entry) = guard.map.get_mut(&key) {
            let old_val = std::mem::replace(&mut old_entry.value, new_entry.value);
            old_entry.expires_at_millis = new_entry.expires_at_millis;
            old_entry.last_accessed_millis = new_entry.last_accessed_millis;
            old_entry.access_count = new_entry.access_count;
            metrics.record_update();
            return Some(old_val);
        }

        // New key: check capacity
        if guard.capacity > 0 && guard.map.len() >= guard.capacity {
            evict_one(&mut guard, now_millis, metrics);
        }

        guard.map.insert(key, new_entry);
        metrics.record_insertion();
        None
    }

    /// Removes an entry by key.
    pub fn remove<Q>(&self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let mut guard = self.inner.write();
        guard.map.remove(key).map(|entry| entry.value)
    }

    /// Purges all expired entries from this shard. Returns count of purged items.
    pub fn purge_expired(&self, now_millis: u64, metrics: &AtomicMetrics) -> usize {
        let mut guard = self.inner.write();
        let initial_len = guard.map.len();
        guard.map.retain(|_, entry| !entry.is_expired(now_millis));
        let purged = initial_len - guard.map.len();

        if purged > 0 {
            for _ in 0..purged {
                metrics.record_expiration();
            }
        }

        purged
    }

    /// Clears all entries from this shard.
    pub fn clear(&self) {
        let mut guard = self.inner.write();
        guard.map.clear();
    }

    /// Refreshes the last-accessed timestamp of an entry if present and not expired.
    pub fn touch<Q>(&self, key: &Q, now_millis: u64) -> bool
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let guard = self.inner.read();
        if let Some(entry) = guard.map.get(key)
            && !entry.is_expired(now_millis)
        {
            entry.touch(now_millis);
            return true;
        }
        false
    }

    /// Collects all non-expired keys from this shard into the output buffer.
    pub fn collect_keys(&self, now_millis: u64, out: &mut Vec<K>)
    where
        K: Clone,
    {
        let guard = self.inner.read();
        for (k, entry) in guard.map.iter() {
            if !entry.is_expired(now_millis) {
                out.push(k.clone());
            }
        }
    }

    /// Retains only the entries in this shard that satisfy the predicate `f`.
    pub fn retain<F>(&self, now_millis: u64, f: &mut F)
    where
        F: FnMut(&K, &V) -> bool,
    {
        let mut guard = self.inner.write();
        guard
            .map
            .retain(|k, entry| !entry.is_expired(now_millis) && f(k, &entry.value));
    }

    /// Returns the number of entries currently stored in this shard.
    pub fn len(&self) -> usize {
        self.inner.read().map.len()
    }

    /// Returns true if the shard contains no entries.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Evicts a single entry from the shard when capacity is full.
fn evict_one<K: Eq + Hash + Clone, V, S: BuildHasher>(
    shard: &mut ShardInner<K, V, S>,
    now_millis: u64,
    metrics: &AtomicMetrics,
) {
    if shard.map.is_empty() {
        return;
    }

    // 1. Opportunistic eviction: check if any sampled item is already expired
    let sample_limit = match shard.eviction_strategy {
        EvictionStrategy::SampledLRU { sample_size } => sample_size.max(1),
        EvictionStrategy::SampledLFU { sample_size } => sample_size.max(1),
        EvictionStrategy::FIFO => 8,
        EvictionStrategy::None => return,
    };

    let mut expired_candidate = None;
    for (k, entry) in shard.map.iter().take(sample_limit) {
        if entry.is_expired(now_millis) {
            expired_candidate = Some(k.clone());
            break;
        }
    }

    if let Some(key) = expired_candidate {
        shard.map.remove(&key);
        metrics.record_expiration();
        return;
    }

    // 2. Apply configured eviction strategy
    match shard.eviction_strategy {
        EvictionStrategy::SampledLRU { sample_size } => {
            let mut oldest_access = u64::MAX;
            let mut best_key = None;

            for (k, entry) in shard.map.iter().take(sample_size.max(1)) {
                let accessed = entry.last_accessed();
                if accessed < oldest_access {
                    oldest_access = accessed;
                    best_key = Some(k.clone());
                }
            }

            if let Some(k) = best_key {
                shard.map.remove(&k);
                metrics.record_eviction();
            }
        }
        EvictionStrategy::SampledLFU { sample_size } => {
            let mut min_freq = u32::MAX;
            let mut best_key = None;

            for (k, entry) in shard.map.iter().take(sample_size.max(1)) {
                let freq = entry.access_count();
                if freq < min_freq {
                    min_freq = freq;
                    best_key = Some(k.clone());
                }
            }

            if let Some(k) = best_key {
                shard.map.remove(&k);
                metrics.record_eviction();
            }
        }
        EvictionStrategy::FIFO => {
            let mut earliest_created = u64::MAX;
            let mut best_key = None;

            for (k, entry) in shard.map.iter().take(sample_limit) {
                if entry.created_at_millis < earliest_created {
                    earliest_created = entry.created_at_millis;
                    best_key = Some(k.clone());
                }
            }

            if let Some(k) = best_key {
                shard.map.remove(&k);
                metrics.record_eviction();
            }
        }
        EvictionStrategy::None => {}
    }
}
