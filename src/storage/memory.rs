use super::{StorageBackend, StorageError, StorageValue};
use async_trait::async_trait;
use papaya::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// In-memory storage backend using concurrent hashmap.
///
/// Fastest backend option. Data is volatile and lost on shutdown.
/// Uses lazy expiry cleanup (expired keys removed on access).
/// Built on papaya for high-performance concurrent access.
pub struct MemoryStorage {
    data: HashMap<String, StorageValue>,
    approximate_count: Arc<AtomicUsize>,
}

impl Default for MemoryStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryStorage {
    pub fn new() -> Self {
        Self::new_with_cleanup_interval(Duration::from_secs(60))
    }

    pub fn new_with_cleanup_interval(cleanup_interval: Duration) -> Self {
        let data = HashMap::new();
        let approximate_count = Arc::new(AtomicUsize::new(0));

        // Spawn background cleanup task
        let data_clone = data.clone();
        let count_clone = Arc::clone(&approximate_count);

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(cleanup_interval);
            loop {
                interval.tick().await;
                Self::cleanup_expired(&data_clone, &count_clone);
            }
        });

        Self {
            data,
            approximate_count,
        }
    }

    fn cleanup_expired(data: &HashMap<String, StorageValue>, count: &AtomicUsize) {
        let mut to_remove = Vec::new();

        {
            let guard = data.pin();
            for (key, value) in guard.iter() {
                if value.is_expired() {
                    to_remove.push(key.clone());
                }
            }
        }

        if !to_remove.is_empty() {
            let guard = data.pin();
            for key in &to_remove {
                if guard.remove(key).is_some() {
                    count.fetch_sub(1, Ordering::Relaxed);
                }
            }
        }
    }
}

#[async_trait]
impl StorageBackend for MemoryStorage {
    async fn set(&self, key: &str, value: &str) -> Result<(), StorageError> {
        let guard = self.data.pin();
        let is_new = guard.get(key).is_none();
        guard.insert(key.to_owned(), StorageValue::new(value.to_owned()));

        if is_new {
            self.approximate_count.fetch_add(1, Ordering::Relaxed);
        }
        Ok(())
    }

    async fn set_with_expiry(
        &self,
        key: &str,
        value: &str,
        ttl: Duration,
    ) -> Result<(), StorageError> {
        let guard = self.data.pin();
        let is_new = guard.get(key).is_none();
        guard.insert(
            key.to_owned(),
            StorageValue::new_with_expiry(value.to_owned(), ttl),
        );

        if is_new {
            self.approximate_count.fetch_add(1, Ordering::Relaxed);
        }
        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Option<String>, StorageError> {
        let guard = self.data.pin();

        if let Some(entry) = guard.get(key) {
            if entry.is_expired() {
                // Drop guard before removing to avoid holding reference
                drop(guard);
                let remove_guard = self.data.pin();
                if remove_guard.remove(key).is_some() {
                    self.approximate_count.fetch_sub(1, Ordering::Relaxed);
                }
                Ok(None)
            } else {
                Ok(Some(entry.data.clone()))
            }
        } else {
            Ok(None)
        }
    }

    async fn delete(&self, key: &str) -> Result<bool, StorageError> {
        let guard = self.data.pin();
        let existed = guard.remove(key).is_some();

        if existed {
            self.approximate_count.fetch_sub(1, Ordering::Relaxed);
        }
        Ok(existed)
    }

    async fn exists(&self, key: &str) -> Result<bool, StorageError> {
        let guard = self.data.pin();

        if let Some(entry) = guard.get(key) {
            if entry.is_expired() {
                // Drop guard before removing
                drop(guard);
                let remove_guard = self.data.pin();
                if remove_guard.remove(key).is_some() {
                    self.approximate_count.fetch_sub(1, Ordering::Relaxed);
                }
                Ok(false)
            } else {
                Ok(true)
            }
        } else {
            Ok(false)
        }
    }

    async fn keys_count(&self) -> Result<usize, StorageError> {
        // Return approximate count - O(1) instead of O(n)
        // Note: May include recently expired keys until they're accessed
        Ok(self.approximate_count.load(Ordering::Relaxed))
    }

    async fn flush(&self) -> Result<(), StorageError> {
        let guard = self.data.pin();
        guard.clear();
        self.approximate_count.store(0, Ordering::Relaxed);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[tokio::test]
    async fn test_memory_basic_set_get() {
        let storage = MemoryStorage::new();
        storage.set("key1", "value1").await.unwrap();

        assert_eq!(
            storage.get("key1").await.unwrap(),
            Some("value1".to_string())
        );
        assert_eq!(storage.get("nonexistent").await.unwrap(), None);
    }

    #[tokio::test]
    async fn test_memory_delete() {
        let storage = MemoryStorage::new();
        storage.set("key1", "value1").await.unwrap();

        assert!(storage.delete("key1").await.unwrap());
        assert!(!storage.delete("nonexistent").await.unwrap());
        assert_eq!(storage.get("key1").await.unwrap(), None);
    }

    #[tokio::test]
    async fn test_memory_exists() {
        let storage = MemoryStorage::new();
        storage.set("key1", "value1").await.unwrap();

        assert!(storage.exists("key1").await.unwrap());
        assert!(!storage.exists("nonexistent").await.unwrap());

        storage.delete("key1").await.unwrap();
        assert!(!storage.exists("key1").await.unwrap());
    }

    #[tokio::test]
    async fn test_memory_expiry() {
        let storage = MemoryStorage::new();
        storage
            .set_with_expiry("expiring_key", "value", Duration::from_millis(50))
            .await
            .unwrap();

        assert_eq!(
            storage.get("expiring_key").await.unwrap(),
            Some("value".to_string())
        );
        assert!(storage.exists("expiring_key").await.unwrap());

        thread::sleep(Duration::from_millis(100));

        assert_eq!(storage.get("expiring_key").await.unwrap(), None);
        assert!(!storage.exists("expiring_key").await.unwrap());
    }
}
