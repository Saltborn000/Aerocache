use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::Duration;

/// An individual entry stored within a cache shard.
///
/// Uses atomic integers for last accessed timestamp and frequency counter
/// so that concurrent readers only require a read-lock on the shard without
/// suffering write-lock serializations.
#[derive(Debug)]
pub struct CacheEntry<V> {
    /// The stored value.
    pub value: V,
    /// Absolute timestamp in milliseconds since cache epoch when this entry expires.
    /// `0` signifies no expiration (lives indefinitely).
    pub expires_at_millis: u64,
    /// Milliseconds since cache epoch when this entry was last accessed.
    pub last_accessed_millis: AtomicU64,
    /// Frequency counter for LFU eviction calculations.
    pub access_count: AtomicU32,
    /// Timestamp in milliseconds when this entry was created.
    pub created_at_millis: u64,
}

impl<V> CacheEntry<V> {
    /// Constructs a new entry with an optional Time-To-Live.
    #[inline]
    pub fn new(value: V, ttl: Option<Duration>, now_millis: u64) -> Self {
        let expires_at_millis = match ttl {
            Some(d) => now_millis.saturating_add(d.as_millis() as u64),
            None => 0,
        };

        Self {
            value,
            expires_at_millis,
            last_accessed_millis: AtomicU64::new(now_millis),
            access_count: AtomicU32::new(1),
            created_at_millis: now_millis,
        }
    }

    /// Checks whether this entry has passed its expiration time.
    #[inline]
    pub fn is_expired(&self, now_millis: u64) -> bool {
        self.expires_at_millis != 0 && now_millis >= self.expires_at_millis
    }

    /// Updates the last accessed timestamp and increments access count concurrently.
    #[inline]
    pub fn touch(&self, now_millis: u64) {
        self.last_accessed_millis
            .store(now_millis, Ordering::Relaxed);
        self.access_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Gets the last accessed timestamp in milliseconds.
    #[inline]
    pub fn last_accessed(&self) -> u64 {
        self.last_accessed_millis.load(Ordering::Relaxed)
    }

    /// Gets the access count.
    #[inline]
    pub fn access_count(&self) -> u32 {
        self.access_count.load(Ordering::Relaxed)
    }
}
