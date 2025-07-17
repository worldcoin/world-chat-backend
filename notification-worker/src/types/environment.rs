//! Environment configuration for different deployment stages

use std::env;
use std::time::Duration;

use aws_config::{retry::RetryConfig, timeout::TimeoutConfig, BehaviorVersion};
use backend_storage::queue::QueueConfig;

/// Application environment configuration
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Environment {
    /// Production environment
    Production,
    /// Staging environment  
    Staging,
    /// Development environment (uses LocalStack)
    Development,
}

impl Environment {
    /// Creates an Environment from the `ENVIRONMENT` environment variable
    ///
    /// # Panics
    ///
    /// Panics if `ENVIRONMENT` contains an invalid value
    #[must_use]
    pub fn from_env() -> Self {
        let env = env::var("ENVIRONMENT")
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

    /// AWS DynamoDB service configuration
    pub async fn dynamodb_client_config(&self) -> aws_sdk_dynamodb::Config {
        let aws_config = self.aws_config().await;
        aws_sdk_dynamodb::Config::from(&aws_config)
    }

    /// AWS SQS service configuration
    pub async fn sqs_client_config(&self) -> aws_sdk_sqs::Config {
        let aws_config = self.aws_config().await;
        aws_sdk_sqs::Config::from(&aws_config)
    }

    /// Returns the DynamoDB table name for push subscriptions
    ///
    /// # Panics
    ///
    /// Panics if the `DYNAMODB_PUSH_TABLE_NAME` environment variable is not set in production/staging
    #[must_use]
    pub fn dynamodb_push_table_name(&self) -> String {
        match self {
            Self::Production | Self::Staging => env::var("DYNAMODB_PUSH_TABLE_NAME")
                .expect("DYNAMODB_PUSH_TABLE_NAME environment variable is not set"),
            Self::Development => "world-chat-push-subscriptions".to_string(),
        }
    }

    /// Returns the DynamoDB GSI name for topic queries
    #[must_use]
    pub fn dynamodb_push_gsi_name(&self) -> String {
        env::var("DYNAMODB_PUSH_GSI_NAME").unwrap_or_else(|_| "topic-index".to_string())
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

    /// Returns the XMTP network address for the environment
    #[must_use]
    pub fn xmtp_network_address(&self) -> String {
        match self {
            Self::Production => env::var("XMTP_NETWORK_ADDRESS")
                .unwrap_or_else(|_| "grpc.production.xmtp.network:5556".to_string()),
            Self::Staging => env::var("XMTP_NETWORK_ADDRESS")
                .unwrap_or_else(|_| "grpc.staging.xmtp.network:5556".to_string()),
            Self::Development => env::var("XMTP_NETWORK_ADDRESS")
                .unwrap_or_else(|_| "grpc.dev.xmtp.network:5556".to_string()),
        }
    }

    /// Returns whether TLS should be used for XMTP connections
    #[must_use]
    pub const fn xmtp_use_tls(&self) -> bool {
        true // Always use TLS for XMTP connections
    }

    /// Returns the number of worker threads for the environment
    #[must_use]
    pub fn worker_count(&self) -> usize {
        match self {
            Self::Production => env::var("WORKER_COUNT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(50),
            Self::Staging => env::var("WORKER_COUNT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(25),
            Self::Development => env::var("WORKER_COUNT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(10),
        }
    }

    /// Returns the cache refresh interval in seconds
    #[must_use]
    pub fn cache_refresh_interval_secs(&self) -> u64 {
        env::var("CACHE_REFRESH_INTERVAL")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(30)
    }

    /// Returns the maximum number of reconnection attempts
    #[must_use]
    pub fn max_reconnect_attempts(&self) -> u32 {
        match self {
            Self::Production => 50,
            Self::Staging => 20,
            Self::Development => 10,
        }
    }

    /// Returns the initial reconnection interval in seconds
    #[must_use]
    pub fn reconnect_interval_secs(&self) -> u64 {
        match self {
            Self::Production => 30,
            Self::Staging => 10,
            Self::Development => 5,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_environment_from_env() {
        // Test development (default)
        env::remove_var("ENVIRONMENT");
        assert_eq!(Environment::from_env(), Environment::Development);

        // Test explicit development
        env::set_var("ENVIRONMENT", "development");
        assert_eq!(Environment::from_env(), Environment::Development);

        // Test staging
        env::set_var("ENVIRONMENT", "staging");
        assert_eq!(Environment::from_env(), Environment::Staging);

        // Test production
        env::set_var("ENVIRONMENT", "production");
        assert_eq!(Environment::from_env(), Environment::Production);

        // Cleanup
        env::remove_var("ENVIRONMENT");
    }

    #[test]
    #[should_panic(expected = "Invalid environment: invalid")]
    fn test_invalid_environment() {
        env::set_var("ENVIRONMENT", "invalid");
        let _ = Environment::from_env();
        env::remove_var("ENVIRONMENT");
    }

    #[test]
    fn test_worker_count() {
        let dev = Environment::Development;
        assert_eq!(dev.worker_count(), 10);

        let staging = Environment::Staging;
        assert_eq!(staging.worker_count(), 25);

        let prod = Environment::Production;
        assert_eq!(prod.worker_count(), 50);
    }

    #[test]
    fn test_xmtp_configuration() {
        let dev = Environment::Development;
        assert_eq!(dev.xmtp_network_address(), "grpc.dev.xmtp.network:5556");
        assert!(dev.xmtp_use_tls());

        let staging = Environment::Staging;
        assert_eq!(
            staging.xmtp_network_address(),
            "grpc.staging.xmtp.network:5556"
        );

        let prod = Environment::Production;
        assert_eq!(
            prod.xmtp_network_address(),
            "grpc.production.xmtp.network:5556"
        );
    }
}
