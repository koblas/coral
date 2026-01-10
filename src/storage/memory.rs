use super::{StorageBackend, StorageError, StorageValue};
use async_trait::async_trait;
use dashmap::DashMap;
use std::time::Duration;

/// In-memory storage backend using concurrent hashmap.
///
/// Fastest backend option. Data is volatile and lost on shutdown.
/// Uses lazy expiry cleanup (expired keys removed on access).
pub struct MemoryStorage {
    data: DashMap<String, StorageValue>,
}

impl Default for MemoryStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryStorage {
    pub fn new() -> Self {
        Self {
            data: DashMap::new(),
        }
    }
}

#[async_trait]
impl StorageBackend for MemoryStorage {
    async fn set(&self, key: String, value: String) -> Result<(), StorageError> {
        self.data.insert(key, StorageValue::new(value));
        Ok(())
    }

    async fn set_with_expiry(&self, key: String, value: String, ttl: Duration) -> Result<(), StorageError> {
        self.data.insert(key, StorageValue::new_with_expiry(value, ttl));
        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Option<String>, StorageError> {
        if let Some(entry) = self.data.get(key) {
            if entry.is_expired() {
                drop(entry);
                self.data.remove(key);
                Ok(None)
            } else {
                Ok(Some(entry.data.clone()))
            }
        } else {
            Ok(None)
        }
    }

    async fn delete(&self, key: &str) -> Result<bool, StorageError> {
        Ok(self.data.remove(key).is_some())
    }

    async fn exists(&self, key: &str) -> Result<bool, StorageError> {
        if let Some(entry) = self.data.get(key) {
            if entry.is_expired() {
                drop(entry);
                self.data.remove(key);
                Ok(false)
            } else {
                Ok(true)
            }
        } else {
            Ok(false)
        }
    }

    async fn keys_count(&self) -> Result<usize, StorageError> {
        // Clean up expired keys during count
        let expired_keys: Vec<String> = self
            .data
            .iter()
            .filter_map(|entry| {
                if entry.value().is_expired() {
                    Some(entry.key().clone())
                } else {
                    None
                }
            })
            .collect();

        for key in expired_keys {
            self.data.remove(&key);
        }

        Ok(self.data.len())
    }

    async fn flush(&self) -> Result<(), StorageError> {
        self.data.clear();
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
        storage.set("key1".to_string(), "value1".to_string()).await.unwrap();
        
        assert_eq!(storage.get("key1").await.unwrap(), Some("value1".to_string()));
        assert_eq!(storage.get("nonexistent").await.unwrap(), None);
    }

    #[tokio::test]
    async fn test_memory_delete() {
        let storage = MemoryStorage::new();
        storage.set("key1".to_string(), "value1".to_string()).await.unwrap();
        
        assert!(storage.delete("key1").await.unwrap());
        assert!(!storage.delete("nonexistent").await.unwrap());
        assert_eq!(storage.get("key1").await.unwrap(), None);
    }

    #[tokio::test]
    async fn test_memory_exists() {
        let storage = MemoryStorage::new();
        storage.set("key1".to_string(), "value1".to_string()).await.unwrap();
        
        assert!(storage.exists("key1").await.unwrap());
        assert!(!storage.exists("nonexistent").await.unwrap());
        
        storage.delete("key1").await.unwrap();
        assert!(!storage.exists("key1").await.unwrap());
    }

    #[tokio::test]
    async fn test_memory_expiry() {
        let storage = MemoryStorage::new();
        storage.set_with_expiry(
            "expiring_key".to_string(),
            "value".to_string(),
            Duration::from_millis(50),
        ).await.unwrap();
        
        assert_eq!(storage.get("expiring_key").await.unwrap(), Some("value".to_string()));
        assert!(storage.exists("expiring_key").await.unwrap());
        
        thread::sleep(Duration::from_millis(100));
        
        assert_eq!(storage.get("expiring_key").await.unwrap(), None);
        assert!(!storage.exists("expiring_key").await.unwrap());
    }
}