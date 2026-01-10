use crate::metrics::{Metrics, Timer};
use crate::protocol::{ProtocolVersion, RespParser, RespValue};
use crate::storage::StorageBackend;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tracing::{debug, warn};

/// Handles client connections and Redis command processing.
///
/// Parses RESP protocol, dispatches commands, and records metrics.
/// Tracks protocol version per connection for RESP2/RESP3 support.
pub struct Handler {
    storage: Arc<dyn StorageBackend>,
    protocol_version: ProtocolVersion,
}

impl Handler {
    /// Create a new handler with the given storage backend.
    /// Defaults to RESP2 protocol.
    pub fn new(storage: Arc<dyn StorageBackend>) -> Self {
        Self::new_with_protocol(storage, ProtocolVersion::default())
    }

    /// Create a new handler with specified protocol version.
    pub fn new_with_protocol(storage: Arc<dyn StorageBackend>, protocol_version: ProtocolVersion) -> Self {
        Self {
            storage,
            protocol_version,
        }
    }

    /// Get the current protocol version for this connection.
    pub fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }

    /// Set the protocol version for this connection.
    pub fn set_protocol_version(&mut self, version: ProtocolVersion) {
        self.protocol_version = version;
    }

    /// Process commands from a TCP connection until it closes.
    pub async fn handle_stream(
        &mut self,
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

            loop {
                match parser.parse() {
                    Ok(Some(value)) => {
                        debug!("Received command: {:?}", value);

                        let timer = Timer::new();
                        let response = self.handle_command(value).await;
                        let duration = timer.elapsed_seconds();

                        metrics.record_request(duration);

                        let response_bytes = response.to_bytes();
                        stream.write_all(&response_bytes).await?;
                        stream.flush().await?;
                    }
                    Ok(None) => {
                        // Need more data
                        break;
                    }
                    Err(e) => {
                        // Protocol error - send error response but keep connection alive
                        warn!("Protocol error: {}", e);
                        metrics.record_error("protocol_error", None);

                        let error = RespValue::Error(format!("ERR Protocol error: {}", e));
                        let error_bytes = error.to_bytes();
                        stream.write_all(&error_bytes).await?;
                        stream.flush().await?;

                        // Reset parser to recover from error
                        parser.reset();
                        break;
                    }
                }
            }
        }

        Ok(())
    }

    /// Dispatch a Redis command to the appropriate handler.
    pub async fn handle_command(&mut self, value: RespValue) -> RespValue {
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
                    "HELLO" => self.handle_hello(&parts[1..]).await,
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

    /// Handle HELLO command for protocol negotiation.
    /// Format: HELLO [protover [AUTH username password] [SETNAME clientname]]
    async fn handle_hello(&mut self, args: &[RespValue]) -> RespValue {
        // Parse protocol version if provided
        let requested_version = if args.is_empty() {
            None
        } else {
            match &args[0] {
                RespValue::BulkString(Some(ver_str)) => {
                    match ver_str.parse::<u8>() {
                        Ok(2) => Some(ProtocolVersion::Resp2),
                        Ok(3) => Some(ProtocolVersion::Resp3),
                        Ok(v) => return RespValue::Error(format!("ERR unsupported protocol version: {}", v)),
                        Err(_) => return RespValue::Error("ERR protocol version must be a number".to_string()),
                    }
                }
                _ => return RespValue::Error("ERR protocol version must be a string".to_string()),
            }
        };

        // Set protocol version if requested
        if let Some(version) = requested_version {
            self.set_protocol_version(version);
        }

        // Build response based on current protocol version
        match self.protocol_version() {
            ProtocolVersion::Resp3 => {
                // RESP3: Return Map
                RespValue::Map(vec![
                    (RespValue::BulkString(Some("server".to_string())), RespValue::BulkString(Some("coral-redis".to_string()))),
                    (RespValue::BulkString(Some("version".to_string())), RespValue::BulkString(Some("0.1.0".to_string()))),
                    (RespValue::BulkString(Some("proto".to_string())), RespValue::Integer(3)),
                    (RespValue::BulkString(Some("mode".to_string())), RespValue::BulkString(Some("standalone".to_string()))),
                    (RespValue::BulkString(Some("role".to_string())), RespValue::BulkString(Some("master".to_string()))),
                ])
            }
            ProtocolVersion::Resp2 => {
                // RESP2: Return Array (key1, value1, key2, value2, ...)
                RespValue::Array(Some(vec![
                    RespValue::BulkString(Some("server".to_string())),
                    RespValue::BulkString(Some("coral-redis".to_string())),
                    RespValue::BulkString(Some("version".to_string())),
                    RespValue::BulkString(Some("0.1.0".to_string())),
                    RespValue::BulkString(Some("proto".to_string())),
                    RespValue::Integer(2),
                    RespValue::BulkString(Some("mode".to_string())),
                    RespValue::BulkString(Some("standalone".to_string())),
                    RespValue::BulkString(Some("role".to_string())),
                    RespValue::BulkString(Some("master".to_string())),
                ]))
            }
        }
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

    #[tokio::test]
    async fn test_hello_no_args() {
        let mut handler = create_handler();

        // HELLO with no arguments should return current version info
        let result = handler.handle_hello(&[]).await;

        // Default is RESP2, so should get Array response
        match result {
            RespValue::Array(Some(items)) => {
                assert_eq!(items.len(), 10); // 5 key-value pairs
            },
            _ => panic!("Expected Array for RESP2 response"),
        }
    }

    #[tokio::test]
    async fn test_hello_resp2() {
        let mut handler = create_handler();

        // Request RESP2 protocol
        let args = vec![RespValue::BulkString(Some("2".to_string()))];
        let result = handler.handle_hello(&args).await;

        // Should get Array response (RESP2 format)
        match result {
            RespValue::Array(Some(items)) => {
                assert_eq!(items.len(), 10); // 5 key-value pairs
            },
            _ => panic!("Expected Array for RESP2 response"),
        }

        // Verify protocol version was set
        assert_eq!(handler.protocol_version(), ProtocolVersion::Resp2);
    }

    #[tokio::test]
    async fn test_hello_resp3() {
        let mut handler = create_handler();

        // Request RESP3 protocol
        let args = vec![RespValue::BulkString(Some("3".to_string()))];
        let result = handler.handle_hello(&args).await;

        // Should get Map response (RESP3 format)
        match result {
            RespValue::Map(pairs) => {
                assert_eq!(pairs.len(), 5);

                // Verify "proto" field is 3
                let proto_field = pairs.iter().find(|(k, _)| {
                    matches!(k, RespValue::BulkString(Some(s)) if s == "proto")
                });
                assert!(proto_field.is_some());

                if let Some((_, RespValue::Integer(proto))) = proto_field {
                    assert_eq!(*proto, 3);
                }
            },
            _ => panic!("Expected Map for RESP3 response"),
        }

        // Verify protocol version was set
        assert_eq!(handler.protocol_version(), ProtocolVersion::Resp3);
    }

    #[tokio::test]
    async fn test_hello_invalid_version() {
        let mut handler = create_handler();

        // Request invalid protocol version
        let args = vec![RespValue::BulkString(Some("99".to_string()))];
        let result = handler.handle_hello(&args).await;

        // Should get error
        match result {
            RespValue::Error(msg) => {
                assert!(msg.contains("unsupported protocol version"));
            },
            _ => panic!("Expected Error for invalid version"),
        }

        // Verify protocol version unchanged (still default RESP2)
        assert_eq!(handler.protocol_version(), ProtocolVersion::Resp2);
    }
}
