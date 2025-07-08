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

/// S3 bucket client for image storage operations
pub struct BucketClient {
    client: Client,
    bucket_name: String,
}

impl BucketClient {
    /// Creates a new bucket client
    ///
    /// # Errors
    ///
    /// Returns `BucketError::ConfigError` if AWS configuration fails
    /// Returns `BucketError::ConfigError` if bucket name is not set
    pub async fn new() -> BucketResult<Self> {
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

        let config = aws_config::load_defaults(BehaviorVersion::latest())
            .await
            .to_builder()
            .retry_config(retry_config)
            .timeout_config(timeout_config)
            .build();

        let client = Client::new(&config);

        info!(
            "Initialized S3 bucket client for bucket: {} with {} max retries",
            bucket_name, MAX_RETRIES
        );

        Ok(Self {
            client,
            bucket_name,
        })
    }

    /// Checks if an object exists in the bucket
    ///
    /// # Arguments
    ///
    /// * `image_id` - The image ID to check (64-char hex string)
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
    pub async fn check_object_exists(&self, image_id: &str) -> BucketResult<bool> {
        debug!("Checking if object exists: {}", image_id);

        let result = self
            .client
            .head_object()
            .bucket(&self.bucket_name)
            .key(image_id)
            .send()
            .await;

        match result {
            Ok(_) => {
                debug!("Object exists: {}", image_id);
                Ok(true)
            }
            Err(e) => {
                // Check if it's a 404 (object not found) - this is expected for new uploads
                if let SdkError::ServiceError(ref service_err) = e {
                    if matches!(service_err.err(), HeadObjectError::NotFound(_)) {
                        debug!("Object does not exist: {}", image_id);
                        return Ok(false);
                    }

                    // Check if it's a 5xx error (upstream error)
                    if service_err.raw().status().as_u16() >= 500 {
                        error!(
                            "Upstream error checking object existence for {}: {}",
                            image_id, e
                        );
                        return Err(BucketError::UpstreamError(e.to_string()));
                    }
                }

                error!("Failed to check object existence for {}: {}", image_id, e);
                Err(BucketError::from(e))
            }
        }
    }

    /// Generates a presigned URL for PUT operations
    ///
    /// # Arguments
    ///
    /// * `image_id` - The image ID (64-char hex string)
    /// * `content_length` - The expected content length in bytes
    ///
    /// # Returns
    ///
    /// A `PresignedUrl` struct containing the URL and expiration time
    ///
    /// # Errors
    ///
    /// Returns `BucketError::S3Error` if presigned URL generation fails
    /// Returns `BucketError::ConfigError` if content_length exceeds maximum
    pub async fn generate_presigned_put_url(
        &self,
        image_id: &str,
        content_length: i64,
    ) -> BucketResult<PresignedUrl> {
        debug!(
            "Generating presigned URL for object: {} with content length: {}",
            image_id, content_length
        );

        let put_request = self
            .client
            .put_object()
            .bucket(&self.bucket_name)
            .key(image_id)
            .content_length(content_length)
            .content_type("application/octet-stream");

        let presigned_config =
            PresigningConfig::expires_in(Duration::from_secs(PRESIGNED_URL_EXPIRY_SECS)).map_err(
                |e| BucketError::ConfigError(format!("Failed to create presigning config: {}", e)),
            )?;

        let presigned_url = put_request.presigned(presigned_config).await.map_err(|e| {
            BucketError::S3Error(format!("Failed to generate presigned URL: {}", e))
        })?;

        let expires_at: DateTime<Utc> = Utc::now() + Duration::from_secs(PRESIGNED_URL_EXPIRY_SECS);

        debug!(
            "Generated presigned URL for object: {} expires at: {}",
            image_id, expires_at
        );

        Ok(PresignedUrl {
            url: presigned_url.uri().to_string(),
            expires_at,
        })
    }
}
