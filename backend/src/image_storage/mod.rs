//! S3-based image storage operations

#![deny(
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    missing_docs,
    dead_code
)]

mod error;

use std::sync::Arc;
use std::time::Duration;

use aws_sdk_s3::{
    error::SdkError, operation::head_object::HeadObjectError, presigning::PresigningConfig,
    Client as S3Client,
};
use chrono::{DateTime, Utc};
use tracing::{debug, error};

pub use error::{BucketError, BucketResult};

/// Presigned URL with expiration information
#[derive(Debug, Clone)]
pub struct PresignedUrl {
    /// The presigned URL for PUT operations
    pub url: String,
    /// ISO-8601 UTC timestamp when the URL expires
    pub expires_at: DateTime<Utc>,
}

/// Image storage client for S3 operations
pub struct ImageStorage {
    s3_client: Arc<S3Client>,
    bucket_name: String,
    presigned_url_expiry_secs: u64,
}

impl ImageStorage {
    /// Creates a new image storage client
    ///
    /// # Arguments
    ///
    /// * `s3_client` - Pre-configured S3 client
    /// * `bucket_name` - S3 bucket name for image storage
    /// * `presigned_url_expiry_secs` - Optional expiry time for presigned URLs in seconds (defaults to 15 minutes)
    #[must_use]
    pub const fn new(
        s3_client: Arc<S3Client>,
        bucket_name: String,
        presigned_url_expiry_secs: u64,
    ) -> Self {
        Self {
            s3_client,
            bucket_name,
            presigned_url_expiry_secs,
        }
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
    /// Returns `BucketError::UpstreamError` for 5xx errors
    #[allow(clippy::cognitive_complexity)]
    pub async fn check_object_exists(&self, image_id: &str) -> BucketResult<bool> {
        debug!("Checking if object exists: {}", image_id);

        let result = self
            .s3_client
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
    /// Returns `BucketError::ConfigError` if presigning config creation fails
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
            .s3_client
            .put_object()
            .bucket(&self.bucket_name)
            .key(image_id)
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
            image_id, expires_at
        );

        Ok(PresignedUrl {
            url: presigned_url.uri().to_string(),
            expires_at,
        })
    }
}
