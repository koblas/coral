use serde::{Deserialize, Serialize};
#[cfg(feature = "lmdb-backend")]
use std::path::PathBuf;

/// Main configuration combining server and storage settings.
///
/// Can be loaded from files, env vars, or CLI args with precedence order:
/// CLI > File > Environment > Defaults
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub storage: StorageConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "backend", rename_all = "lowercase")]
pub enum StorageConfig {
    Memory,
    #[cfg(feature = "lmdb-backend")]
    Lmdb { 
        path: PathBuf,
    },
    #[cfg(feature = "s3-backend")]
    S3 {
        bucket: String,
        prefix: Option<String>,
        region: Option<String>,
    },
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 6379,
            },
            storage: StorageConfig::Memory,
        }
    }
}

impl Config {
    /// Create config from environment variables (legacy support)
    pub fn from_env() -> Self {
        let mut config = Self::default();
        
        if let Ok(host) = std::env::var("REDIS_HOST") {
            config.server.host = host;
        }
        
        if let Ok(port_str) = std::env::var("REDIS_PORT") {
            if let Ok(port) = port_str.parse() {
                config.server.port = port;
            }
        }
        
        // Storage backend selection from environment
        match std::env::var("STORAGE_BACKEND").as_deref() {
            Ok("memory") => config.storage = StorageConfig::Memory,
            #[cfg(feature = "lmdb-backend")]
            Ok("lmdb") => {
                let path = std::env::var("LMDB_PATH")
                    .unwrap_or_else(|_| "./data.lmdb".to_string());
                config.storage = StorageConfig::Lmdb { 
                    path: PathBuf::from(path) 
                };
            },
            #[cfg(feature = "s3-backend")]
            Ok("s3") => {
                let bucket = std::env::var("S3_BUCKET")
                    .expect("S3_BUCKET environment variable required for S3 backend");
                let prefix = std::env::var("S3_PREFIX").ok();
                let region = std::env::var("AWS_REGION").ok();
                config.storage = StorageConfig::S3 { bucket, prefix, region };
            },
            _ => {
                // Default to memory if not specified
                config.storage = StorageConfig::Memory;
            }
        }
        
        config
    }

    /// Create config with CLI args taking precedence over environment and file
    pub fn from_sources(cli: &crate::cli::Cli) -> Result<Self, Box<dyn std::error::Error>> {
        // Start with CLI config
        let mut config = cli.to_config();

        // If a config file is specified, merge it as base (CLI overrides file)
        if let Some(config_path) = &cli.config {
            match Self::load_from_file(config_path) {
                Ok(file_config) => {
                    // Only use file values if CLI didn't specify them
                    if cli.host == "127.0.0.1" {
                        config.server.host = file_config.server.host;
                    }
                    if cli.port == 6379 {
                        config.server.port = file_config.server.port;
                    }
                    // Storage backend from CLI always takes precedence
                }
                Err(e) => {
                    eprintln!("Warning: Failed to load config file: {}", e);
                }
            }
        }

        // Environment variables as fallback for unspecified CLI options
        if cli.host == "127.0.0.1" {
            if let Ok(host) = std::env::var("REDIS_HOST") {
                config.server.host = host;
            }
        }
        
        if cli.port == 6379 {
            if let Ok(port_str) = std::env::var("REDIS_PORT") {
                if let Ok(port) = port_str.parse() {
                    config.server.port = port;
                }
            }
        }

        Ok(config)
    }
    
    pub fn load_from_file<P: AsRef<std::path::Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let contents = std::fs::read_to_string(path)?;
        let config: Self = serde_json::from_str(&contents)?;
        Ok(config)
    }
    
    pub fn save_to_file<P: AsRef<std::path::Path>>(&self, path: P) -> Result<(), Box<dyn std::error::Error>> {
        let contents = serde_json::to_string_pretty(self)?;
        std::fs::write(path, contents)?;
        Ok(())
    }
}