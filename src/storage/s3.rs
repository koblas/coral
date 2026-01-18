#[cfg(feature = "s3-backend")]
use super::{StorageBackend, StorageError};
#[cfg(feature = "s3-backend")]
use async_trait::async_trait;
#[cfg(feature = "s3-backend")]
use aws_sdk_s3::Client;
#[cfg(feature = "s3-backend")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "s3-backend")]
use std::time::Duration;

#[cfg(feature = "s3-backend")]
#[derive(Serialize, Deserialize)]
struct S3StorageValue {
    data: String,
    expires_at: Option<u64>,
}

#[cfg(feature = "s3-backend")]
pub struct S3Storage {
    client: Client,
    bucket: String,
    prefix: String,
}

#[cfg(feature = "s3-backend")]
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

#[cfg(feature = "s3-backend")]
#[async_trait]
impl StorageBackend for S3Storage {
    async fn set(&self, key: &str, value: &str) -> Result<(), StorageError> {
        let storage_value = S3StorageValue {
            data: value.to_owned(),
            expires_at: None,
        };

        let body = serde_json::to_vec(&storage_value)?;

        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(self.key_path(key))
            .body(body.into())
            .send()
            .await
            .map_err(|e| StorageError::OperationFailed(format!("S3 put error: {}", e)))?;

        Ok(())
    }

    async fn set_with_expiry(
        &self,
        key: &str,
        value: &str,
        ttl: Duration,
    ) -> Result<(), StorageError> {
        let expires_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
            + ttl.as_millis() as u64;

        let storage_value = S3StorageValue {
            data: value.to_owned(),
            expires_at: Some(expires_at),
        };

        let body = serde_json::to_vec(&storage_value)?;

        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(self.key_path(key))
            .body(body.into())
            .send()
            .await
            .map_err(|e| StorageError::OperationFailed(format!("S3 put error: {}", e)))?;

        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Option<String>, StorageError> {
        match self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(self.key_path(key))
            .send()
            .await
        {
            Ok(output) => {
                let bytes = output
                    .body
                    .collect()
                    .await
                    .map_err(|e| {
                        StorageError::OperationFailed(format!("S3 body read error: {}", e))
                    })?
                    .into_bytes();

                let storage_value: S3StorageValue = serde_json::from_slice(&bytes)?;

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
                    Err(StorageError::OperationFailed(format!(
                        "S3 get error: {}",
                        e
                    )))
                }
            }
        }
    }

    async fn delete(&self, key: &str) -> Result<bool, StorageError> {
        match self
            .client
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
                    Err(StorageError::OperationFailed(format!(
                        "S3 delete error: {}",
                        e
                    )))
                }
            }
        }
    }

    async fn exists(&self, key: &str) -> Result<bool, StorageError> {
        match self
            .client
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
                    Err(StorageError::OperationFailed(format!(
                        "S3 head error: {}",
                        e
                    )))
                }
            }
        }
    }

    async fn keys_count(&self) -> Result<usize, StorageError> {
        let mut count = 0;
        let mut continuation_token = None;

        loop {
            let mut request = self
                .client
                .list_objects_v2()
                .bucket(&self.bucket)
                .prefix(&self.prefix);

            if let Some(token) = continuation_token {
                request = request.continuation_token(token);
            }

            let output = request
                .send()
                .await
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
            let mut request = self
                .client
                .list_objects_v2()
                .bucket(&self.bucket)
                .prefix(&self.prefix);

            if let Some(token) = continuation_token {
                request = request.continuation_token(token);
            }

            let output = request
                .send()
                .await
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
            let delete_objects: Vec<_> = chunk
                .iter()
                .map(|key| {
                    aws_sdk_s3::types::ObjectIdentifier::builder()
                        .key(key)
                        .build()
                        .unwrap()
                })
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
                    .map_err(|e| {
                        StorageError::OperationFailed(format!("S3 batch delete error: {}", e))
                    })?;
            }
        }

        Ok(())
    }
}
