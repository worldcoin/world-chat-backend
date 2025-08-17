use aws_config::{BehaviorVersion, Region};
use aws_credential_types::Credentials;
use aws_sdk_dynamodb::Client as DynamoDbClient;
use aws_sdk_s3::Client as S3Client;
use axum::{body::Body, http::Request, response::Response, Extension, Router};
use backend::{jwt::JwtManager, media_storage::MediaStorage, routes, types::Environment};
use backend_storage::auth_proof::AuthProofStorage;
use std::sync::Arc;
use tower::ServiceExt;

use super::dynamodb_setup::DynamoDbTestSetup;

/// Setup test environment variables with all the required configuration
pub fn setup_test_env() {
    // World ID configuration
    std::env::set_var(
        "WORLD_ID_APP_ID",
        "app_staging_509648994ab005fe79c4ddd0449606ca",
    );
    std::env::set_var("WORLD_ID_ACTION", "test_action");
    std::env::set_var("WORLD_ID_ENV", "staging");

    // JWT secret name - Resource is created in aws-seed.sh
    std::env::set_var("JWT_SECRET_NAME", "world-chat-jwt-secret");

    // Initialize tracing for tests
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .try_init()
        .ok();
}

/// Test configuration for LocalStack
const LOCALSTACK_ENDPOINT: &str = "http://localhost:4566";
const TEST_REGION: &str = "us-east-1";

/// Create AWS config for LocalStack for tests
pub async fn create_aws_config() -> aws_config::SdkConfig {
    let credentials = Credentials::from_keys(
        "test", // AWS_ACCESS_KEY_ID
        "test", // AWS_SECRET_ACCESS_KEY
        None,   // no session token
    );
    let config = aws_config::defaults(BehaviorVersion::latest())
        .endpoint_url(LOCALSTACK_ENDPOINT)
        .region(Region::new(TEST_REGION))
        .credentials_provider(credentials)
        .load()
        .await;

    config
}

/// Base test setup with core dependencies
#[allow(dead_code)]
pub struct TestSetup {
    pub router: Router,
    pub environment: Environment,
    pub s3_client: Arc<S3Client>,
    pub bucket_name: String,
    pub media_storage: Arc<MediaStorage>,
    // Keep DynamoDbTestSetup alive for the duration of the test
    _dynamodb_setup: DynamoDbTestSetup,
}

impl TestSetup {
    pub async fn new(presign_expiry_override: Option<u64>) -> Self {
        setup_test_env();

        let environment = Environment::Development {
            presign_expiry_override,
        };

        let s3_config = environment.s3_client_config().await;
        let s3_client = Arc::new(S3Client::from_conf(s3_config));
        let bucket_name = environment.s3_bucket();

        let media_storage = Arc::new(MediaStorage::new(
            s3_client.clone(),
            bucket_name.clone(),
            environment.presigned_url_expiry_secs(),
        ));

        let config = create_aws_config().await;
        let dynamodb_client = Arc::new(DynamoDbClient::from_conf((&config).into()));
        let dynamodb_test_setup = DynamoDbTestSetup::new(dynamodb_client.clone()).await;

        // Initialize JWT manager
        let jwt_manager = Arc::new(JwtManager::new(&environment).await);

        // Initialize auth proof storage with test table
        let auth_proof_storage = Arc::new(AuthProofStorage::new(
            dynamodb_client.clone(),
            dynamodb_test_setup.auth_proofs_table_name.clone(),
        ));

        let router = routes::handler()
            .layer(Extension(environment.clone()))
            .layer(Extension(media_storage.clone()))
            .layer(Extension(auth_proof_storage.clone()))
            .layer(Extension(jwt_manager.clone()))
            .into();

        Self {
            router,
            environment,
            s3_client,
            bucket_name,
            media_storage,
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
}
