use async_trait::async_trait;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct StorageValue {
    pub data: String,
    pub expires_at: Option<std::time::Instant>,
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
            expires_at: Some(std::time::Instant::now() + ttl),
        }
    }

    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            std::time::Instant::now() > expires_at
        } else {
            false
        }
    }
}

#[async_trait]
pub trait StorageBackend: Send + Sync {
    async fn set(&self, key: String, value: String) -> Result<(), StorageError>;
    async fn set_with_expiry(&self, key: String, value: String, ttl: Duration) -> Result<(), StorageError>;
    async fn get(&self, key: &str) -> Result<Option<String>, StorageError>;
    async fn delete(&self, key: &str) -> Result<bool, StorageError>;
    async fn exists(&self, key: &str) -> Result<bool, StorageError>;
    async fn keys_count(&self) -> Result<usize, StorageError>;
    async fn flush(&self) -> Result<(), StorageError>;
}

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("Storage operation failed: {0}")]
    OperationFailed(String),
    #[error("Key not found: {0}")]
    KeyNotFound(String),
    #[error("Connection error: {0}")]
    ConnectionError(String),
    #[error("Serialization error: {0}")]
    SerializationError(String),
}

// Memory storage implementation (refactored from original Storage)
pub mod memory {
    use super::*;
    use dashmap::DashMap;

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
}

// LMDB storage implementation
#[cfg(feature = "lmdb-backend")]
pub mod lmdb {
    use super::*;
    use serde::{Deserialize, Serialize};
    use std::path::Path;
    use std::sync::Arc;

    #[derive(Serialize, Deserialize)]
    struct SerializableStorageValue {
        data: String,
        expires_at: Option<u64>, // Unix timestamp in milliseconds
    }

    impl From<StorageValue> for SerializableStorageValue {
        fn from(value: StorageValue) -> Self {
            Self {
                data: value.data,
                expires_at: value.expires_at.map(|instant| {
                    instant.duration_since(std::time::Instant::now()).as_millis() as u64 
                    + std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64
                }),
            }
        }
    }

    impl From<SerializableStorageValue> for StorageValue {
        fn from(value: SerializableStorageValue) -> Self {
            Self {
                data: value.data,
                expires_at: value.expires_at.map(|timestamp| {
                    let now_millis = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_millis() as u64;
                    let duration_from_now = if timestamp > now_millis {
                        Duration::from_millis(timestamp - now_millis)
                    } else {
                        Duration::from_millis(0) // Already expired
                    };
                    std::time::Instant::now() + duration_from_now
                }),
            }
        }
    }

    pub struct LmdbStorage {
        env: Arc<lmdb::Environment>,
        db: lmdb::Database,
    }

    impl LmdbStorage {
        pub fn new<P: AsRef<Path>>(path: P) -> Result<Self, StorageError> {
            let env = lmdb::Environment::new()
                .set_flags(lmdb::EnvironmentFlags::NO_SUB_DIR)
                .set_max_dbs(1)
                .open(path)
                .map_err(|e| StorageError::ConnectionError(format!("LMDB open error: {}", e)))?;

            let db = env
                .open_db(None)
                .map_err(|e| StorageError::ConnectionError(format!("LMDB db error: {}", e)))?;

            Ok(Self {
                env: Arc::new(env),
                db,
            })
        }
    }

    #[async_trait]
    impl StorageBackend for LmdbStorage {
        async fn set(&self, key: String, value: String) -> Result<(), StorageError> {
            let storage_value = StorageValue::new(value);
            let serializable = SerializableStorageValue::from(storage_value);
            let serialized = serde_json::to_vec(&serializable)
                .map_err(|e| StorageError::SerializationError(e.to_string()))?;

            let mut txn = self.env.begin_rw_txn()
                .map_err(|e| StorageError::OperationFailed(format!("Transaction error: {}", e)))?;
            
            txn.put(self.db, &key, &serialized, lmdb::WriteFlags::empty())
                .map_err(|e| StorageError::OperationFailed(format!("Put error: {}", e)))?;
            
            txn.commit()
                .map_err(|e| StorageError::OperationFailed(format!("Commit error: {}", e)))?;

            Ok(())
        }

        async fn set_with_expiry(&self, key: String, value: String, ttl: Duration) -> Result<(), StorageError> {
            let storage_value = StorageValue::new_with_expiry(value, ttl);
            let serializable = SerializableStorageValue::from(storage_value);
            let serialized = serde_json::to_vec(&serializable)
                .map_err(|e| StorageError::SerializationError(e.to_string()))?;

            let mut txn = self.env.begin_rw_txn()
                .map_err(|e| StorageError::OperationFailed(format!("Transaction error: {}", e)))?;
            
            txn.put(self.db, &key, &serialized, lmdb::WriteFlags::empty())
                .map_err(|e| StorageError::OperationFailed(format!("Put error: {}", e)))?;
            
            txn.commit()
                .map_err(|e| StorageError::OperationFailed(format!("Commit error: {}", e)))?;

            Ok(())
        }

