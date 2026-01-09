use bytes::{Buf, BytesMut};
use std::io;

#[derive(Debug, Clone)]
pub enum RespValue {
    SimpleString(String),
    Error(String),
    Integer(i64),
    BulkString(Option<String>),
    Array(Option<Vec<RespValue>>),
}

impl RespValue {
    pub fn to_bytes(&self) -> Vec<u8> {
        match self {
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
        }
    }
}

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

    pub fn add_data(&mut self, data: &[u8]) {
        self.buffer.extend_from_slice(data);
    }

    pub fn parse(&mut self) -> Result<Option<RespValue>, io::Error> {
        if self.buffer.is_empty() {
            return Ok(None);
        }

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

    fn parse_value(&mut self) -> Result<Option<RespValue>, io::Error> {
        if self.buffer.is_empty() {
            return Ok(None);
        }

        let type_byte = self.buffer[0];
        self.buffer.advance(1);

        match type_byte {
            b'+' => self.parse_simple_string(),
            b'-' => self.parse_error(),
            b':' => self.parse_integer(),
            b'$' => self.parse_bulk_string(),
            b'*' => self.parse_array(),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid RESP type",
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
        let mut parser = RespParser::new();
        parser.add_data(b"&invalid\r\n");
        
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
}
