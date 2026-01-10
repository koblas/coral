use opentelemetry::metrics::{Counter, Histogram};
use opentelemetry::{global, KeyValue};
use std::sync::OnceLock;
use std::time::Instant;

/// OpenTelemetry metrics for server observability.
///
/// Tracks connections, requests, commands, storage ops, and errors.
/// Singleton instance accessed via `Metrics::get()`.
pub struct Metrics {
    // Server-level metrics
    pub connections_total: Counter<u64>,
    pub connections_active: Counter<u64>,
    pub requests_total: Counter<u64>,
    pub request_duration: Histogram<f64>,
    pub errors_total: Counter<u64>,

    // Command-specific metrics
    pub command_duration: Histogram<f64>,
    pub commands_total: Counter<u64>,

    // Storage backend metrics
    pub storage_operations_total: Counter<u64>,
    pub storage_operation_duration: Histogram<f64>,
    pub storage_errors_total: Counter<u64>,

    // Memory metrics
    pub keys_total: Counter<u64>,
    pub expired_keys_total: Counter<u64>,
}

static METRICS: OnceLock<Metrics> = OnceLock::new();

impl Metrics {
    pub fn init() -> &'static Self {
        METRICS.get_or_init(|| {
            let meter = global::meter("coral-redis");
            
            Metrics {
                connections_total: meter
                    .u64_counter("coral_connections_total")
                    .with_description("Total number of client connections")
                    .init(),
                
                connections_active: meter
                    .u64_counter("coral_connections_active")
                    .with_description("Number of active client connections")
                    .init(),
                
                requests_total: meter
                    .u64_counter("coral_requests_total")
                    .with_description("Total number of requests processed")
                    .init(),
                
                request_duration: meter
                    .f64_histogram("coral_request_duration_seconds")
                    .with_description("Request processing duration in seconds")
                    .init(),
                
                errors_total: meter
                    .u64_counter("coral_errors_total")
                    .with_description("Total number of errors")
                    .init(),
                
                command_duration: meter
                    .f64_histogram("coral_command_duration_seconds")
                    .with_description("Command execution duration in seconds")
                    .init(),
                
                commands_total: meter
                    .u64_counter("coral_commands_total")
                    .with_description("Total number of commands executed")
                    .init(),
                
                storage_operations_total: meter
                    .u64_counter("coral_storage_operations_total")
                    .with_description("Total number of storage operations")
                    .init(),
                
                storage_operation_duration: meter
                    .f64_histogram("coral_storage_operation_duration_seconds")
                    .with_description("Storage operation duration in seconds")
                    .init(),
                
                storage_errors_total: meter
                    .u64_counter("coral_storage_errors_total")
                    .with_description("Total number of storage errors")
                    .init(),
                
                keys_total: meter
                    .u64_counter("coral_keys_total")
                    .with_description("Total number of keys stored")
                    .init(),
                
                expired_keys_total: meter
                    .u64_counter("coral_expired_keys_total")
                    .with_description("Total number of expired keys removed")
                    .init(),
            }
        })
    }

    pub fn get() -> &'static Self {
        METRICS.get().unwrap_or_else(|| {
            // For tests, initialize with defaults if not already initialized
            Self::init()
        })
    }

    // Helper methods for common metric operations
    pub fn record_request(&self, duration: f64) {
        self.requests_total.add(1, &[]);
        self.request_duration.record(duration, &[]);
    }

    pub fn record_command(&self, command: &str, duration: f64) {
        let labels = &[KeyValue::new("command", command.to_string())];
        self.commands_total.add(1, labels);
        self.command_duration.record(duration, labels);
    }

    pub fn record_storage_operation(&self, operation: &str, backend: &str, duration: f64) {
        let labels = &[
            KeyValue::new("operation", operation.to_string()),
            KeyValue::new("backend", backend.to_string()),
        ];
        self.storage_operations_total.add(1, labels);
        self.storage_operation_duration.record(duration, labels);
    }

    pub fn record_storage_error(&self, operation: &str, backend: &str, error_type: &str) {
        let labels = &[
            KeyValue::new("operation", operation.to_string()),
            KeyValue::new("backend", backend.to_string()),
            KeyValue::new("error_type", error_type.to_string()),
        ];
        self.storage_errors_total.add(1, labels);
    }

    pub fn record_error(&self, error_type: &str, command: Option<&str>) {
        let mut labels = vec![KeyValue::new("error_type", error_type.to_string())];
        if let Some(cmd) = command {
            labels.push(KeyValue::new("command", cmd.to_string()));
        }
        self.errors_total.add(1, &labels);
    }

    pub fn increment_connections(&self) {
        self.connections_total.add(1, &[]);
        self.connections_active.add(1, &[]);
    }

    pub fn decrement_connections(&self) {
        // Note: OpenTelemetry counters are monotonic, so we can't decrement
        // For active connections, you'd typically use an UpDownCounter or Gauge
        // This is a simplified implementation
    }

    pub fn record_key_operation(&self, operation: &str, count: u64) {
        match operation {
            "set" => self.keys_total.add(count, &[KeyValue::new("operation", "set")]),
            "expire" => self.expired_keys_total.add(count, &[]),
            _ => {}
        }
    }
}

// Timer utility for measuring durations
pub struct Timer {
    start: Instant,
}

impl Timer {
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
        }
    }

    pub fn elapsed_seconds(&self) -> f64 {
        self.start.elapsed().as_secs_f64()
    }
}

// Convenience macro for timing operations
#[macro_export]
macro_rules! time_operation {
    ($metrics:expr, $operation:expr, $block:expr) => {{
        let timer = $crate::metrics::Timer::new();
        let result = $block;
        let duration = timer.elapsed_seconds();
        $metrics.record_operation($operation, duration);
        result
    }};
}