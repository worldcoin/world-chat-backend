//! Environment configuration for different deployment stages

use std::env;
use std::time::Duration;

use aws_config::{retry::RetryConfig, timeout::TimeoutConfig, BehaviorVersion};
use tracing::Level;

/// Application environment configuration
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Environment {
    /// Production environment
    Production,
    /// Staging environment
    Staging,
    /// Development environment (uses `LocalStack`)
    Development {
        /// Optional override for presigned URL expiry in seconds
        presign_expiry_override: Option<u64>,
    },
}

impl Environment {
    /// Creates an Environment from the `APP_ENV` environment variable
    ///
    /// # Panics
    ///
    /// Panics if `APP_ENV` is not set or contains an invalid value
    #[must_use]
    pub fn from_env() -> Self {
        let env = env::var("APP_ENV")
            .unwrap_or_else(|_| "development".to_string())
            .trim()
            .to_lowercase();

        match env.as_str() {
            "production" => Self::Production,
            "staging" => Self::Staging,
            "development" => {
                // Check for presigned URL expiry override
                let presign_expiry_override = env::var("PRESIGNED_URL_EXPIRY_SECS")
                    .ok()
                    .and_then(|val| val.parse::<u64>().ok());

                Self::Development {
                    presign_expiry_override,
                }
            }
            _ => panic!("Invalid environment: {env}"),
        }
    }

    /// Returns the S3 bucket name for the environment
    ///
    /// # Panics
    ///
    /// Panics if the `S3_BUCKET_NAME` environment variable is not set
    #[must_use]
    pub fn s3_bucket(&self) -> String {
        match self {
            Self::Production | Self::Staging => {
                env::var("S3_BUCKET_NAME").expect("S3_BUCKET_NAME environment variable is not set")
            }
            Self::Development { .. } => {
                env::var("S3_BUCKET_NAME").unwrap_or_else(|_| "world-chat-media".to_string())
            }
        }
    }

    /// Whether to show API docs
    #[must_use]
    pub const fn show_api_docs(&self) -> bool {
        matches!(self, Self::Development { .. } | Self::Staging)
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

    /// AWS S3 service configuration
    pub async fn s3_client_config(&self) -> aws_sdk_s3::Config {
        let aws_config = self.aws_config().await;
        let s3_config: aws_sdk_s3::Config = (&aws_config).into();
        let mut builder = s3_config.to_builder();

        // Override "force path style" to true for compatibility with LocalStack
        // https://github.com/awslabs/aws-sdk-rust/discussions/874
        if matches!(self, Self::Development { .. }) {
            builder.set_force_path_style(Some(true));
        }

        builder.build()
    }

    /// Presigned URL expiry time in seconds
    #[must_use]
    pub fn presigned_url_expiry_secs(&self) -> u64 {
        match self {
            Self::Production | Self::Staging => {
                // Default: 3 minutes
                3 * 60
            }
            Self::Development {
                presign_expiry_override,
            } => {
                // Use override if provided, otherwise default to 3 minutes
                presign_expiry_override.unwrap_or(3 * 60)
            }
        }
    }

    #[must_use]
    pub fn tracing_level(&self) -> Level {
        env::var("TRACING_LEVEL")
            .ok()
            .and_then(|val| val.parse::<Level>().ok())
            .unwrap_or(match self {
                Self::Production | Self::Staging => Level::INFO,
                Self::Development { .. } => Level::DEBUG,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    #[serial]
    fn test_environment_from_env() {
        // Test development (default)
        env::remove_var("APP_ENV");
        env::remove_var("PRESIGNED_URL_EXPIRY_SECS");
        assert_eq!(
            Environment::from_env(),
            Environment::Development {
                presign_expiry_override: None
            }
        );

        // Test explicit development
        env::set_var("APP_ENV", "development");
        assert_eq!(
            Environment::from_env(),
            Environment::Development {
                presign_expiry_override: None
            }
        );

        // Test staging
        env::set_var("APP_ENV", "staging");
        assert_eq!(Environment::from_env(), Environment::Staging);

        // Test production
        env::set_var("APP_ENV", "production");
        assert_eq!(Environment::from_env(), Environment::Production);
    }

    #[test]
    #[serial]
    #[should_panic(expected = "Invalid environment: invalid")]
    fn test_invalid_environment() {
        env::set_var("APP_ENV", "invalid");
        let _ = Environment::from_env();
    }

    #[test]
    #[serial]
    fn test_presigned_url_expiry_secs() {
        // Test default value (3 minutes = 180 seconds)
        let env = Environment::Development {
            presign_expiry_override: None,
        };
        assert_eq!(env.presigned_url_expiry_secs(), 180);

        // Test custom value
        let env = Environment::Development {
            presign_expiry_override: Some(30),
        };
        assert_eq!(env.presigned_url_expiry_secs(), 30);

        // Test Production and Staging always use default
        let env = Environment::Production;
        assert_eq!(env.presigned_url_expiry_secs(), 180);

        let env = Environment::Staging;
        assert_eq!(env.presigned_url_expiry_secs(), 180);
    }

    #[test]
    #[serial]
    fn test_development_with_env_override() {
        // Test development with environment variable override
        env::set_var("APP_ENV", "development");
        env::set_var("PRESIGNED_URL_EXPIRY_SECS", "120");

        let env = Environment::from_env();
        assert_eq!(
            env,
            Environment::Development {
                presign_expiry_override: Some(120)
            }
        );
        assert_eq!(env.presigned_url_expiry_secs(), 120);

        // Test invalid environment variable falls back to None
        env::set_var("PRESIGNED_URL_EXPIRY_SECS", "invalid");
        let env = Environment::from_env();
        assert_eq!(
            env,
            Environment::Development {
                presign_expiry_override: None
            }
        );
        assert_eq!(env.presigned_url_expiry_secs(), 180);

        // Cleanup
        env::remove_var("PRESIGNED_URL_EXPIRY_SECS");
    }
}
