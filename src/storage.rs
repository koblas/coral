use dashmap::DashMap;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct StorageValue {
    pub data: String,
    pub expires_at: Option<Instant>,
}

impl StorageValue {
    pub fn new(data: String) -> Self {
        Self {
            data,
            expires_at: None,
        }
    }

    pub fn new_with_expiry(data: String, ttl: Duration) -> Self {
        Self {
            data,
            expires_at: Some(Instant::now() + ttl),
        }
    }

    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            Instant::now() > expires_at
        } else {
            false
        }
    }
}

pub struct Storage {
    data: DashMap<String, StorageValue>,
}

impl Default for Storage {
    fn default() -> Self {
        Self::new()
    }
}

impl Storage {
    pub fn new() -> Self {
        Self {
            data: DashMap::new(),
        }
    }

    pub fn set(&self, key: String, value: String) {
        self.data.insert(key, StorageValue::new(value));
    }

    pub fn set_with_expiry(&self, key: String, value: String, ttl: Duration) {
        self.data
            .insert(key, StorageValue::new_with_expiry(value, ttl));
    }

    pub fn get(&self, key: &str) -> Option<String> {
        if let Some(entry) = self.data.get(key) {
            if entry.is_expired() {
                drop(entry);
                self.data.remove(key);
                None
            } else {
                Some(entry.data.clone())
            }
        } else {
            None
        }
    }

    pub fn delete(&self, key: &str) -> bool {
        self.data.remove(key).is_some()
    }

    pub fn exists(&self, key: &str) -> bool {
        if let Some(entry) = self.data.get(key) {
            if entry.is_expired() {
                drop(entry);
                self.data.remove(key);
                false
            } else {
                true
            }
        } else {
            false
        }
    }

    pub fn keys_count(&self) -> usize {
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

        self.data.len()
    }

    pub fn flush(&self) {
        self.data.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_basic_set_get() {
        let storage = Storage::new();
        storage.set("key1".to_string(), "value1".to_string());
        
        assert_eq!(storage.get("key1"), Some("value1".to_string()));
        assert_eq!(storage.get("nonexistent"), None);
    }

    #[test]
    fn test_delete() {
        let storage = Storage::new();
        storage.set("key1".to_string(), "value1".to_string());
        
        assert!(storage.delete("key1"));
        assert!(!storage.delete("nonexistent"));
        assert_eq!(storage.get("key1"), None);
    }

    #[test]
    fn test_exists() {
        let storage = Storage::new();
        storage.set("key1".to_string(), "value1".to_string());
        
        assert!(storage.exists("key1"));
        assert!(!storage.exists("nonexistent"));
        
        storage.delete("key1");
        assert!(!storage.exists("key1"));
    }

    #[test]
    fn test_keys_count() {
        let storage = Storage::new();
        assert_eq!(storage.keys_count(), 0);
        
        storage.set("key1".to_string(), "value1".to_string());
        storage.set("key2".to_string(), "value2".to_string());
        assert_eq!(storage.keys_count(), 2);
        
        storage.delete("key1");
        assert_eq!(storage.keys_count(), 1);
    }

    #[test]
    fn test_flush() {
        let storage = Storage::new();
        storage.set("key1".to_string(), "value1".to_string());
        storage.set("key2".to_string(), "value2".to_string());
        
        storage.flush();
        assert_eq!(storage.keys_count(), 0);
        assert_eq!(storage.get("key1"), None);
    }

    #[test]
    fn test_overwrite_value() {
        let storage = Storage::new();
        storage.set("key1".to_string(), "value1".to_string());
        storage.set("key1".to_string(), "value2".to_string());
        
        assert_eq!(storage.get("key1"), Some("value2".to_string()));
    }

    #[test]
    fn test_expiry() {
        let storage = Storage::new();
        storage.set_with_expiry(
            "expiring_key".to_string(),
            "value".to_string(),
            Duration::from_millis(50),
        );
        
        assert_eq!(storage.get("expiring_key"), Some("value".to_string()));
        assert!(storage.exists("expiring_key"));
        
        thread::sleep(Duration::from_millis(100));
        
        assert_eq!(storage.get("expiring_key"), None);
        assert!(!storage.exists("expiring_key"));
    }

    #[test]
    fn test_expired_key_cleanup_in_count() {
        let storage = Storage::new();
        storage.set("normal_key".to_string(), "value".to_string());
        storage.set_with_expiry(
            "expiring_key".to_string(),
            "value".to_string(),
            Duration::from_millis(50),
        );
        
        assert_eq!(storage.keys_count(), 2);
        
        thread::sleep(Duration::from_millis(100));
        
        // The expired key should be cleaned up during count
        assert_eq!(storage.keys_count(), 1);
    }
}
