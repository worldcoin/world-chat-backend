//! S3 bucket client implementation

use std::time::Duration;

use aws_config::{retry::RetryConfig, timeout::TimeoutConfig, BehaviorVersion};
use aws_sdk_s3::{
    error::SdkError, operation::head_object::HeadObjectError, presigning::PresigningConfig, Client,
};
use chrono::{DateTime, Utc};
use tracing::{debug, error, info};

use super::{BucketError, BucketResult, PresignedUrl};

const PRESIGNED_URL_EXPIRY_SECS: u64 = 15 * 60;
const MAX_RETRIES: u32 = 3;

/// S3 bucket client for blob storage operations
pub struct BucketClient {
    client: Client,
    bucket_name: String,
    presigned_url_expiry_secs: u64,
}

impl BucketClient {
    /// Creates a new bucket client
    ///
    /// # Arguments
    ///
    /// * `presigned_url_expiry_secs` - Optional expiry time for presigned URLs in seconds (defaults to 15 minutes)
    ///
    /// # Errors
    ///
    /// Returns `BucketError::ConfigError` if AWS configuration fails
    /// Returns `BucketError::ConfigError` if bucket name is not set
    pub async fn new(presigned_url_expiry_secs: Option<u64>) -> BucketResult<Self> {
        let bucket_name = std::env::var("S3_BUCKET_NAME").map_err(|_| {
            BucketError::ConfigError("S3_BUCKET_NAME environment variable not set".to_string())
        })?;

        // Configure retry policy
        let retry_config = RetryConfig::standard()
            .with_max_attempts(MAX_RETRIES)
            .with_initial_backoff(Duration::from_millis(50));

        // Configure timeout
        let timeout_config = TimeoutConfig::builder()
            .operation_timeout(Duration::from_secs(30))
            .build();

        // Load AWS config with LocalStack support for testing
        let mut config_builder = aws_config::load_defaults(BehaviorVersion::latest())
            .await
            .to_builder()
            .retry_config(retry_config)
            .timeout_config(timeout_config);

        // Check if we're using LocalStack (test credentials)
        if std::env::var("AWS_ACCESS_KEY_ID").unwrap_or_default() == "000000000000" {
            info!("Detected LocalStack test environment, configuring endpoint");
            config_builder = config_builder.endpoint_url("http://localhost:4566");
        }

        let config = config_builder.build();

        // Create S3 client with LocalStack compatibility
        let client = if std::env::var("AWS_ACCESS_KEY_ID").unwrap_or_default() == "000000000000" {
            // For LocalStack, we need to force path-style addressing
            let s3_config = aws_sdk_s3::Config::from(&config)
                .to_builder()
                .force_path_style(true)
                .build();
            Client::from_conf(s3_config)
        } else {
            Client::new(&config)
        };

        info!(
            "Initialized S3 bucket client for bucket: {} with {} max retries",
            bucket_name, MAX_RETRIES
        );

        let presigned_url_expiry_secs =
            presigned_url_expiry_secs.unwrap_or(PRESIGNED_URL_EXPIRY_SECS);

        Ok(Self {
            client,
            bucket_name,
            presigned_url_expiry_secs,
        })
    }

    /// Checks if an object exists in the bucket
    ///
    /// # Arguments
    ///
    /// * `blob_id` - The blob ID to check (64-char hex string)
    ///
    /// # Returns
    ///
    /// * `Ok(true)` if object exists
    /// * `Ok(false)` if object does not exist
    /// * `Err(BucketError)` if S3 operation fails
    ///
    /// # Errors
    ///
    /// Returns `BucketError::S3Error` for S3 service errors
    /// Returns `BucketError::UpstreamError` for 5xx errors after retries
    pub async fn check_object_exists(&self, blob_id: &str) -> BucketResult<bool> {
        debug!("Checking if object exists: {}", blob_id);

        let result = self
            .client
            .head_object()
            .bucket(&self.bucket_name)
            .key(blob_id)
            .send()
            .await;

        match result {
            Ok(_) => {
                debug!("Object exists: {}", blob_id);
                Ok(true)
            }
            Err(e) => {
                // Check if it's a 404 (object not found) - this is expected for new uploads
                if let SdkError::ServiceError(ref service_err) = e {
                    if matches!(service_err.err(), HeadObjectError::NotFound(_)) {
                        debug!("Object does not exist: {}", blob_id);
                        return Ok(false);
                    }

                    // Check if it's a 5xx error (upstream error)
                    if service_err.raw().status().as_u16() >= 500 {
                        error!(
                            "Upstream error checking object existence for {}: {}",
                            blob_id, e
                        );
                        return Err(BucketError::UpstreamError(e.to_string()));
                    }
                }

                error!("Failed to check object existence for {}: {}", blob_id, e);
                Err(BucketError::from(e))
            }
        }
    }

    /// Generates a presigned URL for PUT operations
    ///
    /// # Arguments
    ///
    /// * `blob_id` - The blob ID (64-char hex string)
    /// * `content_length` - The expected content length in bytes
    ///
    /// # Returns
    ///
    /// A `PresignedUrl` struct containing the URL and expiration time
    ///
    /// # Errors
    ///
    /// Returns `BucketError::S3Error` if presigned URL generation fails
    /// Returns `BucketError::ConfigError` if `content_length` exceeds maximum
    pub async fn generate_presigned_put_url(
        &self,
        blob_id: &str,
        content_length: i64,
    ) -> BucketResult<PresignedUrl> {
        debug!(
            "Generating presigned URL for object: {} with content length: {}",
            blob_id, content_length
        );

        let put_request = self
            .client
            .put_object()
            .bucket(&self.bucket_name)
            .key(blob_id)
            .content_length(content_length)
            .content_type("application/octet-stream");

        let presigned_config =
            PresigningConfig::expires_in(Duration::from_secs(self.presigned_url_expiry_secs))
                .map_err(|e| {
                    BucketError::ConfigError(format!("Failed to create presigning config: {e}"))
                })?;

        let presigned_url = put_request
            .presigned(presigned_config)
            .await
            .map_err(|e| BucketError::S3Error(format!("Failed to generate presigned URL: {e}")))?;

        let expires_at: DateTime<Utc> =
            Utc::now() + Duration::from_secs(self.presigned_url_expiry_secs);

        debug!(
            "Generated presigned URL for object: {} expires at: {}",
            blob_id, expires_at
        );

        Ok(PresignedUrl {
            url: presigned_url.uri().to_string(),
            expires_at,
        })
    }
}
