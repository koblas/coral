use async_trait::async_trait;
use std::time::Duration;

/// Value stored in backend with optional expiry time.
#[derive(Debug, Clone)]
pub struct StorageValue {
    pub data: String,
    pub expires_at: Option<std::time::Instant>,
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
            expires_at: Some(std::time::Instant::now() + ttl),
        }
    }

    /// Check if the value has expired.
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            std::time::Instant::now() > expires_at
        } else {
            false
        }
    }
}

/// Trait for pluggable storage backends.
///
/// All operations are async and thread-safe. Implementations handle
/// their own concurrency control and expiry cleanup.
#[async_trait]
pub trait StorageBackend: Send + Sync {
    /// Store a key-value pair without expiry.
    async fn set(&self, key: String, value: String) -> Result<(), StorageError>;

    /// Store a key-value pair with TTL expiry.
    async fn set_with_expiry(&self, key: String, value: String, ttl: Duration) -> Result<(), StorageError>;

    /// Retrieve a value by key. Returns None if key doesn't exist or expired.
    async fn get(&self, key: &str) -> Result<Option<String>, StorageError>;

    /// Delete a key. Returns true if key existed.
    async fn delete(&self, key: &str) -> Result<bool, StorageError>;

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
    #[error("Storage operation failed: {0}")]
    OperationFailed(String),
    #[error("Key not found: {0}")]
    KeyNotFound(String),
    #[error("Connection error: {0}")]
    ConnectionError(String),
    #[error("Serialization error: {0}")]
    SerializationError(String),
}