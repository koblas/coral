use crate::metrics::{Metrics, Timer};
use crate::protocol::{RespParser, RespValue};
use crate::storage::StorageBackend;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tracing::{debug, warn};

pub struct Handler {
    storage: Arc<dyn StorageBackend>,
}

impl Handler {
    pub fn new(storage: Arc<dyn StorageBackend>) -> Self {
        Self { storage }
    }

    pub async fn handle_stream(
        &self,
        stream: &mut TcpStream,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let metrics = Metrics::get();
        metrics.increment_connections();
        
        let mut parser = RespParser::new();
        let mut buffer = [0; 1024];

        loop {
            let n = stream.read(&mut buffer).await?;
            if n == 0 {
                break; // Connection closed
            }

            parser.add_data(&buffer[0..n]);

            while let Some(value) = parser.parse()? {
                debug!("Received command: {:?}", value);
                
                let timer = Timer::new();
                let response = self.handle_command(value).await;
                let duration = timer.elapsed_seconds();
                
                metrics.record_request(duration);
                
                let response_bytes = response.to_bytes();
                stream.write_all(&response_bytes).await?;
                stream.flush().await?;
            }
        }

        Ok(())
    }

    pub async fn handle_command(&self, value: RespValue) -> RespValue {
        let metrics = Metrics::get();
        
        match value {
            RespValue::Array(Some(parts)) if !parts.is_empty() => {
                let command = match &parts[0] {
                    RespValue::BulkString(Some(cmd)) => cmd.to_uppercase(),
                    _ => {
                        metrics.record_error("invalid_command_format", None);
                        return RespValue::Error("Invalid command format".to_string());
                    }
                };

                let timer = Timer::new();
                let response = match command.as_str() {
                    "PING" => self.handle_ping(&parts[1..]).await,
                    "SET" => self.handle_set(&parts[1..]).await,
                    "GET" => self.handle_get(&parts[1..]).await,
                    "DEL" => self.handle_del(&parts[1..]).await,
                    "EXISTS" => self.handle_exists(&parts[1..]).await,
                    "DBSIZE" => self.handle_dbsize().await,
                    "FLUSHDB" => self.handle_flushdb().await,
                    "COMMAND" => self.handle_command_info().await,
                    _ => {
                        metrics.record_error("unknown_command", Some(&command));
                        RespValue::Error(format!("Unknown command: {}", command))
                    }
                };
                
                let duration = timer.elapsed_seconds();
                metrics.record_command(&command, duration);
                
                response
            }
            _ => {
                metrics.record_error("invalid_command_format", None);
                RespValue::Error("Invalid command format".to_string())
            }
        }
    }

    async fn handle_ping(&self, args: &[RespValue]) -> RespValue {
        match args.len() {
            0 => RespValue::SimpleString("PONG".to_string()),
            1 => match &args[0] {
                RespValue::BulkString(Some(message)) => {
                    RespValue::BulkString(Some(message.clone()))
                }
                _ => RespValue::Error("Invalid PING argument".to_string()),
            },
            _ => RespValue::Error("Wrong number of arguments for PING".to_string()),
        }
    }

    async fn handle_set(&self, args: &[RespValue]) -> RespValue {
        let metrics = Metrics::get();
        
        if args.len() < 2 {
            return RespValue::Error("Wrong number of arguments for SET".to_string());
        }

        let key = match &args[0] {
            RespValue::BulkString(Some(k)) => k.clone(),
            _ => return RespValue::Error("Invalid key".to_string()),
        };

        let value = match &args[1] {
            RespValue::BulkString(Some(v)) => v.clone(),
            _ => return RespValue::Error("Invalid value".to_string()),
        };

        // Check for EX option (expiry in seconds)
        if args.len() >= 4 {
            if let (RespValue::BulkString(Some(option)), RespValue::BulkString(Some(ttl_str))) =
                (&args[2], &args[3])
            {
                if option.to_uppercase() == "EX" {
                    if let Ok(ttl_secs) = ttl_str.parse::<u64>() {
                        let timer = Timer::new();
                        let result = self.storage
                            .set_with_expiry(key.clone(), value, Duration::from_secs(ttl_secs)).await;
                        let duration = timer.elapsed_seconds();
                        
                        match result {
                            Ok(()) => {
                                metrics.record_storage_operation("set_with_expiry", "storage", duration);
                                metrics.record_key_operation("set", 1);
                                return RespValue::SimpleString("OK".to_string());
                            }
                            Err(_e) => {
                                metrics.record_storage_error("set_with_expiry", "storage", "operation_failed");
                                return RespValue::Error("SET failed".to_string());
                            }
                        }
                    } else {
                        return RespValue::Error("Invalid TTL".to_string());
                    }
                }
            }
        }

        let timer = Timer::new();
        let result = self.storage.set(key.clone(), value).await;
        let duration = timer.elapsed_seconds();
        
        match result {
            Ok(()) => {
                metrics.record_storage_operation("set", "storage", duration);
                metrics.record_key_operation("set", 1);
                RespValue::SimpleString("OK".to_string())
            }
            Err(_) => {
                metrics.record_storage_error("set", "storage", "operation_failed");
                RespValue::Error("SET failed".to_string())
            }
        }
    }

