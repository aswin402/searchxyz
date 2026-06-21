use std::time::Duration;
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};
use lru::LruCache;
use std::num::NonZeroUsize;
use crate::error::SearchXyzError;

/// A single cached page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    pub content: String,
    pub url: String,
    pub fetched_at: DateTime<Utc>,
    pub ttl_secs: u64,
}

impl CacheEntry {
    pub fn new(content: String, url: String) -> Self {
        Self {
            content,
            url,
            fetched_at: Utc::now(),
            ttl_secs: 3600, // default 1 hour
        }
    }

    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.ttl_secs = ttl.as_secs();
        self
    }

    pub fn is_expired(&self) -> bool {
        let elapsed = Utc::now().signed_duration_since(self.fetched_at);
        if elapsed.is_zero() || elapsed.num_seconds() < 0 {
            false
        } else {
            elapsed.num_seconds() as u64 > self.ttl_secs
        }
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

    /// Load cache from a JSON file, or start empty if the file is missing/corrupted.
    pub fn load_from_file(
        max_entries: usize,
        ttl_secs: u64,
        path: &std::path::Path,
    ) -> Self {
        let mut cache = Self::new(max_entries, ttl_secs);
        if !path.exists() {
            return cache;
        }

        match std::fs::read_to_string(path) {
            Ok(contents) => {
                match serde_json::from_str::<Vec<CacheEntry>>(&contents) {
                    Ok(entries) => {
                        // Insert non-expired entries
                        for entry in entries {
                            if !entry.is_expired() {
                                cache.put(entry.url.clone(), entry);
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!(path = ?path, error = %e, "Failed to parse cache file, starting with empty cache");
                    }
                }
            }
            Err(e) => {
                tracing::warn!(path = ?path, error = %e, "Failed to read cache file, starting with empty cache");
            }
        }
        cache
    }

    /// Save cache entries to a JSON file.
    pub fn save_to_file(&self, path: &std::path::Path) -> Result<(), SearchXyzError> {
        let mut entries = Vec::new();
        // LruCache yields entries from oldest to newest.
        for (_, entry) in self.inner.iter() {
            if !entry.is_expired() {
                entries.push(entry.clone());
            }
        }

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let file = std::fs::File::create(path)?;
        serde_json::to_writer_pretty(file, &entries)?;
        Ok(())
    }

    /// Retrieve a cached entry. Returns `None` if missing or expired.
    pub fn get(&self, url: &str) -> Option<&CacheEntry> {
        self.inner.peek(url).filter(|e| !e.is_expired())
    }

    /// Retrieve and promote a cached entry.
    pub fn get_mut(&mut self, url: &str) -> Option<&CacheEntry> {
        let entry = self.inner.get(url)?;
        if entry.is_expired() {
            self.inner.pop(url);
            return None;
        }
        self.inner.get(url).map(|e| &*e)
    }

    /// Insert or update a cache entry.
    pub fn put(&mut self, url: String, mut entry: CacheEntry) {
        if entry.ttl_secs == 3600 {
            entry.ttl_secs = self.default_ttl.as_secs();
        }
        self.inner.put(url, entry);
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_serialization_deserialization() {
        let temp_dir = std::env::temp_dir();
        let cache_path = temp_dir.join("searchxyz_test_cache_persistence.json");

        // Clean up any stale files
        let _ = std::fs::remove_file(&cache_path);

        let mut cache = Cache::new(3, 30);
        let entry1 = CacheEntry::new("content 1".to_string(), "http://example.com/1".to_string());
        let entry2 = CacheEntry::new("content 2".to_string(), "http://example.com/2".to_string());

        cache.put("http://example.com/1".to_string(), entry1);
        cache.put("http://example.com/2".to_string(), entry2);

        // Save to file
        assert!(cache.save_to_file(&cache_path).is_ok());
        assert!(cache_path.exists());

        // Load into new cache instance
        let restored_cache = Cache::load_from_file(3, 30, &cache_path);
        assert_eq!(restored_cache.len(), 2);

        let hit1 = restored_cache.get("http://example.com/1");
        assert!(hit1.is_some());
        assert_eq!(hit1.unwrap().content, "content 1");

        let hit2 = restored_cache.get("http://example.com/2");
        assert!(hit2.is_some());
        assert_eq!(hit2.unwrap().content, "content 2");

        // Clean up
        let _ = std::fs::remove_file(&cache_path);
    }
}
