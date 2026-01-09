#[cfg(feature = "lmdb-backend")]
use super::{StorageBackend, StorageError, StorageValue};
#[cfg(feature = "lmdb-backend")]
use async_trait::async_trait;
#[cfg(feature = "lmdb-backend")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "lmdb-backend")]
use std::path::Path;
#[cfg(feature = "lmdb-backend")]
use std::sync::Arc;
#[cfg(feature = "lmdb-backend")]
use std::time::Duration;

#[cfg(feature = "lmdb-backend")]
#[derive(Serialize, Deserialize)]
struct SerializableStorageValue {
    data: String,
    expires_at: Option<u64>, // Unix timestamp in milliseconds
}

#[cfg(feature = "lmdb-backend")]
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

#[cfg(feature = "lmdb-backend")]
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

#[cfg(feature = "lmdb-backend")]
pub struct LmdbStorage {
    env: Arc<lmdb::Environment>,
    db: lmdb::Database,
}

#[cfg(feature = "lmdb-backend")]
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

#[cfg(feature = "lmdb-backend")]
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