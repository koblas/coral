use crate::cli::{Cli, StorageBackend as CliStorageBackend};
use crate::error::ConfigError;
use serde::{Deserialize, Serialize};
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ServerConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
}

fn default_host() -> String {
    "127.0.0.1".to_string()
}

fn default_port() -> u16 {
    6379
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "backend", rename_all = "lowercase")]
pub enum StorageConfig {
    Memory,
    Lmdb { path: PathBuf },
    #[cfg(feature = "s3-backend")]
    S3 {
        bucket: String,
        prefix: Option<String>,
        region: Option<String>,
    },
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self::Memory
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerConfig {
                host: default_host(),
                port: default_port(),
            },
            storage: StorageConfig::Memory,
        }
    }
}

impl Config {
    /// Create config from environment variables.
    pub fn from_env() -> Result<Self, ConfigError> {
        let mut config = Self::default();

        if let Ok(host) = std::env::var("REDIS_HOST") {
            config.server.host = host;
        }

        if let Ok(port_str) = std::env::var("REDIS_PORT") {
            if let Ok(port) = port_str.parse() {
                config.server.port = port;
            }
        }

        match std::env::var("STORAGE_BACKEND").as_deref() {
            Ok("memory") => config.storage = StorageConfig::Memory,
            Ok("lmdb") => {
                let path = std::env::var("LMDB_PATH").unwrap_or_else(|_| "./data.lmdb".to_string());
                config.storage = StorageConfig::Lmdb {
                    path: PathBuf::from(path),
                };
            }
            #[cfg(feature = "s3-backend")]
            Ok("s3") => {
                let bucket = std::env::var("S3_BUCKET").map_err(|_| {
                    ConfigError::MissingField("S3_BUCKET environment variable".to_string())
                })?;
                let prefix = std::env::var("S3_PREFIX").ok();
                let region = std::env::var("AWS_REGION").ok();
                config.storage = StorageConfig::S3 {
                    bucket,
                    prefix,
                    region,
                };
            }
            _ => {}
        }

        Ok(config)
    }

    /// Create config with CLI args taking precedence over environment and file.
    ///
    /// Precedence: CLI > File > Environment > Defaults
    pub fn from_sources(cli: &Cli) -> Result<Self, ConfigError> {
        let env_config = Self::from_env()?;

        let file_config = cli
            .config
            .as_ref()
            .map(Self::load_from_file)
            .transpose()?;

        let server = ServerConfig {
            host: cli
                .host
                .clone()
                .or_else(|| file_config.as_ref().map(|c| c.server.host.clone()))
                .unwrap_or_else(|| env_config.server.host.clone()),
            port: cli
                .port
                .or_else(|| file_config.as_ref().map(|c| c.server.port))
                .unwrap_or(env_config.server.port),
        };

        let storage = Self::resolve_storage(cli, file_config.as_ref(), &env_config)?;

        Ok(Config { server, storage })
    }

    fn resolve_storage(
        cli: &Cli,
        file_config: Option<&Config>,
        env_config: &Config,
    ) -> Result<StorageConfig, ConfigError> {
        let backend = cli.storage.clone().unwrap_or(CliStorageBackend::Memory);

        cli.validate_for_storage(&backend)?;

        let storage = match backend {
            CliStorageBackend::Memory => StorageConfig::Memory,
            CliStorageBackend::Lmdb => {
                let path = cli
                    .lmdb_path
                    .clone()
                    .or_else(|| {
                        file_config.and_then(|c| match &c.storage {
                            StorageConfig::Lmdb { path } => Some(path.clone()),
                            _ => None,
                        })
                    })
                    .or_else(|| match &env_config.storage {
                        StorageConfig::Lmdb { path } => Some(path.clone()),
                        _ => None,
                    })
                    .ok_or_else(|| ConfigError::MissingField("lmdb_path".to_string()))?;

                StorageConfig::Lmdb { path }
            }
            #[cfg(feature = "s3-backend")]
            CliStorageBackend::S3 => {
                let bucket = cli
                    .s3_bucket
                    .clone()
                    .or_else(|| {
                        file_config.and_then(|c| match &c.storage {
                            StorageConfig::S3 { bucket, .. } => Some(bucket.clone()),
                            _ => None,
                        })
                    })
                    .ok_or_else(|| ConfigError::MissingField("s3_bucket".to_string()))?;

                let prefix = cli.s3_prefix.clone().or_else(|| {
                    file_config.and_then(|c| match &c.storage {
                        StorageConfig::S3 { prefix, .. } => prefix.clone(),
                        _ => None,
                    })
                });

                let region = cli.aws_region.clone().or_else(|| {
                    file_config.and_then(|c| match &c.storage {
                        StorageConfig::S3 { region, .. } => region.clone(),
                        _ => None,
                    })
                });

                StorageConfig::S3 {
                    bucket,
                    prefix,
                    region,
                }
            }
        };

        Ok(storage)
    }

    pub fn load_from_file<P: AsRef<std::path::Path>>(path: P) -> Result<Self, ConfigError> {
        let contents = std::fs::read_to_string(path)?;
        let config: Self = serde_json::from_str(&contents)?;
        Ok(config)
    }

    pub fn save_to_file<P: AsRef<std::path::Path>>(&self, path: P) -> Result<(), ConfigError> {
        let contents = serde_json::to_string_pretty(self)?;
        std::fs::write(path, contents)?;
        Ok(())
    }
}
