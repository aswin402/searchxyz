use std::time::{Duration, Instant};

use lru::LruCache;
use std::num::NonZeroUsize;

/// A single cached page.
#[derive(Debug, Clone)]
pub struct CacheEntry {
    pub content: String,
    pub url: String,
    pub fetched_at: Instant,
    pub ttl: Duration,
}

impl CacheEntry {
    pub fn new(content: String, url: String) -> Self {
        Self {
            content,
            url,
            fetched_at: Instant::now(),
            ttl: Duration::from_secs(3600), // default 1 hour
        }
    }

    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.ttl = ttl;
        self
    }

    pub fn is_expired(&self) -> bool {
        self.fetched_at.elapsed() > self.ttl
    }
}

/// Thread-safe LRU cache for crawled page content.
/// Wrap in `Arc<Mutex<Cache>>` for shared access.
pub struct Cache {
    inner: LruCache<String, CacheEntry>,
    default_ttl: Duration,
}

impl Cache {
    pub fn new(max_entries: usize, ttl_secs: u64) -> Self {
        let cap = NonZeroUsize::new(max_entries.max(1)).unwrap();
        Self {
            inner: LruCache::new(cap),
            default_ttl: Duration::from_secs(ttl_secs),
        }
    }

    /// Retrieve a cached entry. Returns `None` if missing or expired.
    pub fn get(&self, url: &str) -> Option<&CacheEntry> {
        // Note: LruCache::peek does not promote the entry.
        // We use peek here because `get` requires &mut self.
        // The caller can use `get_mut` if promotion is desired.
        self.inner.peek(url).filter(|e| !e.is_expired())
    }

    /// Retrieve and promote a cached entry.
    pub fn get_mut(&mut self, url: &str) -> Option<&CacheEntry> {
        // Get promotes the entry to most-recently-used.
        let entry = self.inner.get(url)?;
        if entry.is_expired() {
            // Remove expired entry.
            self.inner.pop(url);
            return None;
        }
        // Re-borrow after potential removal.
        self.inner.get(url).map(|e| &*e)
    }

    /// Insert or update a cache entry.
    pub fn put(&mut self, url: String, mut entry: CacheEntry) {
        // Apply default TTL if entry uses the 1-hour default.
        if entry.ttl == Duration::from_secs(3600) {
            entry.ttl = self.default_ttl;
        }
        self.inner.put(url, entry);
    }

    /// Number of entries currently cached.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Whether the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}