    async fn handle_get(&self, args: &[RespValue]) -> RespValue {
        let metrics = Metrics::get();
        
        if args.len() != 1 {
            return RespValue::Error("Wrong number of arguments for GET".to_string());
        }

        let key = match &args[0] {
            RespValue::BulkString(Some(k)) => k,
            _ => return RespValue::Error("Invalid key".to_string()),
        };

        let timer = Timer::new();
        let result = self.storage.get(key).await;
        let duration = timer.elapsed_seconds();
        
        match result {
            Ok(Some(value)) => {
                metrics.record_storage_operation("get", "storage", duration);
                RespValue::BulkString(Some(value))
            }
            Ok(None) => {
                metrics.record_storage_operation("get", "storage", duration);
                RespValue::BulkString(None)
            }
            Err(_) => {
                metrics.record_storage_error("get", "storage", "operation_failed");
                RespValue::Error("GET failed".to_string())
            }
        }
    }

    async fn handle_del(&self, args: &[RespValue]) -> RespValue {
        let metrics = Metrics::get();
        
        if args.is_empty() {
            return RespValue::Error("Wrong number of arguments for DEL".to_string());
        }

        let mut deleted_count = 0;
        for arg in args {
            let key = match arg {
                RespValue::BulkString(Some(k)) => k,
                _ => {
                    warn!("Invalid key in DEL command");
                    continue;
                }
            };

            let timer = Timer::new();
            let result = self.storage.delete(key).await;
            let duration = timer.elapsed_seconds();
            
            match result {
                Ok(true) => {
                    metrics.record_storage_operation("delete", "storage", duration);
                    deleted_count += 1;
                }
                Ok(false) => {
                    metrics.record_storage_operation("delete", "storage", duration);
                    // Key didn't exist, not an error
                }
                Err(_) => {
                    metrics.record_storage_error("delete", "storage", "operation_failed");
                    warn!("Failed to delete key: {}", key);
                }
            }
        }

        RespValue::Integer(deleted_count)
    }

    async fn handle_exists(&self, args: &[RespValue]) -> RespValue {
        if args.is_empty() {
            return RespValue::Error("Wrong number of arguments for EXISTS".to_string());
        }

        let mut exists_count = 0;
        for arg in args {
            let key = match arg {
                RespValue::BulkString(Some(k)) => k,
                _ => {
                    warn!("Invalid key in EXISTS command");
                    continue;
                }
            };

            match self.storage.exists(key).await {
                Ok(true) => exists_count += 1,
                Ok(false) => {},
                Err(_) => {
                    warn!("Failed to check existence of key: {}", key);
                }
            }
        }

        RespValue::Integer(exists_count)
    }

    async fn handle_dbsize(&self) -> RespValue {
        match self.storage.keys_count().await {
            Ok(count) => RespValue::Integer(count as i64),
            Err(_) => RespValue::Error("DBSIZE failed".to_string()),
        }
    }

    async fn handle_flushdb(&self) -> RespValue {
        match self.storage.flush().await {
            Ok(()) => RespValue::SimpleString("OK".to_string()),
            Err(_) => RespValue::Error("FLUSHDB failed".to_string()),
        }
    }

