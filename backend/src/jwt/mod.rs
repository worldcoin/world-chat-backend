//! JWT token management with cached secrets for high performance

pub mod error;

use std::sync::Arc;

use chrono::Utc;
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

use crate::types::Environment;
use error::JwtError;

/// JWT manager with cached secrets for blazing fast token operations
#[derive(Clone)]
pub struct JwtManager {
    signing_key: Arc<EncodingKey>,
    validation_key: Arc<DecodingKey>,
    validation: Validation,
}

/// JWT claims structure
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    /// Subject - the encrypted push ID
    pub sub: String,
    /// Expiration time (Unix timestamp)
    pub exp: i64,
    /// Issued at (Unix timestamp)
    pub iat: i64,
}

impl JwtManager {
    /// Creates a new JWT manager with the secret loaded once at startup.
    ///
    /// # Panics
    ///
    /// Panics if `JWT_SECRET_NAME` or `JWT_SECRET_ARN` environment variable is not set
    /// or if the secret cannot be loaded from AWS Secrets Manager
    pub async fn new(environment: &Environment) -> Self {
        // Always load from AWS Secrets Manager (including LocalStack in dev)
        let secret = Self::load_from_secrets_manager(environment)
            .await
            .expect("Failed to load JWT secret from Secrets Manager");

        let signing_key = Arc::new(EncodingKey::from_secret(secret.as_bytes()));
        let validation_key = Arc::new(DecodingKey::from_secret(secret.as_bytes()));
        let validation = Validation::new(Algorithm::HS256);

        tracing::info!("JWT manager initialized successfully");

        Self {
            signing_key,
            validation_key,
            validation,
        }
    }

    /// Load secret from AWS Secrets Manager (works with both AWS and `LocalStack`)
    ///
    /// # Panics
    ///
    /// Panics if `JWT_SECRET_NAME` environment variable is not set
    async fn load_from_secrets_manager(environment: &Environment) -> Result<String, JwtError> {
        use aws_sdk_secretsmanager::Client;

        // Get AWS config (will use LocalStack endpoint in development)
        let aws_config = environment.aws_config().await;
        let client = Client::new(&aws_config);

        // Secret name - must be set via environment variable
        let secret_id = std::env::var("JWT_SECRET_NAME")
            .or_else(|_| std::env::var("JWT_SECRET_ARN"))
            .expect("JWT_SECRET_NAME or JWT_SECRET_ARN environment variable must be set");

        tracing::info!("Loading JWT secret from Secrets Manager: {secret_id}");

        let response = client
            .get_secret_value()
            .secret_id(secret_id)
            .send()
            .await
            .map_err(|e| {
                JwtError::SecretLoadError(format!(
                    "Failed to fetch secret from Secrets Manager: {e}"
                ))
            })?;

        // Parse the secret - it could be plain text or JSON
        let secret_string = response.secret_string().ok_or_else(|| {
            JwtError::SecretLoadError("Secret is binary, expected string".to_string())
        })?;

        Ok(secret_string.to_string())
    }

    /// Issues a JWT token with the given subject and expiry time.
    /// This operation is BLAZING FAST (sub-millisecond) since it uses cached keys.
    ///
    /// # Errors
    ///
    /// Returns `JwtError` if JWT encoding fails
    pub fn issue_token(
        &self,
        encrypted_push_id: &str,
        expiry_secs: i64,
    ) -> Result<String, JwtError> {
        let now = Utc::now().timestamp();
        let claims = Claims {
            sub: encrypted_push_id.to_string(),
            exp: now + expiry_secs,
            iat: now,
        };

        encode(&Header::new(Algorithm::HS256), &claims, &self.signing_key).map_err(JwtError::from)
    }

    /// Validates a JWT token and returns the claims if valid.
    /// This operation is also BLAZING FAST since it uses cached keys.
    ///
    /// # Errors
    ///
    /// Returns `JwtError` if token is invalid or expired
    pub fn validate_token(&self, token: &str) -> Result<Claims, JwtError> {
        decode::<Claims>(token, &self.validation_key, &self.validation)
            .map(|data| data.claims)
            .map_err(|_| JwtError::ValidationError)
    }
}
