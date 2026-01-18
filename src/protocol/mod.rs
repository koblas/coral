//! Redis Serialization Protocol (RESP2/RESP3) and inline protocol support.

pub mod detector;
pub mod inline;
pub mod resp;

pub use detector::{detect_format, ProtocolFormat};
pub use inline::InlineParser;
pub use resp::*;

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
