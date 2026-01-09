use opentelemetry::global;
use opentelemetry_sdk::metrics::MeterProvider;
use std::time::Duration;
use tracing::info;

pub struct TelemetryConfig {
    pub enable_metrics: bool,
    pub collection_interval: Duration,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            enable_metrics: true,
            collection_interval: Duration::from_secs(15),
        }
    }
}

pub struct TelemetryService {
    config: TelemetryConfig,
}

impl TelemetryService {
    pub fn new(config: TelemetryConfig) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        if config.enable_metrics {
            // Initialize OpenTelemetry with default SDK for push-based metrics
            let provider = MeterProvider::builder().build();
            global::set_meter_provider(provider);
        }

        Ok(Self {
            config,
        })
    }

    pub async fn initialize(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if !self.config.enable_metrics {
            info!("Metrics collection disabled");
            return Ok(());
        }

        info!("OpenTelemetry metrics initialized for push-based collection");
        Ok(())
    }
}

// Convenience function to initialize telemetry with default configuration
pub async fn init_telemetry() -> Result<TelemetryService, Box<dyn std::error::Error + Send + Sync>> {
    init_telemetry_with_config(TelemetryConfig::default()).await
}

pub async fn init_telemetry_with_config(config: TelemetryConfig) -> Result<TelemetryService, Box<dyn std::error::Error + Send + Sync>> {
    let service = TelemetryService::new(config)?;
    
    // Initialize metrics
    crate::metrics::Metrics::init();
    
    // Initialize telemetry service
    service.initialize().await?;
    
    info!("OpenTelemetry telemetry initialized");
    Ok(service)
}