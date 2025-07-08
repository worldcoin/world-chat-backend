//! S3-based image storage operations
mod error;

use std::sync::Arc;
use std::time::Duration;

use aws_sdk_s3::{
    error::SdkError, operation::head_object::HeadObjectError, presigning::PresigningConfig,
    types::ChecksumAlgorithm, Client as S3Client,
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use chrono::{DateTime, Utc};
use hex::FromHex;

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
pub struct MediaStorage {
    s3_client: Arc<S3Client>,
    bucket_name: String,
    presigned_url_expiry_secs: u64,
}

impl MediaStorage {
    /// Creates a new media storage client
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

    #[must_use]
    pub fn map_sha256_to_s3_key(sha256: &str) -> String {
        let ad = &sha256[0..2];
        let cd = &sha256[2..4];
        format!("media/{ad}/{cd}/{sha256}")
    }

    /// Maps a SHA-256 digest to a base64-encoded string
    /// # Panics
    ///
    /// Panics when the input is not a valid 64-char hex string
    #[must_use]
    pub fn map_sha256_to_b64(sha256: &str) -> String {
        // 1. Convert the hex string to bytes
        let digest_bytes: [u8; 32] =
            <[u8; 32]>::from_hex(sha256).expect("input must be valid 64-char hex");

        // 2. Base-64-encode those bytes for the checksum header / query param
        STANDARD.encode(digest_bytes)
    }

    /// Checks if an object exists in the bucket
    ///
    /// # Arguments
    ///
    /// * `s3_key` - The image ID to check (64-char hex string)
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
    pub async fn check_object_exists(&self, s3_key: &str) -> BucketResult<bool> {
        let result = self
            .s3_client
            .head_object()
            .bucket(&self.bucket_name)
            .key(s3_key)
            .send()
            .await;

        match result {
            Ok(_) => Ok(true),
            Err(SdkError::ServiceError(service_err))
                if matches!(service_err.err(), HeadObjectError::NotFound(_)) =>
            {
                Ok(false)
            }
            Err(SdkError::ServiceError(service_err))
                if service_err.raw().status().as_u16() >= 500 =>
            {
                Err(BucketError::UpstreamError(format!("{service_err:?}")))
            }
            Err(e) => Err(BucketError::from(e)),
        }
    }

    /// Generates a presigned URL for PUT operations
    ///
    /// # Arguments
    ///
    /// * `content_digest_sha256` - The SHA-256 digest of the content
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
        content_digest_sha256: &str,
        content_length: i64,
    ) -> BucketResult<PresignedUrl> {
        let s3_key = Self::map_sha256_to_s3_key(content_digest_sha256);
        let base64_checksum = Self::map_sha256_to_b64(content_digest_sha256);

        let presigned_config =
            PresigningConfig::expires_in(Duration::from_secs(self.presigned_url_expiry_secs))
                .map_err(|e| {
                    BucketError::ConfigError(format!("Failed to create presigning config: {e}"))
                })?;

        let presigned_url = self
            .s3_client
            .put_object()
            .bucket(&self.bucket_name)
            .key(s3_key)
            .content_length(content_length)
            .content_type("application/octet-stream")
            .checksum_sha256(base64_checksum)
            .checksum_algorithm(ChecksumAlgorithm::Sha256)
            .presigned(presigned_config)
            .await
            .map_err(|e| BucketError::S3Error(format!("Failed to generate presigned URL: {e}")))?;

        let expires_at: DateTime<Utc> =
            Utc::now() + Duration::from_secs(self.presigned_url_expiry_secs);

        Ok(PresignedUrl {
            url: presigned_url.uri().to_string(),
            expires_at,
        })
    }
}
