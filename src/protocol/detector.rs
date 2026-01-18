//! Protocol format detection for auto-detecting inline vs RESP commands.

/// Detected protocol format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolFormat {
    /// RESP format (starts with special byte)
    Resp,
    /// Inline/telnet format (plain text)
    Inline,
}

/// Detect the protocol format from the first byte of the buffer.
///
/// RESP messages start with: +, -, :, $, *, _, #, ,, ~, %
/// Inline messages start with any other byte (typically letters).
pub fn detect_format(buffer: &[u8]) -> Option<ProtocolFormat> {
    if buffer.is_empty() {
        return None;
    }

    match buffer[0] {
        // RESP2 type bytes
        b'+' | b'-' | b':' | b'$' | b'*' |
        // RESP3 type bytes
        b'_' | b'#' | b',' | b'~' | b'%' => Some(ProtocolFormat::Resp),
        // Everything else is inline
        _ => Some(ProtocolFormat::Inline),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_resp2() {
        assert_eq!(detect_format(b"+OK\r\n"), Some(ProtocolFormat::Resp));
        assert_eq!(detect_format(b"-Error\r\n"), Some(ProtocolFormat::Resp));
        assert_eq!(detect_format(b":42\r\n"), Some(ProtocolFormat::Resp));
        assert_eq!(
            detect_format(b"$5\r\nhello\r\n"),
            Some(ProtocolFormat::Resp)
        );
        assert_eq!(detect_format(b"*2\r\n"), Some(ProtocolFormat::Resp));
    }

    #[test]
    fn test_detect_resp3() {
        assert_eq!(detect_format(b"_\r\n"), Some(ProtocolFormat::Resp));
        assert_eq!(detect_format(b"#t\r\n"), Some(ProtocolFormat::Resp));
        assert_eq!(detect_format(b",3.14\r\n"), Some(ProtocolFormat::Resp));
        assert_eq!(detect_format(b"~2\r\n"), Some(ProtocolFormat::Resp));
        assert_eq!(detect_format(b"%2\r\n"), Some(ProtocolFormat::Resp));
    }

    #[test]
    fn test_detect_inline() {
        assert_eq!(detect_format(b"PING\r\n"), Some(ProtocolFormat::Inline));
        assert_eq!(
            detect_format(b"SET key value\r\n"),
            Some(ProtocolFormat::Inline)
        );
        assert_eq!(
            detect_format(b"GET mykey\r\n"),
            Some(ProtocolFormat::Inline)
        );
    }

    #[test]
    fn test_detect_empty() {
        assert_eq!(detect_format(b""), None);
    }
}
