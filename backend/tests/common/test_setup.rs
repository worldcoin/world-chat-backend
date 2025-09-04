use aws_config::{BehaviorVersion, Region};
use aws_credential_types::Credentials;
use aws_sdk_dynamodb::Client as DynamoDbClient;
use aws_sdk_kms::types::{KeySpec, KeyUsageType};
use aws_sdk_kms::Client as KmsClient;
use aws_sdk_s3::Client as S3Client;
use axum::{body::Body, http::Request, response::Response, Extension, Router};
use backend::{jwt::JwtManager, media_storage::MediaStorage, routes, types::Environment};
use backend_storage::auth_proof::AuthProofStorage;
use backend_storage::push_subscription::PushSubscriptionStorage;
use std::sync::Arc;
use tower::ServiceExt;

use super::dynamodb_setup::DynamoDbTestSetup;

/// Setup test environment variables with all the required configuration
pub fn setup_test_env() {
    // Load test environment variables
    dotenvy::from_path(".env.test").ok();

    // Initialize tracing for tests
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .try_init()
        .ok();
}

/// Base test setup with core dependencies
#[allow(dead_code)]
pub struct TestSetup {
    pub router: Router,
    pub environment: Environment,
    pub media_storage: Arc<MediaStorage>,
    pub kms_client: Arc<KmsClient>,
    pub push_subscription_storage: Arc<PushSubscriptionStorage>,
    // Keep DynamoDbTestSetup alive for the duration of the test
    _dynamodb_setup: DynamoDbTestSetup,
}

impl TestSetup {
    /// Create a default test setup with auth disabled
    #[must_use]
    pub async fn default() -> Self {
        Self::new(None, true).await
    }

    pub async fn new(presign_expiry_override: Option<u64>, disable_auth: bool) -> Self {
    /// Create a default test setup with auth disabled
    #[must_use]
    pub async fn default() -> Self {
        Self::new(None, true).await
    }

    pub async fn new(presign_expiry_override: Option<u64>, disable_auth: bool) -> Self {
        setup_test_env();

        let environment = Environment::Development {
            presign_expiry_override,
            disable_auth,
            disable_auth,
        };

        let s3_config = environment.s3_client_config().await;
        let s3_client = Arc::new(S3Client::from_conf(s3_config));
        let bucket_name = environment.s3_bucket();

        let media_storage = Arc::new(MediaStorage::new(
            s3_client.clone(),
            bucket_name.clone(),
            environment.presigned_url_expiry_secs(),
        ));

        let dynamodb_client = Arc::new(DynamoDbClient::new(&environment.aws_config().await));
        let dynamodb_test_setup = DynamoDbTestSetup::new(dynamodb_client.clone()).await;

        // Initialize JWT manager (KMS-backed)
        let kms_client = Arc::new(KmsClient::new(&environment.aws_config().await));
        let jwt_manager = Arc::new(
            JwtManager::new(kms_client.clone(), &environment)
                .await
                .expect("jwt manager"),
        );

        // Initialize auth proof storage with test table
        let auth_proof_storage = Arc::new(AuthProofStorage::new(
            dynamodb_client.clone(),
            dynamodb_test_setup.auth_proofs_table_name.clone(),
        ));
        let push_subscription_storage = Arc::new(PushSubscriptionStorage::new(
            dynamodb_client.clone(),
            dynamodb_test_setup.push_subscriptions_table_name.clone(),
        ));

        let router = routes::handler()
            .layer(Extension(environment.clone()))
            .layer(Extension(media_storage.clone()))
            .layer(Extension(auth_proof_storage.clone()))
            .layer(Extension(jwt_manager.clone()))
            .layer(Extension(push_subscription_storage.clone()))
            .into();

        Self {
            router,
            environment,
            media_storage,
            kms_client,
            push_subscription_storage,
            _dynamodb_setup: dynamodb_test_setup,
        }
    }

    pub async fn send_post_request(
        &self,
        route: &str,
        payload: serde_json::Value,
    ) -> Result<Response, Box<dyn std::error::Error>> {
        let request = Request::builder()
            .uri(route)
            .method("POST")
            .header("Content-Type", "application/json")
            .body(Body::from(payload.to_string()))?;

        let response = self.router.clone().oneshot(request).await?;
        Ok(response)
    }

    pub async fn parse_response_body(
        &self,
        response: Response,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        use http_body_util::BodyExt;

        let body = response.into_body().collect().await?.to_bytes();
        let json = serde_json::from_slice(&body)?;
        Ok(json)
    }

    pub async fn send_get_request(
        &self,
        route: &str,
    ) -> Result<Response, Box<dyn std::error::Error>> {
        let request = Request::builder()
            .uri(route)
            .method("GET")
            .body(Body::empty())?;
        let response = self.router.clone().oneshot(request).await?;
        Ok(response)
    }

    /// Send a POST request with custom headers (e.g., Authorization)
    pub async fn send_post_request_with_headers(
        &self,
        route: &str,
        payload: serde_json::Value,
        headers: Vec<(&str, &str)>,
    ) -> Result<Response, Box<dyn std::error::Error>> {
        let mut request_builder = Request::builder()
            .uri(route)
            .method("POST")
            .header("Content-Type", "application/json");

        // Add custom headers
        for (key, value) in headers {
            request_builder = request_builder.header(key, value);
        }

        let request = request_builder.body(Body::from(payload.to_string()))?;
        let response = self.router.clone().oneshot(request).await?;
        Ok(response)
    }
}
