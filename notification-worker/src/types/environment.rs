//! Environment configuration for different deployment stages

use std::{env, time::Duration};

use aws_config::{retry::RetryConfig, timeout::TimeoutConfig, BehaviorVersion};
use backend_storage::queue::QueueConfig;

const DEFAULT_RECONNECT_DELAY_MS: u64 = 100;
const DEFAULT_MAX_RECONNECT_DELAY_MS: u64 = 30_000;
const DEFAULT_REQUEST_TIMEOUT_MS: u64 = 30_000;
const DEFAULT_CONNECTION_TIMEOUT_MS: u64 = 5_000;

/// Application environment configuration
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Environment {
    /// Production environment
    Production,
    /// Staging environment  
    Staging,
    /// Development environment (uses `LocalStack`)
    Development,
}

impl Environment {
    /// Creates an Environment from the `APP_ENV` environment variable
    ///
    /// # Panics
    ///
    /// Panics if `APP_ENV` contains an invalid value
    #[must_use]
    pub fn from_env() -> Self {
        let env = env::var("APP_ENV")
            .unwrap_or_else(|_| "development".to_string())
            .trim()
            .to_lowercase();

        match env.as_str() {
            "production" => Self::Production,
            "staging" => Self::Staging,
            "development" => Self::Development,
            _ => panic!("Invalid environment: {env}"),
        }
    }

    /// Returns the XMTP gRPC endpoint for this environment
    ///
    /// # Panics
    ///
    /// Panics if the `XMTP_ENDPOINT_URL` environment variable is not set
    #[must_use]
    pub fn xmtp_endpoint(&self) -> String {
        env::var("XMTP_ENDPOINT_URL").expect("XMTP_ENDPOINT_URL environment variable is not set")
    }

    /// Returns whether to use TLS for this environment
    ///
    /// # Panics
    ///
    /// Panics if the `XMTP_ENDPOINT_URL` environment variable is not set, or if TLS is disabled in Production or Staging environments
    #[must_use]
    pub fn use_tls(&self) -> bool {
        let endpoint_url = env::var("XMTP_ENDPOINT_URL")
            .expect("XMTP_ENDPOINT_URL environment variable is not set");

        // Determine TLS based on URL scheme
        let use_tls = endpoint_url.starts_with("https://");

        // Validate TLS usage for production environments
        match self {
            Self::Production | Self::Staging if !use_tls => {
                panic!("TLS must be enabled in {self:?} environment. Current endpoint: {endpoint_url}")
            }
            _ => use_tls,
        }
    }

    /// Returns the default number of workers for this environment
    #[must_use]
    pub const fn num_workers(&self) -> usize {
        match self {
            Self::Production => 50,
            Self::Staging => 20,
            Self::Development => 10,
        }
    }

    /// Returns the channel capacity for this environment
    #[must_use]
    pub const fn channel_capacity(&self) -> usize {
        self.num_workers() * 2
    }

