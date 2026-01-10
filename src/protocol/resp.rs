use bytes::{Buf, BytesMut};
use std::io;
use super::{InlineParser, detect_format, ProtocolFormat};

/// RESP (Redis Serialization Protocol) value types.
///
/// Supports RESP2 and RESP3 core types including null variants.
#[derive(Debug, Clone)]
pub enum RespValue {
    // RESP2 types
    SimpleString(String),           // +
    Error(String),                  // -
    Integer(i64),                   // :
    BulkString(Option<String>),     // $ (None = null in RESP2)
    Array(Option<Vec<RespValue>>),  // * (None = null in RESP2)

    // RESP3 types
    Null,                          // _ (dedicated null type)
    Boolean(bool),                 // # (true/false)
    Double(f64),                   // , (floating point)
    Set(Vec<RespValue>),          // ~ (unordered collection)
    Map(Vec<(RespValue, RespValue)>), // % (key-value pairs)
}

impl RespValue {
    /// Serialize this value to Redis wire format.
    pub fn to_bytes(&self) -> Vec<u8> {
        match self {
            // RESP2 types
            RespValue::SimpleString(s) => format!("+{}\r\n", s).into_bytes(),
            RespValue::Error(e) => format!("-{}\r\n", e).into_bytes(),
            RespValue::Integer(i) => format!(":{}\r\n", i).into_bytes(),
            RespValue::BulkString(Some(s)) => format!("${}\r\n{}\r\n", s.len(), s).into_bytes(),
            RespValue::BulkString(None) => b"$-1\r\n".to_vec(),
            RespValue::Array(Some(arr)) => {
                let mut result = format!("*{}\r\n", arr.len()).into_bytes();
                for item in arr {
                    result.extend_from_slice(&item.to_bytes());
                }
                result
            }
            RespValue::Array(None) => b"*-1\r\n".to_vec(),

            // RESP3 types
            RespValue::Null => b"_\r\n".to_vec(),
            RespValue::Boolean(true) => b"#t\r\n".to_vec(),
            RespValue::Boolean(false) => b"#f\r\n".to_vec(),
            RespValue::Double(d) => format!(",{}\r\n", d).into_bytes(),
            RespValue::Set(items) => {
                let mut result = format!("~{}\r\n", items.len()).into_bytes();
                for item in items {
                    result.extend_from_slice(&item.to_bytes());
                }
                result
            }
            RespValue::Map(pairs) => {
                let mut result = format!("%{}\r\n", pairs.len()).into_bytes();
                for (key, value) in pairs {
                    result.extend_from_slice(&key.to_bytes());
                    result.extend_from_slice(&value.to_bytes());
                }
                result
            }
        }
    }
}

/// Stateful parser for Redis protocol messages.
///
/// Accumulates data in a buffer and parses complete RESP values.
pub struct RespParser {
    buffer: BytesMut,
}

impl Default for RespParser {
    fn default() -> Self {
        Self::new()
    }
}

impl RespParser {
    pub fn new() -> Self {
        Self {
            buffer: BytesMut::with_capacity(4096),
        }
    }

    /// Add incoming bytes to the parser buffer.
    pub fn add_data(&mut self, data: &[u8]) {
        self.buffer.extend_from_slice(data);
    }

    /// Reset the parser buffer to recover from errors.
    /// Clears the buffer to allow processing of subsequent messages.
    pub fn reset(&mut self) {
        self.buffer.clear();
    }

    /// Parse next complete value. Returns None if incomplete.
    /// Auto-detects inline (telnet) vs RESP format.
    pub fn parse(&mut self) -> Result<Option<RespValue>, io::Error> {
        if self.buffer.is_empty() {
            return Ok(None);
        }

        // Detect protocol format
        let format = detect_format(&self.buffer);

        match format {
            Some(ProtocolFormat::Inline) => {
                // Try inline parsing
                match InlineParser::parse(&self.buffer)? {
                    Some(value) => {
                        // Consume the parsed bytes
                        let consumed = InlineParser::bytes_consumed(&self.buffer);
                        self.buffer.advance(consumed);
                        Ok(Some(value))
                    }
                    None => Ok(None), // Incomplete
                }
            }
            Some(ProtocolFormat::Resp) => {
                // RESP parsing (existing logic)
                let original_len = self.buffer.len();
                match self.parse_value() {
                    Ok(Some(value)) => Ok(Some(value)),
                    Ok(None) => {
                        // Reset buffer if we couldn't parse
                        self.buffer.truncate(original_len);
                        Ok(None)
                    }
                    Err(e) => Err(e),
                }
            }
            None => Ok(None), // Empty buffer
        }
    }

