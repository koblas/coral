use coral_redis::{Handler, RespValue, StorageBackend};
use coral_redis::config::Config;
use coral_redis::storage::memory::MemoryStorage;
use std::sync::Arc;

#[tokio::test]
async fn test_server_integration_basic_commands() {
    // Start server in background
    let storage: Arc<dyn StorageBackend> = Arc::new(MemoryStorage::new());
    let mut handler = Handler::new(storage);
    
    // This is a simplified integration test focusing on the handler logic
    // In a real integration test, you'd start the actual server
    
    // Test PING command
    let ping_response = handler.handle_command(
        RespValue::Array(Some(vec![
            RespValue::BulkString(Some("PING".to_string())),
        ]))
    ).await;
    
    match ping_response {
        RespValue::SimpleString(s) => assert_eq!(s, "PONG"),
        _ => panic!("Expected PONG"),
    }
    
    // Test SET command
    let set_response = handler.handle_command(
        RespValue::Array(Some(vec![
            RespValue::BulkString(Some("SET".to_string())),
            RespValue::BulkString(Some("testkey".to_string())),
            RespValue::BulkString(Some("testvalue".to_string())),
        ]))
    ).await;
    
    match set_response {
        RespValue::SimpleString(s) => assert_eq!(s, "OK"),
        _ => panic!("Expected OK"),
    }
    
    // Test GET command
    let get_response = handler.handle_command(
        RespValue::Array(Some(vec![
            RespValue::BulkString(Some("GET".to_string())),
            RespValue::BulkString(Some("testkey".to_string())),
        ]))
    ).await;
    
    match get_response {
        RespValue::BulkString(Some(s)) => assert_eq!(s, "testvalue"),
        _ => panic!("Expected testvalue"),
    }
}

#[tokio::test]
async fn test_multiple_clients() {
    let storage: Arc<dyn coral_redis::StorageBackend> = Arc::new(MemoryStorage::new());

    // Simulate multiple clients accessing the same storage
    let mut handler1 = Handler::new(Arc::clone(&storage));
    let mut handler2 = Handler::new(Arc::clone(&storage));
    
    // Client 1 sets a key
    handler1.handle_command(
        RespValue::Array(Some(vec![
            RespValue::BulkString(Some("SET".to_string())),
            RespValue::BulkString(Some("shared_key".to_string())),
            RespValue::BulkString(Some("shared_value".to_string())),
        ]))
    ).await;
    
    // Client 2 should be able to get the same key
    let response = handler2.handle_command(
        RespValue::Array(Some(vec![
            RespValue::BulkString(Some("GET".to_string())),
            RespValue::BulkString(Some("shared_key".to_string())),
        ]))
    ).await;
    
    match response {
        RespValue::BulkString(Some(s)) => assert_eq!(s, "shared_value"),
        _ => panic!("Expected shared_value"),
    }
}

#[tokio::test]
async fn test_error_handling() {
    let storage: Arc<dyn coral_redis::StorageBackend> = Arc::new(MemoryStorage::new());
    let mut handler = Handler::new(storage);

    // Test invalid command
    let response = handler.handle_command(
        RespValue::Array(Some(vec![
            RespValue::BulkString(Some("INVALID_COMMAND".to_string())),
        ]))
    ).await;

    match response {
        RespValue::Error(_) => {},
        _ => panic!("Expected error for invalid command"),
    }

    // Test invalid command format
    let response = handler.handle_command(
        RespValue::SimpleString("not_array".to_string())
    ).await;

    match response {
        RespValue::Error(_) => {},
        _ => panic!("Expected error for invalid format"),
    }
}

#[tokio::test]
async fn test_protocol_error_recovery() {
    use coral_redis::RespParser;

    let mut parser = RespParser::new();

    // Send invalid RESP data
    parser.add_data(b"$-5\r\n"); // Invalid bulk string length
    let result = parser.parse();
    assert!(result.is_err(), "Expected parse error for invalid length");

    // Reset parser to recover
    parser.reset();

    // Now send valid command - should work
    parser.add_data(b"*1\r\n$4\r\nPING\r\n");
    let result = parser.parse();
    assert!(result.is_ok(), "Parser should recover after reset");

    let value = result.unwrap().unwrap();
    match value {
        RespValue::Array(Some(parts)) => {
            assert_eq!(parts.len(), 1);
            match &parts[0] {
                RespValue::BulkString(Some(s)) => assert_eq!(s, "PING"),
                _ => panic!("Expected BulkString"),
            }
        }
        _ => panic!("Expected Array"),
    }
}

#[tokio::test]
async fn test_config_get_integration() {
    let storage: Arc<dyn StorageBackend> = Arc::new(MemoryStorage::new());
    let config = Arc::new(Config::default());
    let mut handler = Handler::new_with_config(storage, config);

    // Test CONFIG GET port
    let response = handler.handle_command(
        RespValue::Array(Some(vec![
            RespValue::BulkString(Some("CONFIG".to_string())),
            RespValue::BulkString(Some("GET".to_string())),
            RespValue::BulkString(Some("port".to_string())),
        ]))
    ).await;

    match response {
        RespValue::Array(Some(items)) => {
            assert_eq!(items.len(), 2);
            match (&items[0], &items[1]) {
                (RespValue::BulkString(Some(key)), RespValue::BulkString(Some(value))) => {
                    assert_eq!(key, "port");
                    assert_eq!(value, "6379");
                },
                _ => panic!("Expected BulkString pairs"),
            }
        },
        _ => panic!("Expected Array response"),
    }

    // Test CONFIG GET with multiple parameters
    let response = handler.handle_command(
        RespValue::Array(Some(vec![
            RespValue::BulkString(Some("CONFIG".to_string())),
            RespValue::BulkString(Some("GET".to_string())),
            RespValue::BulkString(Some("port".to_string())),
            RespValue::BulkString(Some("bind".to_string())),
        ]))
    ).await;

    match response {
        RespValue::Array(Some(items)) => {
            assert_eq!(items.len(), 4); // 2 key-value pairs
        },
        _ => panic!("Expected Array response"),
    }
}