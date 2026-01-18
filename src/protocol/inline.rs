//! Inline command parser for Redis protocol.
//!
//! Supports telnet-style space-separated commands like "PING" or "SET key value".

use super::RespValue;
use std::io;

/// Parser for inline (telnet-style) Redis commands.
///
/// Converts space-separated text commands into RESP Array format.
pub struct InlineParser;

impl InlineParser {
    /// Parse inline command format: "COMMAND arg1 arg2\r\n"
    /// Returns RESP Array of BulkStrings.
    pub fn parse(buffer: &[u8]) -> Result<Option<RespValue>, io::Error> {
        // Find \r\n terminator
        let end_pos = buffer.windows(2).position(|w| w == b"\r\n");

        let end_pos = match end_pos {
            Some(pos) => pos,
            None => return Ok(None), // Incomplete command
        };

        let line_data = &buffer[0..end_pos];

        // Convert to string without allocating a vec
        let line = std::str::from_utf8(line_data)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Invalid UTF-8 in inline command"))?;

        // Parse the command line
        let parts = Self::parse_command_line(line)?;

        if parts.is_empty() {
            return Ok(None); // Empty command
        }

        // Convert to RESP Array of BulkStrings
        let resp_array: Vec<RespValue> = parts
            .into_iter()
            .map(|s| RespValue::BulkString(Some(s)))
            .collect();

        Ok(Some(RespValue::Array(Some(resp_array))))
    }

    /// Parse command line handling quoted strings.
    fn parse_command_line(line: &str) -> Result<Vec<String>, io::Error> {
        let mut parts = Vec::new();
        let mut current = String::new();
        let mut in_quotes = false;
        let mut chars = line.chars().peekable();

        while let Some(ch) = chars.next() {
            match ch {
                '"' if !in_quotes => {
                    in_quotes = true;
                }
                '"' if in_quotes => {
                    in_quotes = false;
                }
                ' ' | '\t' if !in_quotes => {
                    if !current.is_empty() {
                        parts.push(std::mem::take(&mut current));
                    }
                }
                '\\' if in_quotes => {
                    // Handle escape sequences in quotes
                    if let Some(next_ch) = chars.next() {
                        match next_ch {
                            'n' => current.push('\n'),
                            'r' => current.push('\r'),
                            't' => current.push('\t'),
                            '"' => current.push('"'),
                            '\\' => current.push('\\'),
                            _ => {
                                current.push('\\');
                                current.push(next_ch);
                            }
                        }
                    }
                }
                _ => {
                    current.push(ch);
                }
            }
        }

        // Add final part
        if !current.is_empty() {
            parts.push(current);
        }

        // Check for unclosed quotes
        if in_quotes {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Unclosed quote in inline command",
            ));
        }

        Ok(parts)
    }

    /// Get the number of bytes consumed by a successful parse.
    /// Returns the position after \r\n, or 0 if command is incomplete.
    pub fn bytes_consumed(buffer: &[u8]) -> usize {
        if let Some(pos) = buffer.windows(2).position(|w| w == b"\r\n") {
            pos + 2
        } else {
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_command() {
        let result = InlineParser::parse(b"PING\r\n").unwrap().unwrap();
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
    fn test_command_with_args() {
        let result = InlineParser::parse(b"SET mykey myvalue\r\n").unwrap().unwrap();
        match result {
            RespValue::Array(Some(parts)) => {
                assert_eq!(parts.len(), 3);
                match &parts[0] {
                    RespValue::BulkString(Some(s)) => assert_eq!(s, "SET"),
                    _ => panic!("Expected BulkString"),
                }
                match &parts[1] {
                    RespValue::BulkString(Some(s)) => assert_eq!(s, "mykey"),
                    _ => panic!("Expected BulkString"),
                }
                match &parts[2] {
                    RespValue::BulkString(Some(s)) => assert_eq!(s, "myvalue"),
                    _ => panic!("Expected BulkString"),
                }
            }
            _ => panic!("Expected Array"),
        }
    }

    #[test]
    fn test_quoted_argument() {
        let result = InlineParser::parse(b"SET key \"value with spaces\"\r\n")
            .unwrap()
            .unwrap();
        match result {
            RespValue::Array(Some(parts)) => {
                assert_eq!(parts.len(), 3);
                match &parts[2] {
                    RespValue::BulkString(Some(s)) => assert_eq!(s, "value with spaces"),
                    _ => panic!("Expected BulkString"),
                }
            }
            _ => panic!("Expected Array"),
        }
    }

    #[test]
    fn test_escape_sequences() {
        let result = InlineParser::parse(b"SET key \"line1\\nline2\"\r\n")
            .unwrap()
            .unwrap();
        match result {
            RespValue::Array(Some(parts)) => {
                match &parts[2] {
                    RespValue::BulkString(Some(s)) => assert_eq!(s, "line1\nline2"),
                    _ => panic!("Expected BulkString"),
                }
            }
            _ => panic!("Expected Array"),
        }
    }

    #[test]
    fn test_incomplete_command() {
        let result = InlineParser::parse(b"PING").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_unclosed_quote() {
        let result = InlineParser::parse(b"SET key \"unclosed\r\n");
        assert!(result.is_err());
    }

    #[test]
    fn test_multiple_spaces() {
        let result = InlineParser::parse(b"SET   key    value\r\n").unwrap().unwrap();
        match result {
            RespValue::Array(Some(parts)) => {
                assert_eq!(parts.len(), 3);
            }
            _ => panic!("Expected Array"),
        }
    }

    #[test]
    fn test_tabs() {
        let result = InlineParser::parse(b"SET\tkey\tvalue\r\n").unwrap().unwrap();
        match result {
            RespValue::Array(Some(parts)) => {
                assert_eq!(parts.len(), 3);
            }
            _ => panic!("Expected Array"),
        }
    }

    #[test]
    fn test_bytes_consumed() {
        assert_eq!(InlineParser::bytes_consumed(b"PING\r\n"), 6);
        assert_eq!(InlineParser::bytes_consumed(b"SET key val\r\n"), 13);
        assert_eq!(InlineParser::bytes_consumed(b"PING"), 0);
    }
}
