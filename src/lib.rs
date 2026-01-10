//! Coral Redis - A Redis-compatible server with pluggable storage backends.
//!
//! This library provides a Redis protocol implementation with support for
//! multiple storage backends (Memory, LMDB, S3) and comprehensive observability.

pub mod cli;
pub mod config;
pub mod metrics;
pub mod protocol;
pub mod server;
pub mod storage;
pub mod telemetry;

pub use server::Handler;
pub use protocol::{RespParser, RespValue};
pub use storage::{StorageBackend, StorageError, StorageFactory};