    fn parse_value(&mut self) -> Result<Option<RespValue>, io::Error> {
        if self.buffer.is_empty() {
            return Ok(None);
        }

        tracing::trace!("Parsing buffer: {} bytes", self.buffer.len());

        let type_byte = self.buffer[0];
        self.buffer.advance(1);

        match type_byte {
            // RESP2 types
            b'+' => self.parse_simple_string(),
            b'-' => self.parse_error(),
            b':' => self.parse_integer(),
            b'$' => self.parse_bulk_string(),
            b'*' => self.parse_array(),

            // RESP3 types
            b'_' => self.parse_null(),
            b'#' => self.parse_boolean(),
            b',' => self.parse_double(),
            b'~' => self.parse_set(),
            b'%' => self.parse_map(),

            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid RESP type byte: {}", type_byte as char),
            )),
        }
    }

    fn parse_simple_string(&mut self) -> Result<Option<RespValue>, io::Error> {
        if let Some(line) = self.read_line()? {
            Ok(Some(RespValue::SimpleString(line)))
        } else {
            Ok(None)
        }
    }

    fn parse_error(&mut self) -> Result<Option<RespValue>, io::Error> {
        if let Some(line) = self.read_line()? {
            Ok(Some(RespValue::Error(line)))
        } else {
            Ok(None)
        }
    }

    fn parse_integer(&mut self) -> Result<Option<RespValue>, io::Error> {
        if let Some(line) = self.read_line()? {
            let num = line
                .parse::<i64>()
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Invalid integer"))?;
            Ok(Some(RespValue::Integer(num)))
        } else {
            Ok(None)
        }
    }

    fn parse_bulk_string(&mut self) -> Result<Option<RespValue>, io::Error> {
        if let Some(length_str) = self.read_line()? {
            let length = length_str.parse::<i64>().map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidData, "Invalid bulk string length")
            })?;

            if length == -1 {
                return Ok(Some(RespValue::BulkString(None)));
            }

            if length < 0 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Invalid bulk string length",
                ));
            }

            let length = length as usize;
            if self.buffer.len() < length + 2 {
                return Ok(None); // Not enough data
            }

            let data = self.buffer.split_to(length);
            let string = String::from_utf8(data.to_vec())
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Invalid UTF-8"))?;

            // Skip \r\n
            if self.buffer.len() >= 2 && &self.buffer[0..2] == b"\r\n" {
                self.buffer.advance(2);
            }

            Ok(Some(RespValue::BulkString(Some(string))))
        } else {
            Ok(None)
        }
    }

    fn parse_array(&mut self) -> Result<Option<RespValue>, io::Error> {
        if let Some(length_str) = self.read_line()? {
            let length = length_str
                .parse::<i64>()
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Invalid array length"))?;

            if length == -1 {
                return Ok(Some(RespValue::Array(None)));
            }

            if length < 0 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Invalid array length",
                ));
            }

            let mut elements = Vec::with_capacity(length as usize);
            for _ in 0..length {
                if let Some(element) = self.parse_value()? {
                    elements.push(element);
                } else {
                    return Ok(None); // Not enough data
                }
            }

            Ok(Some(RespValue::Array(Some(elements))))
        } else {
            Ok(None)
        }
    }

    fn read_line(&mut self) -> Result<Option<String>, io::Error> {
        if let Some(pos) = self.buffer.windows(2).position(|w| w == b"\r\n") {
            let line_data = self.buffer.split_to(pos);
            self.buffer.advance(2); // Skip \r\n

            let line = String::from_utf8(line_data.to_vec())
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Invalid UTF-8"))?;

            Ok(Some(line))
        } else {
            Ok(None)
        }
    }

    // RESP3 parsers
    fn parse_null(&mut self) -> Result<Option<RespValue>, io::Error> {
        // Null is just _\r\n
        if self.buffer.len() >= 2 && &self.buffer[0..2] == b"\r\n" {
            self.buffer.advance(2);
            Ok(Some(RespValue::Null))
        } else {
            Ok(None)
        }
    }

    fn parse_boolean(&mut self) -> Result<Option<RespValue>, io::Error> {
        // Boolean is #t\r\n or #f\r\n
        if self.buffer.len() >= 3 {
            let value = match self.buffer[0] {
                b't' => true,
                b'f' => false,
                c => return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Invalid boolean value: {}", c as char),
                )),
            };

            if &self.buffer[1..3] == b"\r\n" {
                self.buffer.advance(3);
                Ok(Some(RespValue::Boolean(value)))
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    fn parse_double(&mut self) -> Result<Option<RespValue>, io::Error> {
        if let Some(line) = self.read_line()? {
            let num = line.parse::<f64>().map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidData, "Invalid double")
            })?;
            Ok(Some(RespValue::Double(num)))
        } else {
            Ok(None)
        }
    }

    fn parse_set(&mut self) -> Result<Option<RespValue>, io::Error> {
        if let Some(length_str) = self.read_line()? {
            let length = length_str.parse::<i64>().map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidData, "Invalid set length")
            })?;

            if length < 0 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Invalid set length",
                ));
            }

            let mut elements = Vec::with_capacity(length as usize);
            for _ in 0..length {
                if let Some(element) = self.parse_value()? {
                    elements.push(element);
                } else {
                    return Ok(None); // Not enough data
                }
            }

            Ok(Some(RespValue::Set(elements)))
        } else {
            Ok(None)
        }
    }

    fn parse_map(&mut self) -> Result<Option<RespValue>, io::Error> {
        if let Some(length_str) = self.read_line()? {
            let length = length_str.parse::<i64>().map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidData, "Invalid map length")
            })?;

            if length < 0 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Invalid map length",
                ));
            }

            let mut pairs = Vec::with_capacity(length as usize);
            for _ in 0..length {
                let key = if let Some(k) = self.parse_value()? {
                    k
                } else {
                    return Ok(None); // Not enough data
                };

                let value = if let Some(v) = self.parse_value()? {
                    v
                } else {
                    return Ok(None); // Not enough data
                };

                pairs.push((key, value));
            }

            Ok(Some(RespValue::Map(pairs)))
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_string_serialization() {
        let value = RespValue::SimpleString("OK".to_string());
        assert_eq!(value.to_bytes(), b"+OK\r\n");
    }

    #[test]
    fn test_error_serialization() {
        let value = RespValue::Error("ERROR message".to_string());
        assert_eq!(value.to_bytes(), b"-ERROR message\r\n");
    }

    #[test]
    fn test_integer_serialization() {
        let value = RespValue::Integer(42);
        assert_eq!(value.to_bytes(), b":42\r\n");
    }

    #[test]
    fn test_bulk_string_serialization() {
        let value = RespValue::BulkString(Some("hello".to_string()));
        assert_eq!(value.to_bytes(), b"$5\r\nhello\r\n");
    }

    #[test]
    fn test_null_bulk_string_serialization() {
        let value = RespValue::BulkString(None);
        assert_eq!(value.to_bytes(), b"$-1\r\n");
    }

    #[test]
    fn test_array_serialization() {
        let value = RespValue::Array(Some(vec![
            RespValue::SimpleString("OK".to_string()),
            RespValue::Integer(42),
        ]));
        assert_eq!(value.to_bytes(), b"*2\r\n+OK\r\n:42\r\n");
    }

    #[test]
    fn test_simple_string_parsing() {
        let mut parser = RespParser::new();
        parser.add_data(b"+OK\r\n");
        
        let result = parser.parse().unwrap().unwrap();
        match result {
            RespValue::SimpleString(s) => assert_eq!(s, "OK"),
            _ => panic!("Expected SimpleString"),
        }
    }

    #[test]
    fn test_integer_parsing() {
        let mut parser = RespParser::new();
        parser.add_data(b":42\r\n");
        
        let result = parser.parse().unwrap().unwrap();
        match result {
            RespValue::Integer(i) => assert_eq!(i, 42),
            _ => panic!("Expected Integer"),
        }
    }

    #[test]
    fn test_bulk_string_parsing() {
        let mut parser = RespParser::new();
        parser.add_data(b"$5\r\nhello\r\n");
        
        let result = parser.parse().unwrap().unwrap();
        match result {
            RespValue::BulkString(Some(s)) => assert_eq!(s, "hello"),
            _ => panic!("Expected BulkString"),
        }
    }

    #[test]
    fn test_null_bulk_string_parsing() {
        let mut parser = RespParser::new();
        parser.add_data(b"$-1\r\n");
        
        let result = parser.parse().unwrap().unwrap();
        match result {
            RespValue::BulkString(None) => {},
            _ => panic!("Expected null BulkString"),
        }
    }

    #[test]
    fn test_array_parsing() {
        let mut parser = RespParser::new();
        parser.add_data(b"*2\r\n+OK\r\n:42\r\n");
        
        let result = parser.parse().unwrap().unwrap();
        match result {
            RespValue::Array(Some(arr)) => {
                assert_eq!(arr.len(), 2);
                match &arr[0] {
                    RespValue::SimpleString(s) => assert_eq!(s, "OK"),
                    _ => panic!("Expected SimpleString"),
                }
                match &arr[1] {
                    RespValue::Integer(i) => assert_eq!(*i, 42),
                    _ => panic!("Expected Integer"),
                }
            },
            _ => panic!("Expected Array"),
        }
    }

    #[test]
    fn test_incomplete_data() {
        let mut parser = RespParser::new();
        parser.add_data(b"+OK");
        
        let result = parser.parse().unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_invalid_type() {
        // With auto-detection, "&invalid" is treated as inline command
        // To test invalid RESP, we need malformed RESP structure
        let mut parser = RespParser::new();
        parser.add_data(b"$-5\r\n"); // Invalid negative bulk string length (not -1)

        let result = parser.parse();
        assert!(result.is_err());
    }

    #[test]
    fn test_multiple_commands() {
        let mut parser = RespParser::new();
        parser.add_data(b"+OK\r\n:42\r\n");
        
        let first = parser.parse().unwrap().unwrap();
        let second = parser.parse().unwrap().unwrap();
        
        match first {
            RespValue::SimpleString(s) => assert_eq!(s, "OK"),
            _ => panic!("Expected SimpleString"),
        }
        
        match second {
            RespValue::Integer(i) => assert_eq!(i, 42),
            _ => panic!("Expected Integer"),
        }
    }

    // RESP3 serialization tests
    #[test]
    fn test_null_serialization() {
        let value = RespValue::Null;
        assert_eq!(value.to_bytes(), b"_\r\n");
    }

    #[test]
    fn test_boolean_serialization() {
        let value_true = RespValue::Boolean(true);
        assert_eq!(value_true.to_bytes(), b"#t\r\n");

        let value_false = RespValue::Boolean(false);
        assert_eq!(value_false.to_bytes(), b"#f\r\n");
    }

    #[test]
    fn test_double_serialization() {
        let value = RespValue::Double(3.14);
        assert_eq!(value.to_bytes(), b",3.14\r\n");

        let value_neg = RespValue::Double(-2.5);
        assert_eq!(value_neg.to_bytes(), b",-2.5\r\n");
    }

    #[test]
    fn test_set_serialization() {
        let value = RespValue::Set(vec![
            RespValue::SimpleString("hello".to_string()),
            RespValue::Integer(42),
        ]);
        assert_eq!(value.to_bytes(), b"~2\r\n+hello\r\n:42\r\n");
    }

    #[test]
    fn test_map_serialization() {
        let value = RespValue::Map(vec![
            (RespValue::SimpleString("key1".to_string()), RespValue::Integer(1)),
            (RespValue::SimpleString("key2".to_string()), RespValue::Integer(2)),
        ]);
        assert_eq!(value.to_bytes(), b"%2\r\n+key1\r\n:1\r\n+key2\r\n:2\r\n");
    }

    // RESP3 parsing tests
    #[test]
    fn test_null_parsing() {
        let mut parser = RespParser::new();
        parser.add_data(b"_\r\n");

        let result = parser.parse().unwrap().unwrap();
        match result {
            RespValue::Null => {},
            _ => panic!("Expected Null"),
        }
    }

    #[test]
    fn test_boolean_parsing() {
        let mut parser = RespParser::new();
        parser.add_data(b"#t\r\n");

        let result = parser.parse().unwrap().unwrap();
        match result {
            RespValue::Boolean(true) => {},
            _ => panic!("Expected Boolean(true)"),
        }

        let mut parser2 = RespParser::new();
        parser2.add_data(b"#f\r\n");

        let result2 = parser2.parse().unwrap().unwrap();
        match result2 {
            RespValue::Boolean(false) => {},
            _ => panic!("Expected Boolean(false)"),
        }
    }

    #[test]
    fn test_double_parsing() {
        let mut parser = RespParser::new();
        parser.add_data(b",3.14\r\n");

        let result = parser.parse().unwrap().unwrap();
        match result {
            RespValue::Double(d) => assert_eq!(d, 3.14),
            _ => panic!("Expected Double"),
        }
    }

    #[test]
    fn test_set_parsing() {
        let mut parser = RespParser::new();
        parser.add_data(b"~2\r\n+hello\r\n:42\r\n");

        let result = parser.parse().unwrap().unwrap();
        match result {
            RespValue::Set(items) => {
                assert_eq!(items.len(), 2);
                match &items[0] {
                    RespValue::SimpleString(s) => assert_eq!(s, "hello"),
                    _ => panic!("Expected SimpleString"),
                }
                match &items[1] {
                    RespValue::Integer(i) => assert_eq!(*i, 42),
                    _ => panic!("Expected Integer"),
                }
            },
            _ => panic!("Expected Set"),
        }
    }

    #[test]
    fn test_map_parsing() {
        let mut parser = RespParser::new();
        parser.add_data(b"%2\r\n+key1\r\n:1\r\n+key2\r\n:2\r\n");

        let result = parser.parse().unwrap().unwrap();
        match result {
            RespValue::Map(pairs) => {
                assert_eq!(pairs.len(), 2);
                match (&pairs[0].0, &pairs[0].1) {
                    (RespValue::SimpleString(k), RespValue::Integer(v)) => {
                        assert_eq!(k, "key1");
                        assert_eq!(*v, 1);
                    },
                    _ => panic!("Expected SimpleString key and Integer value"),
                }
            },
            _ => panic!("Expected Map"),
        }
    }

    #[test]
    fn test_nested_resp3_types() {
        // Test nested map with sets as values
        let mut parser = RespParser::new();
        parser.add_data(b"%1\r\n+mykey\r\n~2\r\n+val1\r\n+val2\r\n");

        let result = parser.parse().unwrap().unwrap();
        match result {
            RespValue::Map(pairs) => {
                assert_eq!(pairs.len(), 1);
                match (&pairs[0].0, &pairs[0].1) {
                    (RespValue::SimpleString(k), RespValue::Set(items)) => {
                        assert_eq!(k, "mykey");
                        assert_eq!(items.len(), 2);
                    },
                    _ => panic!("Expected SimpleString and Set"),
                }
            },
            _ => panic!("Expected Map"),
        }
    }

    // Inline command integration tests
    #[test]
    fn test_inline_command_integration() {
        let mut parser = RespParser::new();
        parser.add_data(b"PING\r\n");

        let result = parser.parse().unwrap().unwrap();
        match result {
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

    #[test]
    fn test_inline_set_command() {
        let mut parser = RespParser::new();
        parser.add_data(b"SET mykey myvalue\r\n");

        let result = parser.parse().unwrap().unwrap();
        match result {
            RespValue::Array(Some(parts)) => {
                assert_eq!(parts.len(), 3);
            }
            _ => panic!("Expected Array"),
        }
    }

    #[test]
    fn test_mixed_inline_and_resp() {
        let mut parser = RespParser::new();
        // First an inline command
        parser.add_data(b"PING\r\n");
        let first = parser.parse().unwrap().unwrap();
        match first {
            RespValue::Array(_) => {}, // Inline becomes array
            _ => panic!("Expected Array from inline"),
        }

        // Then a RESP command
        parser.add_data(b"*1\r\n$4\r\nPING\r\n");
        let second = parser.parse().unwrap().unwrap();
        match second {
            RespValue::Array(_) => {},
            _ => panic!("Expected Array from RESP"),
        }
    }

    #[test]
    fn test_parser_reset() {
        let mut parser = RespParser::new();

        // Add some invalid data
        parser.add_data(b"$-5\r\n"); // Invalid bulk string length
        assert!(parser.parse().is_err());

        // Reset parser
        parser.reset();

        // Should be able to parse new command
        parser.add_data(b"+OK\r\n");
        let result = parser.parse().unwrap();
        assert!(matches!(result, Some(RespValue::SimpleString(_))));
    }

    #[test]
    fn test_error_recovery() {
        let mut parser = RespParser::new();

        // Parse error with invalid length
        parser.add_data(b"$-5\r\n");
        let result = parser.parse();
        assert!(result.is_err());

        // Clear buffer manually (simulating reset)
        parser.reset();

        // Next valid command should parse correctly
        parser.add_data(b"*1\r\n$4\r\nPING\r\n");
        let result = parser.parse().unwrap();
        assert!(matches!(result, Some(RespValue::Array(_))));
    }
}
