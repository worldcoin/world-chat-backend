#[path = "../common/mod.rs"]
mod common;

use common::e2e_utils::*;
use common::*;
use std::sync::Arc;
use aws_sdk_s3::Client as S3Client;
use backend::{media_storage::MediaStorage, routes, types::Environment};
use axum::Extension;

/// E2E test setup with real dependencies
pub struct E2ETestSetup {
    pub router: axum::Router,
    pub s3_client: Arc<S3Client>,
    pub media_storage: Arc<MediaStorage>,
    pub environment: Environment,
    pub bucket_name: String,
}

impl E2ETestSetup {
    /// Create a new E2E test setup with real dependencies
    pub async fn new() -> Self {
        // Setup test environment
        setup_test_env();

        // Use development environment for E2E tests (LocalStack)
        let environment = Environment::Development;

        // Configure AWS S3 client for LocalStack
        let s3_config = environment.s3_client_config().await;
        let s3_client = Arc::new(S3Client::from_conf(s3_config));

        // Get bucket name
        let bucket_name = environment.s3_bucket();

        // Create media storage client
        let media_storage = Arc::new(MediaStorage::new(
            s3_client.clone(),
            bucket_name.clone(),
            environment.presigned_url_expiry_secs(),
        ));

        // Create router with extensions
        let router = routes::handler()
            .layer(Extension(environment))
            .layer(Extension(media_storage.clone()))
            .into();

        Self {
            router,
            s3_client,
            media_storage,
            environment,
            bucket_name,
        }
    }

    /// Get presigned URL expiry duration for testing
    pub fn presigned_url_expiry_secs(&self) -> u64 {
        self.environment.presigned_url_expiry_secs()
    }
}

// Placeholder test to ensure E2E infrastructure works
#[tokio::test]
#[ignore = "E2E tests - run manually"]
async fn test_e2e_infrastructure() {
    let setup = E2ETestSetup::new().await;
    
    // Test that we can generate test data
    let (data, sha256) = generate_test_image(1024);
    assert_eq!(data.len(), 1024);
    assert_eq!(sha256.len(), 64);
    
    // Test that we can calculate checksums
    let calculated_sha256 = calculate_sha256(&data);
    assert_eq!(sha256, calculated_sha256);
    
    // Test that we have LocalStack setup
    assert!(setup.is_localstack());
    
    println!("E2E infrastructure test passed!");
}

impl E2ETestSetup {
    /// Check if running in LocalStack environment
    pub fn is_localstack(&self) -> bool {
        matches!(self.environment, Environment::Development)
    }
}