        async fn get(&self, key: &str) -> Result<Option<String>, StorageError> {
            let txn = self.env.begin_ro_txn()
                .map_err(|e| StorageError::OperationFailed(format!("Transaction error: {}", e)))?;
            
            match txn.get(self.db, &key) {
                Ok(bytes) => {
                    let serializable: SerializableStorageValue = serde_json::from_slice(bytes)
                        .map_err(|e| StorageError::SerializationError(e.to_string()))?;
                    let storage_value = StorageValue::from(serializable);
                    
                    if storage_value.is_expired() {
                        // Clean up expired key
                        drop(txn);
                        self.delete(key).await?;
                        Ok(None)
                    } else {
                        Ok(Some(storage_value.data))
                    }
                }
                Err(lmdb::Error::NotFound) => Ok(None),
                Err(e) => Err(StorageError::OperationFailed(format!("Get error: {}", e))),
            }
        }

        async fn delete(&self, key: &str) -> Result<bool, StorageError> {
            let mut txn = self.env.begin_rw_txn()
                .map_err(|e| StorageError::OperationFailed(format!("Transaction error: {}", e)))?;
            
            match txn.del(self.db, &key, None) {
                Ok(()) => {
                    txn.commit()
                        .map_err(|e| StorageError::OperationFailed(format!("Commit error: {}", e)))?;
                    Ok(true)
                }
                Err(lmdb::Error::NotFound) => Ok(false),
                Err(e) => Err(StorageError::OperationFailed(format!("Delete error: {}", e))),
            }
        }

        async fn exists(&self, key: &str) -> Result<bool, StorageError> {
            match self.get(key).await? {
                Some(_) => Ok(true),
                None => Ok(false),
            }
        }

        async fn keys_count(&self) -> Result<usize, StorageError> {
            let txn = self.env.begin_ro_txn()
                .map_err(|e| StorageError::OperationFailed(format!("Transaction error: {}", e)))?;
            
            let stat = txn.stat(self.db)
                .map_err(|e| StorageError::OperationFailed(format!("Stat error: {}", e)))?;
            
            Ok(stat.entries())
        }

        async fn flush(&self) -> Result<(), StorageError> {
            let mut txn = self.env.begin_rw_txn()
                .map_err(|e| StorageError::OperationFailed(format!("Transaction error: {}", e)))?;
            
            txn.clear_db(self.db)
                .map_err(|e| StorageError::OperationFailed(format!("Clear error: {}", e)))?;
            
            txn.commit()
                .map_err(|e| StorageError::OperationFailed(format!("Commit error: {}", e)))?;
            
            Ok(())
        }
    }
}

// S3 storage implementation
#[cfg(feature = "s3-backend")]
pub mod s3 {
    use super::*;
    use aws_sdk_s3::Client;
    use serde::{Deserialize, Serialize};
    use std::collections::HashMap;

    #[derive(Serialize, Deserialize)]
    struct S3StorageValue {
        data: String,
        expires_at: Option<u64>,
    }

    pub struct S3Storage {
        client: Client,
        bucket: String,
        prefix: String,
    }

    impl S3Storage {
        pub async fn new(bucket: String, prefix: Option<String>) -> Result<Self, StorageError> {
            let config = aws_config::load_from_env().await;
            let client = Client::new(&config);
            
            Ok(Self {
                client,
                bucket,
                prefix: prefix.unwrap_or_else(|| "redis/".to_string()),
            })
        }

        fn key_path(&self, key: &str) -> String {
            format!("{}{}", self.prefix, key)
        }
    }

    #[async_trait]
    impl StorageBackend for S3Storage {
        async fn set(&self, key: String, value: String) -> Result<(), StorageError> {
            let storage_value = S3StorageValue {
                data: value,
                expires_at: None,
            };
            
            let body = serde_json::to_vec(&storage_value)
                .map_err(|e| StorageError::SerializationError(e.to_string()))?;

            self.client
                .put_object()
                .bucket(&self.bucket)
                .key(self.key_path(&key))
                .body(body.into())
                .send()
                .await
                .map_err(|e| StorageError::OperationFailed(format!("S3 put error: {}", e)))?;

            Ok(())
        }

        async fn set_with_expiry(&self, key: String, value: String, ttl: Duration) -> Result<(), StorageError> {
            let expires_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64 + ttl.as_millis() as u64;

            let storage_value = S3StorageValue {
                data: value,
                expires_at: Some(expires_at),
            };
            
            let body = serde_json::to_vec(&storage_value)
                .map_err(|e| StorageError::SerializationError(e.to_string()))?;

            self.client
                .put_object()
                .bucket(&self.bucket)
                .key(self.key_path(&key))
                .body(body.into())
                .send()
                .await
                .map_err(|e| StorageError::OperationFailed(format!("S3 put error: {}", e)))?;

            Ok(())
        }

