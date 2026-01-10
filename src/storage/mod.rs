//! Storage backend implementations and abstractions.
//!
//! Provides pluggable storage with Memory, LMDB, and S3 backends.

pub mod traits;
pub mod memory;
pub mod lmdb;
pub mod s3;

pub use traits::*;

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