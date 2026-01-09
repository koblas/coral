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