        async fn get(&self, key: &str) -> Result<Option<String>, StorageError> {
            match self.client
                .get_object()
                .bucket(&self.bucket)
                .key(self.key_path(key))
                .send()
                .await
            {
                Ok(output) => {
                    let bytes = output.body.collect().await
                        .map_err(|e| StorageError::OperationFailed(format!("S3 body read error: {}", e)))?
                        .into_bytes();
                    
                    let storage_value: S3StorageValue = serde_json::from_slice(&bytes)
                        .map_err(|e| StorageError::SerializationError(e.to_string()))?;

                    // Check expiration
                    if let Some(expires_at) = storage_value.expires_at {
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_millis() as u64;
                        
                        if now > expires_at {
                            // Delete expired key
                            self.delete(key).await?;
                            return Ok(None);
                        }
                    }

                    Ok(Some(storage_value.data))
                }
                Err(e) => {
                    if e.to_string().contains("NoSuchKey") {
                        Ok(None)
                    } else {
                        Err(StorageError::OperationFailed(format!("S3 get error: {}", e)))
                    }
                }
            }
        }

        async fn delete(&self, key: &str) -> Result<bool, StorageError> {
            match self.client
                .delete_object()
                .bucket(&self.bucket)
                .key(self.key_path(key))
                .send()
                .await
            {
                Ok(_) => Ok(true),
                Err(e) => {
                    if e.to_string().contains("NoSuchKey") {
                        Ok(false)
                    } else {
                        Err(StorageError::OperationFailed(format!("S3 delete error: {}", e)))
                    }
                }
            }
        }

        async fn exists(&self, key: &str) -> Result<bool, StorageError> {
            match self.client
                .head_object()
                .bucket(&self.bucket)
                .key(self.key_path(key))
                .send()
                .await
            {
                Ok(_) => Ok(true),
                Err(e) => {
                    if e.to_string().contains("NotFound") {
                        Ok(false)
                    } else {
                        Err(StorageError::OperationFailed(format!("S3 head error: {}", e)))
                    }
                }
            }
        }

        async fn keys_count(&self) -> Result<usize, StorageError> {
            let mut count = 0;
            let mut continuation_token = None;

            loop {
                let mut request = self.client
                    .list_objects_v2()
                    .bucket(&self.bucket)
                    .prefix(&self.prefix);

                if let Some(token) = continuation_token {
                    request = request.continuation_token(token);
                }

                let output = request.send().await
                    .map_err(|e| StorageError::OperationFailed(format!("S3 list error: {}", e)))?;

                if let Some(contents) = output.contents {
                    count += contents.len();
                }

                if output.is_truncated.unwrap_or(false) {
                    continuation_token = output.next_continuation_token;
                } else {
                    break;
                }
            }

            Ok(count)
        }

        async fn flush(&self) -> Result<(), StorageError> {
            // List all objects with the prefix
            let mut continuation_token = None;
            let mut keys_to_delete = Vec::new();

            loop {
                let mut request = self.client
                    .list_objects_v2()
                    .bucket(&self.bucket)
                    .prefix(&self.prefix);

                if let Some(token) = continuation_token {
                    request = request.continuation_token(token);
                }

                let output = request.send().await
                    .map_err(|e| StorageError::OperationFailed(format!("S3 list error: {}", e)))?;

                if let Some(contents) = output.contents {
                    for object in contents {
                        if let Some(key) = object.key {
                            keys_to_delete.push(key);
                        }
                    }
                }

                if output.is_truncated.unwrap_or(false) {
                    continuation_token = output.next_continuation_token;
                } else {
                    break;
                }
            }

            // Delete objects in batches (S3 allows up to 1000 per batch)
            for chunk in keys_to_delete.chunks(1000) {
                let delete_objects: Vec<_> = chunk.iter()
                    .map(|key| aws_sdk_s3::types::ObjectIdentifier::builder().key(key).build().unwrap())
                    .collect();

                if !delete_objects.is_empty() {
                    let delete = aws_sdk_s3::types::Delete::builder()
                        .set_objects(Some(delete_objects))
                        .build()
                        .unwrap();

                    self.client
                        .delete_objects()
                        .bucket(&self.bucket)
                        .delete(delete)
                        .send()
                        .await
                        .map_err(|e| StorageError::OperationFailed(format!("S3 batch delete error: {}", e)))?;
                }
            }

            Ok(())
        }
    }
}

// Storage factory for creating different backends
pub struct StorageFactory;

impl StorageFactory {
    pub async fn create_memory() -> Box<dyn StorageBackend> {
        Box::new(memory::MemoryStorage::new())
    }

    #[cfg(feature = "lmdb-backend")]
    pub async fn create_lmdb<P: AsRef<std::path::Path>>(path: P) -> Result<Box<dyn StorageBackend>, StorageError> {
        Ok(Box::new(lmdb::LmdbStorage::new(path)?))
    }

    #[cfg(feature = "s3-backend")]
    pub async fn create_s3(bucket: String, prefix: Option<String>) -> Result<Box<dyn StorageBackend>, StorageError> {
        Ok(Box::new(s3::S3Storage::new(bucket, prefix).await?))
    }
}