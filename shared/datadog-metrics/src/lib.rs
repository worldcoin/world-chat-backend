use dogstatsd::{Client, OptionsBuilder};
use once_cell::sync::OnceCell;
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::error;

/// Global flag to suppress metrics - useful for tests
pub static METRICS_SUPPRESS: AtomicBool = AtomicBool::new(false);

/// Global Datadog client - initialized once at startup
static DATADOG: OnceCell<Client> = OnceCell::new();

/// Initialize the global DogStatsD client with namespace and default tags
/// Call this once at application startup
///
/// # Arguments
/// * `host` - DD agent host (e.g., "localhost:8125")
/// * `namespace` - Metric namespace prefix (e.g., "world_chat")
/// * `service` - Service name for tagging (e.g., "backend", "enclave_worker", "notification_worker")
/// * `environment` - Environment tag (e.g., "production", "staging", "development")
pub fn init(
    host: impl Into<String>,
    namespace: impl Into<String>,
    service: impl Into<String>,
    environment: impl Into<String>,
) {
    let mut options = OptionsBuilder::new()
        .to_addr(host.into())
        .namespace(namespace.into())
        .build();

    // Set default tags after building the Options struct
    options.default_tags = vec![
        format!("service:{}", service.into()),
        format!("env:{}", environment.into()),
    ];

    match Client::new(options) {
        Ok(client) => {
            if DATADOG.set(client).is_err() {
                error!("DogStatsD client already initialized");
            }
        }
        Err(e) => {
            error!("Failed to create DogStatsD client: {}", e);
        }
    }
}

/// Increment a counter metric
pub fn increment(metric: &str) {
    if METRICS_SUPPRESS.load(Ordering::Relaxed) {
        return;
    }

    if let Some(client) = DATADOG.get() {
        let tags: Vec<&str> = vec![];
        if let Err(e) = client.incr(metric, &tags) {
            error!("Failed to send metric {}: {}", metric, e);
        }
    }
}

/// Increment with additional tags (default tags are automatically included)
pub fn increment_with_tags(metric: &str, tags: &[&str]) {
    if METRICS_SUPPRESS.load(Ordering::Relaxed) {
        return;
    }

    if let Some(client) = DATADOG.get() {
        if let Err(e) = client.incr(metric, tags) {
            error!("Failed to send metric {}: {}", metric, e);
        }
    }
}

/// Record a timing in milliseconds
pub fn timing(metric: &str, ms: u64) {
    if METRICS_SUPPRESS.load(Ordering::Relaxed) {
        return;
    }

    if let Some(client) = DATADOG.get() {
        let tags: Vec<&str> = vec![];
        if let Err(e) = client.timing(metric, ms as i64, &tags) {
            error!("Failed to send timing {}: {}", metric, e);
        }
    }
}

/// Record a timing with additional tags
pub fn timing_with_tags(metric: &str, ms: u64, tags: &[&str]) {
    if METRICS_SUPPRESS.load(Ordering::Relaxed) {
        return;
    }

    if let Some(client) = DATADOG.get() {
        if let Err(e) = client.timing(metric, ms as i64, tags) {
            error!("Failed to send timing {}: {}", metric, e);
        }
    }
}

/// Helper functions to control metrics
pub fn suppress_metrics() {
    METRICS_SUPPRESS.store(true, Ordering::Relaxed);
}

pub fn enable_metrics() {
    METRICS_SUPPRESS.store(false, Ordering::Relaxed);
}

pub fn are_metrics_suppressed() -> bool {
    METRICS_SUPPRESS.load(Ordering::Relaxed)
}
