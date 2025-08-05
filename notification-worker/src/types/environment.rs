//! Environment configuration for different deployment stages

use std::{env, time::Duration};

use aws_config::{retry::RetryConfig, timeout::TimeoutConfig, BehaviorVersion};
use backend_storage::queue::QueueConfig;

const DEFAULT_RECONNECT_DELAY_MS: u64 = 100;
const DEFAULT_MAX_RECONNECT_DELAY_MS: u64 = 30_000;
const DEFAULT_CONNECTION_TIMEOUT_MS: u64 = 30_000;
const DEFAULT_CONNECT_TIMEOUT_MS: u64 = 5_000;

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
    #[must_use]
    pub const fn xmtp_endpoint(&self) -> &'static str {
        match self {
            Self::Production => "https://grpc.production.xmtp.network:443",
            Self::Staging => "https://grpc.dev.xmtp.network:443",
            Self::Development => "http://localhost:25556", // Local Docker
        }
    }

    /// Returns whether to use TLS for this environment
    #[must_use]
    pub const fn use_tls(&self) -> bool {
        match self {
            Self::Production | Self::Staging => true,
            Self::Development => false,
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

    /// Returns the connection timeout in milliseconds
    #[must_use]
    pub fn connection_timeout_ms(&self) -> u64 {
        env::var("XMTP_CONNECTION_TIMEOUT_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_CONNECTION_TIMEOUT_MS)
    }

    /// Returns the connect timeout in milliseconds
    #[must_use]
    pub fn connect_timeout_ms(&self) -> u64 {
        env::var("XMTP_CONNECT_TIMEOUT_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_CONNECT_TIMEOUT_MS)
    }

    /// Returns the endpoint URL to use for AWS services
    #[must_use]
    pub const fn override_aws_endpoint_url(&self) -> Option<&str> {
        match self {
            // Regular AWS endpoints for production and staging
            Self::Production | Self::Staging => None,
            // LocalStack endpoint for development
            Self::Development { .. } => Some("http://localhost:4566"),
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
}
