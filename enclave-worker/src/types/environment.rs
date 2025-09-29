use std::{env, time::Duration};

use aws_config::{retry::RetryConfig, timeout::TimeoutConfig, BehaviorVersion};
use backend_storage::queue::QueueConfig;

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

    /// Returns the Push Notification Subscription storage table name
    ///
    /// # Panics
    ///
    /// Panics if the `DYNAMODB_PUSH_TOPIC_GSI` environment variable is not set in production/staging
    #[must_use]
    pub fn push_subscription_table_name(&self) -> String {
        match self {
            Self::Production | Self::Staging => env::var("DYNAMODB_PUSH_TABLE_NAME")
                .expect("DYNAMODB_PUSH_TABLE_NAME environment variable is not set"),
            Self::Development => "world-chat-push-subscriptions".to_string(),
        }
    }

    /// Whether to show API docs
    #[must_use]
    pub const fn show_api_docs(&self) -> bool {
        matches!(self, Self::Development { .. } | Self::Staging)
    }

    /// Returns the Enclave CID
    ///
    /// # Panics
    ///
    /// Panics if the `ENCLAVE_CID` environment variable is not set in production/staging
    #[must_use]
    pub fn enclave_cid(&self) -> u32 {
        env::var("ENCLAVE_CID")
            .expect("ENCLAVE_CID environment variable is not set")
            .parse()
            .expect("ENCLAVE_CID environment variable is not a valid u32")
    }

    /// Returns the Enclave PORT
    ///
    /// # Panics
    ///
    /// Panics if the `ENCLAVE_PORT` environment variable is not set in production/staging
    #[must_use]
    pub fn enclave_port(&self) -> u32 {
        env::var("ENCLAVE_PORT")
            .expect("ENCLAVE_PORT environment variable is not set")
            .parse()
            .expect("ENCLAVE_PORT environment variable is not a valid u32")
    }

    /// Returns the Braze API KEY
    ///
    /// # Panics
    ///
    /// Panics if the `BRAZE_API_KEY` environment variable is not set in production/staging
    #[must_use]
    pub fn braze_api_key(&self) -> String {
        env::var("BRAZE_API_KEY").expect("BRAZE_API_KEY environment variable is not set")
    }

    /// Returns the Braze API REGION
    ///
    /// # Panics
    ///
    /// Panics if the `BRAZE_API_REGION` environment variable is not set in production/staging
    #[must_use]
    pub fn braze_api_region(&self) -> String {
        env::var("BRAZE_API_REGION").expect("BRAZE_API_REGION environment variable is not set")
    }

    /// Returns the Braze HTTP PROXY PORT
    ///
    /// # Panics
    ///
    /// Panics if the `BRAZE_HTTP_PROXY_PORT` environment variable is not set in production/staging
    #[must_use]
    pub fn braze_http_proxy_port(&self) -> u32 {
        env::var("BRAZE_HTTP_PROXY_PORT")
            .expect("BRAZE_HTTP_PROXY_PORT environment variable is not set")
            .parse()
            .expect("BRAZE_HTTP_PROXY_PORT environment variable is not a valid u32")
    }

    /// Returns the Redis URL for caching
    ///
    /// # Panics
    ///
    /// Panics if the `REDIS_URL` environment variable is not set in production/staging
    #[must_use]
    pub fn redis_url(&self) -> String {
        match self {
            Self::Production | Self::Staging => {
                env::var("REDIS_URL").expect("REDIS_URL environment variable is not set")
            }
            Self::Development => "redis://localhost:6379".to_string(),
        }
    }

    /// Metrics url
    ///
    /// # Panics
    ///
    /// Panics if the `METRICS_URL` environment variable is not set in production/staging
    #[must_use]
    pub fn metrics_url(&self) -> String {
        env::var("METRICS_URL").expect("METRICS_URL environment variable is not set")
    }

    /// Metrics host
    ///
    /// # Panics
    ///
    /// Panics if the `METRICS_HOST` environment variable is not set in production/staging
    #[must_use]
    pub fn metrics_host(&self) -> String {
        env::var("METRICS_HOST").expect("METRICS_HOST environment variable is not set")
    }

    /// Metrics port
    ///
    /// # Panics
    ///
    /// Panics if the `METRICS_PORT` environment variable is not set in production/staging
    #[must_use]
    pub fn metrics_port(&self) -> u32 {
        env::var("METRICS_PORT")
            .expect("METRICS_PORT environment variable is not set")
            .parse()
            .expect("METRICS_PORT environment variable is not a valid u32")
    }

    /// DD Service
    ///
    /// # Panics
    ///
    /// Panics if the `DD_SERVICE` environment variable is not set in production/staging
    #[must_use]
    pub fn dd_service(&self) -> String {
        env::var("DD_SERVICE").expect("DD_SERVICE environment variable is not set")
    }

    // DD Environment
    ///
    /// # Panics
    ///
    /// Panics if the `DD_ENV` environment variable is not set in production/staging
    #[must_use]
    pub fn dd_env(&self) -> String {
        env::var("DD_ENV").expect("DD_ENV environment variable is not set")
    }
}
