use dogstatsd::{Client, Options};
use once_cell::sync::Lazy;
use std::sync::atomic::{AtomicBool, Ordering};

/// Global flag to suppress metrics - useful for tests
pub static METRICS_SUPPRESS: AtomicBool = AtomicBool::new(false);

/// Global Datadog client
pub static DATADOG: Lazy<Client> = Lazy::new(|| Client::new(Options::default()).unwrap());

/// Increment macro with suppression check
/// Usage: dd_incr!("metric.name") or dd_incr!("metric.name", "tag1:value1", "tag2:value2")
#[macro_export]
macro_rules! dd_incr {
    ($key:literal $(, $tag:expr)*) => {
        if !$crate::METRICS_SUPPRESS.load(std::sync::atomic::Ordering::Relaxed) {
            let tags: &[&str] = &[$($tag),*];
            if let Err(e) = $crate::DATADOG.incr(concat!("world-chat.", $key), tags) {
                tracing::error!("Failed to send metric: {}", e);
            }
        }
    };
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