    async fn handle_command_info(&self) -> RespValue {
        // Return empty array for COMMAND (Redis clients sometimes call this)
        RespValue::Array(Some(vec![]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::memory::MemoryStorage;
    use std::sync::Arc;

    fn create_handler() -> Handler {
        let storage: Arc<dyn StorageBackend> = Arc::new(MemoryStorage::new());
        Handler::new(storage)
    }

    #[tokio::test]
    async fn test_ping_no_args() {
        let handler = create_handler();
        let result = handler.handle_ping(&[]).await;
        
        match result {
            RespValue::SimpleString(s) => assert_eq!(s, "PONG"),
            _ => panic!("Expected SimpleString"),
        }
    }

    #[tokio::test]
    async fn test_ping_with_message() {
        let handler = create_handler();
        let args = vec![RespValue::BulkString(Some("hello".to_string()))];
        let result = handler.handle_ping(&args).await;
        
        match result {
            RespValue::BulkString(Some(s)) => assert_eq!(s, "hello"),
            _ => panic!("Expected BulkString"),
        }
    }

    #[tokio::test]
    async fn test_set_get() {
        let handler = create_handler();
        
        // SET key value
        let set_args = vec![
            RespValue::BulkString(Some("mykey".to_string())),
            RespValue::BulkString(Some("myvalue".to_string())),
        ];
        let set_result = handler.handle_set(&set_args).await;
        
        match set_result {
            RespValue::SimpleString(s) => assert_eq!(s, "OK"),
            _ => panic!("Expected OK"),
        }
        
        // GET key
        let get_args = vec![RespValue::BulkString(Some("mykey".to_string()))];
        let get_result = handler.handle_get(&get_args).await;
        
        match get_result {
            RespValue::BulkString(Some(s)) => assert_eq!(s, "myvalue"),
            _ => panic!("Expected BulkString with value"),
        }
    }

    #[tokio::test]
    async fn test_get_nonexistent() {
        let handler = create_handler();
        let args = vec![RespValue::BulkString(Some("nonexistent".to_string()))];
        let result = handler.handle_get(&args).await;
        
        match result {
            RespValue::BulkString(None) => {},
            _ => panic!("Expected null BulkString"),
        }
    }

    #[tokio::test]
    async fn test_del() {
        let handler = create_handler();
        
        // Set a key first
        let set_args = vec![
            RespValue::BulkString(Some("key1".to_string())),
            RespValue::BulkString(Some("value1".to_string())),
        ];
        handler.handle_set(&set_args).await;
        
        // Delete the key
        let del_args = vec![RespValue::BulkString(Some("key1".to_string()))];
        let result = handler.handle_del(&del_args).await;
        
        match result {
            RespValue::Integer(i) => assert_eq!(i, 1),
            _ => panic!("Expected Integer 1"),
        }
        
        // Try to delete non-existent key
        let del_args = vec![RespValue::BulkString(Some("nonexistent".to_string()))];
        let result = handler.handle_del(&del_args).await;
        
        match result {
            RespValue::Integer(i) => assert_eq!(i, 0),
            _ => panic!("Expected Integer 0"),
        }
    }

    #[tokio::test]
    async fn test_exists() {
        let handler = create_handler();
        
        // Set a key
        let set_args = vec![
            RespValue::BulkString(Some("key1".to_string())),
            RespValue::BulkString(Some("value1".to_string())),
        ];
        handler.handle_set(&set_args).await;
        
        // Check if key exists
        let exists_args = vec![RespValue::BulkString(Some("key1".to_string()))];
        let result = handler.handle_exists(&exists_args).await;
        
        match result {
            RespValue::Integer(i) => assert_eq!(i, 1),
            _ => panic!("Expected Integer 1"),
        }
        
        // Check non-existent key
        let exists_args = vec![RespValue::BulkString(Some("nonexistent".to_string()))];
        let result = handler.handle_exists(&exists_args).await;
        
        match result {
            RespValue::Integer(i) => assert_eq!(i, 0),
            _ => panic!("Expected Integer 0"),
        }
    }

    #[tokio::test]
    async fn test_dbsize() {
        let handler = create_handler();
        
        let result = handler.handle_dbsize().await;
        match result {
            RespValue::Integer(i) => assert_eq!(i, 0),
            _ => panic!("Expected Integer 0"),
        }
        
        // Add some keys
        let set_args = vec![
            RespValue::BulkString(Some("key1".to_string())),
            RespValue::BulkString(Some("value1".to_string())),
        ];
        handler.handle_set(&set_args).await;
        
        let result = handler.handle_dbsize().await;
        match result {
            RespValue::Integer(i) => assert_eq!(i, 1),
            _ => panic!("Expected Integer 1"),
        }
    }

    #[tokio::test]
    async fn test_flushdb() {
        let handler = create_handler();
        
        // Add some keys
        let set_args = vec![
            RespValue::BulkString(Some("key1".to_string())),
            RespValue::BulkString(Some("value1".to_string())),
        ];
        handler.handle_set(&set_args).await;
        
        let result = handler.handle_flushdb().await;
        match result {
            RespValue::SimpleString(s) => assert_eq!(s, "OK"),
            _ => panic!("Expected OK"),
        }
        
        // Verify database is empty
        let dbsize_result = handler.handle_dbsize().await;
        match dbsize_result {
            RespValue::Integer(i) => assert_eq!(i, 0),
            _ => panic!("Expected Integer 0"),
        }
    }

    #[tokio::test]
    async fn test_invalid_commands() {
        let handler = create_handler();
        
        // Test wrong number of arguments for SET
        let result = handler.handle_set(&[]).await;
        match result {
            RespValue::Error(_) => {},
            _ => panic!("Expected Error"),
        }
        
        // Test wrong number of arguments for GET
        let result = handler.handle_get(&[]).await;
        match result {
            RespValue::Error(_) => {},
            _ => panic!("Expected Error"),
        }
    }
}
