use async_trait::async_trait;
use std::time::{Duration, SystemTime};

/// Value stored in backend with optional expiry time.
///
/// Uses `SystemTime` for expiry to support persistence across restarts.
#[derive(Debug, Clone)]
pub struct StorageValue {
    pub data: String,
    /// Absolute expiry time (Unix epoch based, persistable).
    pub expires_at: Option<SystemTime>,
}

impl StorageValue {
    /// Create a value with no expiry.
    pub fn new(data: String) -> Self {
        Self {
            data,
            expires_at: None,
        }
    }

    /// Create a value that expires after the given TTL.
    pub fn new_with_expiry(data: String, ttl: Duration) -> Self {
        Self {
            data,
            expires_at: Some(SystemTime::now() + ttl),
        }
    }

    /// Check if the value has expired.
    pub fn is_expired(&self) -> bool {
        self.expires_at
            .map(|expires_at| SystemTime::now() > expires_at)
            .unwrap_or(false)
    }
}

/// Trait for pluggable storage backends.
///
/// All operations are async and thread-safe. Implementations handle
/// their own concurrency control and expiry cleanup.
#[async_trait]
pub trait StorageBackend: Send + Sync {
    /// Store a key-value pair without expiry.
    async fn set(&self, key: &str, value: &str) -> Result<(), StorageError>;

    /// Store a key-value pair with TTL expiry.
    async fn set_with_expiry(
        &self,
        key: &str,
        value: &str,
        ttl: Duration,
    ) -> Result<(), StorageError>;

    /// Retrieve a value by key. Returns None if key doesn't exist or expired.
    async fn get(&self, key: &str) -> Result<Option<String>, StorageError>;

    /// Delete a key. Returns true if key existed.
    async fn delete(&self, key: &str) -> Result<bool, StorageError>;

    /// Delete multiple keys. Returns count of keys that existed and were deleted.
    /// Default implementation calls delete() for each key individually.
    /// Backends can override for more efficient batch operations.
    async fn delete_many(&self, keys: &[&str]) -> Result<usize, StorageError> {
        let mut count = 0;
        for key in keys {
            if self.delete(key).await? {
                count += 1;
            }
        }
        Ok(count)
    }

    /// Check if a key exists and is not expired.
    async fn exists(&self, key: &str) -> Result<bool, StorageError>;

    /// Get total count of non-expired keys.
    async fn keys_count(&self) -> Result<usize, StorageError>;

    /// Remove all keys from the database.
    async fn flush(&self) -> Result<(), StorageError>;
}

/// Errors that can occur during storage operations.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("LMDB error: {0}")]
    Lmdb(#[from] lmdb::Error),

    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("operation failed: {0}")]
    OperationFailed(String),

    #[error("key not found: {0}")]
    KeyNotFound(String),

    #[error("connection error: {0}")]
    ConnectionError(String),
}
