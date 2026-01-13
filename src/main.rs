use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tracing::{error, info};

pub mod cli;
pub mod config;
pub mod metrics;
pub mod protocol;
pub mod server;
pub mod storage;
pub mod telemetry;

use cli::Cli;
use config::{Config, StorageConfig};
use server::Handler;
use storage::StorageFactory;
use telemetry::{TelemetryConfig, init_telemetry_with_config};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse command line arguments
    let cli = Cli::parse();

    // Initialize logging based on CLI flags
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(if cli.debug {
            tracing::Level::DEBUG
        } else if cli.verbose {
            tracing::Level::INFO
        } else {
            tracing::Level::WARN
        });
    subscriber.init();

    // Validate CLI arguments
    if let Err(e) = cli.validate() {
        eprintln!("Configuration error: {}", e);
        std::process::exit(1);
    }

    // Create configuration from CLI args (with file and env fallbacks)
    let config = Config::from_sources(&cli)?;
    let bind_addr = format!("{}:{}", config.server.host, config.server.port);
    
    info!("Starting Coral Redis Server v{}", env!("CARGO_PKG_VERSION"));
    
    // Initialize OpenTelemetry and metrics
    let telemetry_config = TelemetryConfig {
        enable_metrics: true,
        ..Default::default()
    };
    let _telemetry = match init_telemetry_with_config(telemetry_config).await {
        Ok(service) => service,
        Err(e) => {
            error!("Failed to initialize telemetry: {}", e);
            std::process::exit(1);
        }
    };
    info!("Storage backend: {}", cli.storage);
    info!("Initializing storage backend: {:?}", config.storage);
    let storage = create_storage_backend(&config.storage).await?;
    
    let listener = TcpListener::bind(&bind_addr).await?;
    info!("Redis server listening on {}", bind_addr);

    let config = Arc::new(config);

    loop {
        let (socket, addr) = listener.accept().await?;
        let storage_clone = Arc::clone(&storage);
        let config_clone = Arc::clone(&config);

        tokio::spawn(async move {
            info!("New connection from {}", addr);
            if let Err(e) = handle_connection(socket, storage_clone, config_clone).await {
                error!("Error handling connection: {}", e);
            }
        });
    }
}

async fn create_storage_backend(config: &StorageConfig) -> Result<Arc<dyn storage::StorageBackend>, Box<dyn std::error::Error>> {
    match config {
        StorageConfig::Memory => {
            info!("Using memory storage backend");
            Ok(Arc::from(StorageFactory::create_memory().await))
        },
        StorageConfig::Lmdb { path } => {
            info!("Using LMDB storage backend at path: {:?}", path);
            Ok(Arc::from(StorageFactory::create_lmdb(path).await?))
        },
        #[cfg(feature = "s3-backend")]
        StorageConfig::S3 { bucket, prefix, .. } => {
            info!("Using S3 storage backend with bucket: {}", bucket);
            Ok(Arc::from(StorageFactory::create_s3(bucket.clone(), prefix.clone()).await?))
        },
    }
}

async fn handle_connection(
    mut socket: TcpStream,
    storage: Arc<dyn storage::StorageBackend>,
    config: Arc<Config>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut handler = Handler::new_with_config(storage, config);
    handler.handle_stream(&mut socket).await
}
