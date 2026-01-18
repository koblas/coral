use super::{StorageBackend, StorageError, StorageValue};
use async_trait::async_trait;
use lmdb::{Transaction, WriteFlags};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};

/// Serializable representation of storage values for LMDB persistence.
#[derive(Serialize, Deserialize)]
struct SerializableStorageValue {
    data: String,
    /// Unix timestamp in milliseconds for expiry (persistable across restarts).
    expires_at_ms: Option<u64>,
}

impl From<&StorageValue> for SerializableStorageValue {
    fn from(value: &StorageValue) -> Self {
        Self {
            data: value.data.clone(),
            expires_at_ms: value.expires_at.and_then(|t| {
                t.duration_since(UNIX_EPOCH).ok().map(|d| d.as_millis() as u64)
            }),
        }
    }
}

impl From<SerializableStorageValue> for StorageValue {
    fn from(value: SerializableStorageValue) -> Self {
        Self {
            data: value.data,
            expires_at: value.expires_at_ms.map(|ms| {
                UNIX_EPOCH + Duration::from_millis(ms)
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
            .open(path.as_ref())?;

        let db = env.create_db(None, lmdb::DatabaseFlags::empty())?;

        Ok(Self {
            env: Arc::new(env),
            db,
        })
    }
}

#[async_trait]
impl StorageBackend for LmdbStorage {
    async fn set(&self, key: &str, value: &str) -> Result<(), StorageError> {
        let storage_value = StorageValue::new(value.to_owned());
        let serializable = SerializableStorageValue::from(&storage_value);
        let serialized = serde_json::to_vec(&serializable)?;

        let mut txn = self.env.begin_rw_txn()?;
        txn.put(self.db, &key, &serialized, WriteFlags::empty())?;
        Transaction::commit(txn)?;

        Ok(())
    }

    async fn set_with_expiry(&self, key: &str, value: &str, ttl: Duration) -> Result<(), StorageError> {
        let storage_value = StorageValue::new_with_expiry(value.to_owned(), ttl);
        let serializable = SerializableStorageValue::from(&storage_value);
        let serialized = serde_json::to_vec(&serializable)?;

        let mut txn = self.env.begin_rw_txn()?;
        txn.put(self.db, &key, &serialized, WriteFlags::empty())?;
        Transaction::commit(txn)?;

        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Option<String>, StorageError> {
        let (data, is_expired) = {
            let txn = self.env.begin_ro_txn()?;

            match Transaction::get(&txn, self.db, &key) {
                Ok(bytes) => {
                    let serializable: SerializableStorageValue = serde_json::from_slice(bytes)?;
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
        let mut txn = self.env.begin_rw_txn()?;

        match txn.del(self.db, &key, None) {
            Ok(()) => {
                txn.commit()?;
                Ok(true)
            }
            Err(lmdb::Error::NotFound) => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    async fn exists(&self, key: &str) -> Result<bool, StorageError> {
        Ok(self.get(key).await?.is_some())
    }

    async fn keys_count(&self) -> Result<usize, StorageError> {
        let stat = self.env.stat()?;
        Ok(stat.entries())
    }

    async fn flush(&self) -> Result<(), StorageError> {
        let mut txn = self.env.begin_rw_txn()?;
        txn.clear_db(self.db)?;
        Transaction::commit(txn)?;
        Ok(())
    }
}