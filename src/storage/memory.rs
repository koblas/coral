use super::{StorageBackend, StorageError, StorageValue};
use async_trait::async_trait;
use papaya::HashMap;
use std::time::Duration;

/// In-memory storage backend using concurrent hashmap.
///
/// Fastest backend option. Data is volatile and lost on shutdown.
/// Uses lazy expiry cleanup (expired keys removed on access).
/// Built on papaya for high-performance concurrent access.
pub struct MemoryStorage {
    data: HashMap<String, StorageValue>,
}

impl Default for MemoryStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryStorage {
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
        }
    }
}

#[async_trait]
impl StorageBackend for MemoryStorage {
    async fn set(&self, key: &str, value: &str) -> Result<(), StorageError> {
        let guard = self.data.pin();
        guard.insert(key.to_owned(), StorageValue::new(value.to_owned()));
        Ok(())
    }

    async fn set_with_expiry(&self, key: &str, value: &str, ttl: Duration) -> Result<(), StorageError> {
        let guard = self.data.pin();
        guard.insert(key.to_owned(), StorageValue::new_with_expiry(value.to_owned(), ttl));
        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Option<String>, StorageError> {
        let guard = self.data.pin();

        if let Some(entry) = guard.get(key) {
            if entry.is_expired() {
                // Drop guard before removing to avoid holding reference
                drop(guard);
                let remove_guard = self.data.pin();
                remove_guard.remove(key);
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
        Ok(guard.remove(key).is_some())
    }

    async fn exists(&self, key: &str) -> Result<bool, StorageError> {
        let guard = self.data.pin();

        if let Some(entry) = guard.get(key) {
            if entry.is_expired() {
                // Drop guard before removing
                drop(guard);
                let remove_guard = self.data.pin();
                remove_guard.remove(key);
                Ok(false)
            } else {
                Ok(true)
            }
        } else {
            Ok(false)
        }
    }

    async fn keys_count(&self) -> Result<usize, StorageError> {
        // Count non-expired keys and collect expired ones
        let mut count = 0;
        let mut expired_keys = Vec::new();

        {
            let guard = self.data.pin();
            for (key, value) in guard.iter() {
                if value.is_expired() {
                    expired_keys.push(key.clone());
                } else {
                    count += 1;
                }
            }
        }

        // Remove expired keys
        let remove_guard = self.data.pin();
        for key in expired_keys {
            remove_guard.remove(&key);
        }

        Ok(count)
    }

    async fn flush(&self) -> Result<(), StorageError> {
        let guard = self.data.pin();
        guard.clear();
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