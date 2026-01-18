use crate::error::ConfigError;
use clap::{Parser, ValueEnum};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "coral-redis")]
#[command(about = "A Redis-compatible server implemented in Rust")]
#[command(long_about = "Coral Redis is a high-performance Redis-compatible server with pluggable storage backends")]
#[command(version)]
pub struct Cli {
    /// Server host to bind to
    #[arg(short = 'H', long)]
    pub host: Option<String>,

    /// Server port to bind to
    #[arg(short, long)]
    pub port: Option<u16>,

    /// Storage backend to use
    #[arg(short, long)]
    pub storage: Option<StorageBackend>,

    /// Path for LMDB storage (required when using LMDB backend)
    #[arg(long)]
    pub lmdb_path: Option<PathBuf>,

    /// S3 bucket name (required when using S3 backend)
    #[arg(long)]
    pub s3_bucket: Option<String>,

    /// S3 key prefix (optional when using S3 backend)
    #[arg(long)]
    pub s3_prefix: Option<String>,

    /// AWS region (optional, uses default AWS config if not specified)
    #[arg(long)]
    pub aws_region: Option<String>,

    /// Configuration file path (JSON format)
    #[arg(short, long)]
    pub config: Option<PathBuf>,

    /// Enable verbose logging
    #[arg(short, long)]
    pub verbose: bool,

    /// Enable debug logging
    #[arg(short, long)]
    pub debug: bool,
}

#[derive(Debug, Clone, ValueEnum)]
pub enum StorageBackend {
    /// In-memory storage (default, fast but not persistent)
    Memory,
    /// LMDB storage (persistent, ACID transactions)
    Lmdb,
    /// AWS S3 storage (cloud-based, highly scalable)
    #[cfg(feature = "s3-backend")]
    S3,
}

impl std::fmt::Display for StorageBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StorageBackend::Memory => write!(f, "memory"),
            StorageBackend::Lmdb => write!(f, "lmdb"),
            #[cfg(feature = "s3-backend")]
            StorageBackend::S3 => write!(f, "s3"),
        }
    }
}

impl Cli {
    /// Parse command line arguments
    pub fn parse() -> Self {
        <Self as Parser>::parse()
    }

    /// Validate the configuration after merging with file/env.
    pub fn validate_for_storage(&self, storage: &StorageBackend) -> Result<(), ConfigError> {
        match storage {
            StorageBackend::Lmdb => {
                if self.lmdb_path.is_none() {
                    return Err(ConfigError::MissingField(
                        "lmdb_path is required when using LMDB backend".to_string(),
                    ));
                }
            }
            #[cfg(feature = "s3-backend")]
            StorageBackend::S3 => {
                if self.s3_bucket.is_none() {
                    return Err(ConfigError::MissingField(
                        "s3_bucket is required when using S3 backend".to_string(),
                    ));
                }
            }
            StorageBackend::Memory => {}
        }
        Ok(())
    }

    /// Get effective storage backend (CLI or default).
    pub fn effective_storage(&self) -> StorageBackend {
        self.storage.clone().unwrap_or(StorageBackend::Memory)
    }

    /// Print example usage for different backends
    pub fn print_examples() {
        println!("Examples:");
        println!("  # Start with memory backend (default)");
        println!("  {} --storage memory", env!("CARGO_PKG_NAME"));
        println!();

        println!("  # Start with LMDB backend");
        println!(
            "  {} --storage lmdb --lmdb-path ./data.lmdb",
            env!("CARGO_PKG_NAME")
        );
        println!();

        #[cfg(feature = "s3-backend")]
        {
            println!("  # Start with S3 backend");
            println!(
                "  {} --storage s3 --s3-bucket my-bucket --s3-prefix redis/",
                env!("CARGO_PKG_NAME")
            );
            println!();
        }

        println!("  # Custom host and port");
        println!("  {} --host 0.0.0.0 --port 6380", env!("CARGO_PKG_NAME"));
        println!();

        println!("  # Load from config file");
        println!("  {} --config config.json", env!("CARGO_PKG_NAME"));
        println!();

        println!("  # Verbose logging");
        println!("  {} --verbose", env!("CARGO_PKG_NAME"));
    }
}