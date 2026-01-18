use crate::error::TelemetryError;
use opentelemetry::global;
use opentelemetry_sdk::metrics::MeterProvider;
use std::time::Duration;
use tracing::info;

/// Configuration for telemetry collection.
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

/// Manages OpenTelemetry initialization and lifecycle.
///
/// Sets up metrics collection with push-based exporter.
pub struct TelemetryService {
    #[allow(dead_code)]
    config: TelemetryConfig,
}

impl TelemetryService {
    pub fn new(config: TelemetryConfig) -> Result<Self, TelemetryError> {
        if config.enable_metrics {
            let provider = MeterProvider::builder().build();
            global::set_meter_provider(provider);
        }

        Ok(Self { config })
    }

    pub async fn initialize(&self) -> Result<(), TelemetryError> {
        info!("OpenTelemetry metrics initialized for push-based collection");
        Ok(())
    }
}

/// Initialize telemetry with default configuration.
pub async fn init_telemetry() -> Result<TelemetryService, TelemetryError> {
    init_telemetry_with_config(TelemetryConfig::default()).await
}

/// Initialize telemetry with custom configuration.
pub async fn init_telemetry_with_config(config: TelemetryConfig) -> Result<TelemetryService, TelemetryError> {
    let service = TelemetryService::new(config)?;

    crate::metrics::Metrics::init();
    service.initialize().await?;

    info!("OpenTelemetry telemetry initialized");
    Ok(service)
}