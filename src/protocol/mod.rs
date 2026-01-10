//! Redis Serialization Protocol (RESP2/RESP3) and inline protocol support.

pub mod resp;
pub mod inline;
pub mod detector;

pub use resp::*;
pub use inline::InlineParser;
pub use detector::{detect_format, ProtocolFormat};

/// Protocol version for RESP communication.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolVersion {
    /// RESP2 - Original Redis protocol
    Resp2,
    /// RESP3 - Enhanced protocol with additional types
    Resp3,
}

impl Default for ProtocolVersion {
    fn default() -> Self {
        Self::Resp2
    }
}