    /// Returns the initial reconnection delay in milliseconds
    #[must_use]
    pub fn reconnect_delay_ms(&self) -> u64 {
        env::var("XMTP_RECONNECT_DELAY_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_RECONNECT_DELAY_MS)
    }

    /// Returns the maximum reconnection delay in milliseconds
    #[must_use]
    pub fn max_reconnect_delay_ms(&self) -> u64 {
        env::var("XMTP_MAX_RECONNECT_DELAY_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_MAX_RECONNECT_DELAY_MS)
    }

    /// Returns the request timeout in milliseconds
    /// 
    /// This timeout applies to individual gRPC requests after connection is established
    #[must_use]
    pub fn request_timeout_ms(&self) -> u64 {
        env::var("XMTP_REQUEST_TIMEOUT_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_REQUEST_TIMEOUT_MS)
    }

    /// Returns the connection timeout in milliseconds
    /// 
    /// This timeout applies only to establishing the initial TCP connection
    #[must_use]
    pub fn connection_timeout_ms(&self) -> u64 {
        env::var("XMTP_CONNECTION_TIMEOUT_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_CONNECTION_TIMEOUT_MS)
    }

    /// Returns the endpoint URL to use for AWS services
    #[must_use]
    pub const fn override_aws_endpoint_url(&self) -> Option<&str> {
        match self {
            // Regular AWS endpoints for production and staging
            Self::Production | Self::Staging => None,
            // LocalStack endpoint for development
            Self::Development => Some("http://localhost:4566"),
        }
    }

    /// AWS configuration with retry and timeout settings
    pub async fn aws_config(&self) -> aws_config::SdkConfig {
        let retry_config = RetryConfig::standard()
            .with_max_attempts(3)
            .with_initial_backoff(Duration::from_millis(50));

        let timeout_config = TimeoutConfig::builder()
            .operation_timeout(Duration::from_secs(30))
            .build();

        let mut config_builder = aws_config::load_defaults(BehaviorVersion::latest())
            .await
            .to_builder()
            .retry_config(retry_config)
            .timeout_config(timeout_config);

        if let Some(endpoint_url) = self.override_aws_endpoint_url() {
            config_builder = config_builder.endpoint_url(endpoint_url);
        }

        config_builder.build()
    }

    /// AWS SQS service configuration
    pub async fn sqs_client_config(&self) -> aws_sdk_sqs::Config {
        let aws_config = self.aws_config().await;
        aws_sdk_sqs::Config::from(&aws_config)
    }

    /// Returns the notification queue configuration
    ///
    /// # Panics
    ///
    /// Panics if the `NOTIFICATION_QUEUE_URL` environment variable is not set in production/staging
    #[must_use]
    pub fn notification_queue_config(&self) -> QueueConfig {
        let queue_url = match self {
            Self::Production | Self::Staging => env::var("NOTIFICATION_QUEUE_URL")
                .expect("NOTIFICATION_QUEUE_URL environment variable is not set"),
            Self::Development => {
                "http://localhost:4566/000000000000/notification-queue.fifo".to_string()
            }
        };

        QueueConfig {
            queue_url,
            default_max_messages: 10,
            default_visibility_timeout: 60, // 60 seconds - Longer timeout for notifications
            default_wait_time_seconds: 20,  // Enable long polling by default
        }
    }
}

#[cfg(test)]
mod tests {
    use serial_test::serial;

    use super::*;

    #[test]
    #[serial]
    fn test_environment_from_env() {
        // Test development (default)
        env::remove_var("APP_ENV");
        assert_eq!(Environment::from_env(), Environment::Development);

        // Test explicit development
        env::set_var("APP_ENV", "development");
        assert_eq!(Environment::from_env(), Environment::Development);

        // Test staging
        env::set_var("APP_ENV", "staging");
        assert_eq!(Environment::from_env(), Environment::Staging);

        // Test production
        env::set_var("APP_ENV", "production");
        assert_eq!(Environment::from_env(), Environment::Production);

        // Cleanup
        env::remove_var("APP_ENV");
    }

    #[test]
    #[serial]
    #[should_panic(expected = "Invalid environment: invalid")]
    fn test_invalid_environment() {
        env::set_var("APP_ENV", "invalid");
        let _ = Environment::from_env();
        env::remove_var("APP_ENV");
    }

    #[test]
    #[serial]
    fn test_xmtp_endpoint_required() {
        let env_instance = Environment::Development;

        // Test HTTP endpoint
        env::set_var("XMTP_ENDPOINT_URL", "http://custom-xmtp.example.com:8080");
        assert_eq!(
            env_instance.xmtp_endpoint(),
            "http://custom-xmtp.example.com:8080"
        );
        assert!(!env_instance.use_tls());

        // Test HTTPS endpoint
        env::set_var("XMTP_ENDPOINT_URL", "https://secure-xmtp.example.com:443");
        assert_eq!(
            env_instance.xmtp_endpoint(),
            "https://secure-xmtp.example.com:443"
        );
        assert!(env_instance.use_tls());

        // Cleanup
        env::remove_var("XMTP_ENDPOINT_URL");
    }

    #[test]
    #[serial]
    #[should_panic(expected = "XMTP_ENDPOINT_URL environment variable is not set")]
    fn test_xmtp_endpoint_panic_when_missing() {
        let env_instance = Environment::Development;

        env::remove_var("XMTP_ENDPOINT_URL");
        let _ = env_instance.xmtp_endpoint();
    }

    #[test]
    #[serial]
    fn test_xmtp_endpoint_with_different_environments() {
        // Test with staging environment
        let staging_env = Environment::Staging;

        // Set staging with HTTPS endpoint
        env::set_var("XMTP_ENDPOINT_URL", "https://grpc.dev.xmtp.network:443");
        assert_eq!(
            staging_env.xmtp_endpoint(),
            "https://grpc.dev.xmtp.network:443"
        );
        assert!(staging_env.use_tls());

        // Set staging with custom endpoint
        env::set_var("XMTP_ENDPOINT_URL", "http://localhost:9999");
        assert_eq!(staging_env.xmtp_endpoint(), "http://localhost:9999");
        // This should panic due to HTTP endpoint in staging environment
        std::panic::catch_unwind(|| staging_env.use_tls()).expect_err("Expected panic for HTTP endpoint in staging");

        // Cleanup
        env::remove_var("XMTP_ENDPOINT_URL");
    }

    #[test]
    #[serial]
    #[should_panic(expected = "TLS must be enabled in Production environment")]
    fn test_production_requires_tls_panic() {
        let prod_env = Environment::Production;

        // Test with HTTP endpoint (should panic)
        env::set_var("XMTP_ENDPOINT_URL", "http://insecure-endpoint.com");
        let _ = prod_env.use_tls();

        // Cleanup - won't be reached due to panic
        env::remove_var("XMTP_ENDPOINT_URL");
    }

    #[test]
    #[serial]
    fn test_production_allows_tls() {
        let prod_env = Environment::Production;

        // Test with HTTPS endpoint (should succeed)
        env::set_var("XMTP_ENDPOINT_URL", "https://secure-endpoint.com");
        assert!(prod_env.use_tls());

        // Cleanup
        env::remove_var("XMTP_ENDPOINT_URL");
    }

    #[test]
    #[serial]
    #[should_panic(expected = "TLS must be enabled in Staging environment")]
    fn test_staging_requires_tls_panic() {
        let staging_env = Environment::Staging;

        // Test with HTTP endpoint (should panic)
        env::set_var("XMTP_ENDPOINT_URL", "http://staging-insecure.com");
        let _ = staging_env.use_tls();

        // Cleanup - won't be reached due to panic
        env::remove_var("XMTP_ENDPOINT_URL");
    }

    #[test]
    #[serial]
    fn test_staging_allows_tls() {
        let staging_env = Environment::Staging;

        // Test with HTTPS endpoint (should succeed)
        env::set_var("XMTP_ENDPOINT_URL", "https://staging-secure.com");
        assert!(staging_env.use_tls());

        // Cleanup
        env::remove_var("XMTP_ENDPOINT_URL");
    }

    #[test]
    #[serial]
    fn test_development_allows_insecure_tls() {
        let dev_env = Environment::Development;

        // Test with HTTP endpoint (should succeed in Development)
        env::set_var("XMTP_ENDPOINT_URL", "http://localhost:8080");
        assert!(!dev_env.use_tls());

        // Test with HTTPS endpoint (should also succeed)
        env::set_var("XMTP_ENDPOINT_URL", "https://localhost:8443");
        assert!(dev_env.use_tls());

        // Cleanup
        env::remove_var("XMTP_ENDPOINT_URL");
    }
}
