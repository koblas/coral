use super::{StorageBackend, StorageError, StorageValue};
use async_trait::async_trait;
use lmdb::{Transaction, WriteFlags};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

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
    /// Create a new LMDB storage with default map size (10GB)
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self, StorageError> {
        Self::new_with_map_size(path, 10 * 1024 * 1024 * 1024) // 10GB default
    }

    /// Create a new LMDB storage with custom map size
    ///
    /// # Arguments
    /// * `path` - Path to the LMDB database file
    /// * `map_size` - Maximum size the database can grow to in bytes
    ///
    /// # Note
    /// On 64-bit systems, this is just address space reservation and doesn't
    /// consume actual memory or disk space until data is written.
    pub fn new_with_map_size<P: AsRef<Path>>(path: P, map_size: usize) -> Result<Self, StorageError> {
        let env = lmdb::Environment::new()
            .set_flags(lmdb::EnvironmentFlags::NO_SUB_DIR)
            .set_max_dbs(1)
            .set_map_size(map_size)
            .open(path.as_ref())
            .map_err(|e| StorageError::ConnectionError(format!("LMDB open error: {}", e)))?;

        let db = env
            .create_db(None, lmdb::DatabaseFlags::empty())
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
        
        txn.put(self.db, &key, &serialized, WriteFlags::empty())
            .map_err(|e| StorageError::OperationFailed(format!("Put error: {}", e)))?;
        
        Transaction::commit(txn)
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
        
        txn.put(self.db, &key, &serialized, WriteFlags::empty())
            .map_err(|e| StorageError::OperationFailed(format!("Put error: {}", e)))?;
        
        Transaction::commit(txn)
            .map_err(|e| StorageError::OperationFailed(format!("Commit error: {}", e)))?;

        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Option<String>, StorageError> {
        let (data, is_expired) = {
            let txn = self.env.begin_ro_txn()
                .map_err(|e| StorageError::OperationFailed(format!("Transaction error: {}", e)))?;

            match Transaction::get(&txn, self.db, &key) {
                Ok(bytes) => {
                    let serializable: SerializableStorageValue = serde_json::from_slice(bytes)
                        .map_err(|e| StorageError::SerializationError(e.to_string()))?;
                    let storage_value = StorageValue::from(serializable);

                    if storage_value.is_expired() {
                        (None, true)
                    } else {
                        (Some(storage_value.data), false)
                    }
                }
                Err(lmdb::Error::NotFound) => (None, false),
                Err(e) => return Err(StorageError::OperationFailed(format!("Get error: {}", e))),
            }
        };

        if is_expired {
            // Clean up expired key after transaction is dropped
            self.delete(key).await?;
        }

        Ok(data)
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
        let stat = self.env.stat()
            .map_err(|e| StorageError::OperationFailed(format!("Stat error: {}", e)))?;

        Ok(stat.entries())
    }

    async fn flush(&self) -> Result<(), StorageError> {
        let mut txn = self.env.begin_rw_txn()
            .map_err(|e| StorageError::OperationFailed(format!("Transaction error: {}", e)))?;
        
        txn.clear_db(self.db)
            .map_err(|e| StorageError::OperationFailed(format!("Clear error: {}", e)))?;
        
        Transaction::commit(txn)
            .map_err(|e| StorageError::OperationFailed(format!("Commit error: {}", e)))?;
        
        Ok(())
    }
}