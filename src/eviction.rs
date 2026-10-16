#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Policy used to select entries for eviction when a shard exceeds capacity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum EvictionStrategy {
    /// Sampled Least Recently Used (LRU).
    ///
    /// Samples `sample_size` candidates from the shard and evicts the one
    /// with the oldest access timestamp.
    /// Default `sample_size` is typically 8. This achieves near-optimal
    /// eviction efficiency with zero linked-list overhead and zero lock contention.
    SampledLRU {
        /// Number of random candidate keys to sample during eviction.
        sample_size: usize,
    },

    /// Sampled Least Frequently Used (LFU).
    ///
    /// Samples `sample_size` candidates and evicts the one with the lowest
    /// cumulative access count.
    SampledLFU {
        /// Number of random candidate keys to sample during eviction.
        sample_size: usize,
    },

    /// First In First Out (FIFO).
    ///
    /// Evicts the oldest entry based on insertion time.
    FIFO,

    /// No eviction.
    ///
    /// Once the shard reaches capacity, subsequent insertions are rejected
    /// unless expired entries can be freed.
    None,
}

impl Default for EvictionStrategy {
    fn default() -> Self {
        Self::SampledLRU { sample_size: 8 }
    }
}
