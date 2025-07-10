use crate::common::*;
use aws_sdk_s3::Client as S3Client;
use axum::{body::Body, http::Request, response::Response, Extension, Router};
use backend::{media_storage::MediaStorage, routes, types::Environment};
use std::sync::Arc;
use tower::ServiceExt;

/// E2E test setup with real dependencies
pub struct E2ETestSetup {
    pub router: Router,
    pub s3_client: Arc<S3Client>,
    pub bucket_name: String,
}

impl E2ETestSetup {
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

        let router = routes::handler()
            .layer(Extension(environment.clone()))
            .layer(Extension(media_storage.clone()))
            .into();

        Self {
            router,
            s3_client,
            bucket_name,
        }
    }
}

impl E2ETestSetup {
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
        response: axum::response::Response,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        use http_body_util::BodyExt;

        let body = response.into_body().collect().await?.to_bytes();
        let json = serde_json::from_slice(&body)?;
        Ok(json)
    }
}
