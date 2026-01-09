use coral_redis::{Handler, RespValue, StorageBackend};
use coral_redis::storage::memory::MemoryStorage;
use std::sync::Arc;

#[tokio::test]
async fn test_server_integration_basic_commands() {
    // Start server in background
    let storage: Arc<dyn StorageBackend> = Arc::new(MemoryStorage::new());
    let handler = Handler::new(storage);
    
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
    let handler1 = Handler::new(Arc::clone(&storage));
    let handler2 = Handler::new(Arc::clone(&storage));
    
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
    let handler = Handler::new(storage);
    
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