//! Auth proof storage integration using Dynamo DB
//!
//! Auth Proof Storage is used to store latest encrypted push id for a user

mod error;

use std::sync::Arc;

use aws_sdk_dynamodb::{error::SdkError, types::AttributeValue, Client as DynamoDbClient};
use chrono::Utc;
use rand::Rng;
use serde::{Deserialize, Serialize};

pub use error::{AuthProofStorageError, AuthProofStorageResult};
use strum::Display;

/// TTL boundaries for random selection (in seconds)
const TTL_MIN_SECONDS: i64 = 6 * 30 * 24 * 60 * 60; // 6 months in seconds
const TTL_MAX_SECONDS: i64 = 8 * 30 * 24 * 60 * 60; // 8 months in seconds

/// Attribute names for auth proof table
#[derive(Debug, Clone, Display)]
#[strum(serialize_all = "snake_case")]
pub enum AuthProofAttribute {
    /// Nullifier (Primary Key)
    /// Extracted from user's ZK proof
    Nullifier,
    /// Encrypted Push ID
    EncryptedPushId,
    /// Updated At
    UpdatedAt,
    /// TTL timestamp
    Ttl,
}

/// Auth proof data structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthProof {
    /// Nullifier (Primary Key)
    pub nullifier: String,
    /// Encrypted Push ID
    pub encrypted_push_id: String,
    /// Updated At
    pub updated_at: i64,
    /// TTL timestamp
    pub ttl: i64,
}

/// Auth proof data structure
#[derive(Debug, Clone, Serialize)]
pub struct AuthProofInsertRequest {
    /// Nullifier (Primary Key)
    pub nullifier: String,
    /// Encrypted Push ID
    pub encrypted_push_id: String,
}

/// Auth proof storage client for Dynamo DB operations
pub struct AuthProofStorage {
    dynamodb_client: Arc<DynamoDbClient>,
    table_name: String,
}

impl AuthProofStorage {
    /// Creates a new auth proof storage client
    ///
    /// # Arguments
    ///
    /// * `dynamodb_client` - Pre-configured Dynamo DB client
    /// * `table_name` - Dynamo DB table name for auth proofs
    #[must_use]
    pub const fn new(dynamodb_client: Arc<DynamoDbClient>, table_name: String) -> Self {
        Self {
            dynamodb_client,
            table_name,
        }
    }

    /// Generates a random TTL between 6-8 months from now
    fn generate_ttl() -> i64 {
        let now = Utc::now().timestamp();
        let mut rng = rand::thread_rng();
        let ttl_seconds = rng.gen_range(TTL_MIN_SECONDS..=TTL_MAX_SECONDS);
        now + ttl_seconds
    }

    /// Inserts a new auth proof with a random TTL between 6-8 months
    ///
    /// # Arguments
    ///
    /// * `auth_proof_request` - The auth proof to insert
    ///
    /// # Errors
    ///
    /// Returns `AuthProofStorageError` if the Dynamo DB operation fails
    pub async fn insert(
        &self,
        auth_proof_request: AuthProofInsertRequest,
    ) -> AuthProofStorageResult<AuthProof> {
        let now = Utc::now().timestamp();
        let ttl = Self::generate_ttl();

        let auth_proof = AuthProof {
            nullifier: auth_proof_request.nullifier.clone(),
            encrypted_push_id: auth_proof_request.encrypted_push_id.clone(),
            updated_at: now,
            ttl,
        };

        // Convert to DynamoDB item
        let item = serde_dynamo::to_item(&auth_proof)
            .map_err(|e| AuthProofStorageError::SerializationError(e.to_string()))?;

        self.dynamodb_client
            .put_item()
            .table_name(&self.table_name)
            .set_item(Some(item))
            .condition_expression("attribute_not_exists(#pk)")
            .expression_attribute_names("#pk", AuthProofAttribute::Nullifier.to_string())
            .send()
            .await
            .map_err(|err| {
                if matches!(
                    err,
                    SdkError::ServiceError(ref svc) if svc.err().is_conditional_check_failed_exception()
                ) {
                    AuthProofStorageError::AuthProofExists
                } else {
                    err.into()
                }
            })?;

        Ok(auth_proof)
    }

    /// Updates the encrypted push id for a given nullifier and refreshes TTL
    ///
    /// This should only happen when the plaintext push id changes.
    ///
    /// This method updates the `encrypted_push_id`, `updated_at` timestamp, and `ttl`.
    /// Unlike `ping_auth_proof`, this method DOES update `updated_at` since it's
    /// modifying actual user data (not just keeping the row alive).
    ///
    /// # Arguments
    ///
    /// * `nullifier` - The nullifier of the auth proof to update
    /// * `encrypted_push_id` - The new encrypted push id
    ///
    /// # Errors
    ///
    /// Returns `AuthProofStorageError` if the Dynamo DB operation fails
    pub async fn update_encrypted_push_id(
        &self,
        nullifier: &str,
        encrypted_push_id: &str,
    ) -> AuthProofStorageResult<()> {
        let now = Utc::now().timestamp();
        let ttl = Self::generate_ttl();

        self.dynamodb_client
            .update_item()
            .table_name(&self.table_name)
            .key("nullifier", AttributeValue::S(nullifier.to_string()))
            .update_expression(
                "SET #encrypted_push_id = :encrypted_push_id, #updated_at = :updated_at, #ttl = :ttl",
            )
            .expression_attribute_names(
                "#encrypted_push_id",
                AuthProofAttribute::EncryptedPushId.to_string(),
            )
            .expression_attribute_values(
                ":encrypted_push_id",
                AttributeValue::S(encrypted_push_id.to_string()),
            )
            .expression_attribute_names("#updated_at", AuthProofAttribute::UpdatedAt.to_string())
            .expression_attribute_values(":updated_at", AttributeValue::N(now.to_string()))
            .expression_attribute_names("#ttl", AuthProofAttribute::Ttl.to_string())
            .expression_attribute_values(":ttl", AttributeValue::N(ttl.to_string()))
            .send()
            .await?;

        Ok(())
    }

    /// Gets a auth proof by nullifier
    ///
    /// # Arguments
    ///
    /// * `nullifier` - The nullifier of the auth proof to get
    ///
    /// # Errors
    ///
    /// Returns `AuthProofStorageError` if the Dynamo DB operation fails    
    pub async fn get_by_nullifier(
        &self,
        nullifier: &str,
    ) -> AuthProofStorageResult<Option<AuthProof>> {
        let response = self
            .dynamodb_client
            .get_item()
            .table_name(&self.table_name)
            .key(
                AuthProofAttribute::Nullifier.to_string(),
                AttributeValue::S(nullifier.to_string()),
            )
            .send()
            .await?;

        let item = response
            .item()
            .map(|item| serde_dynamo::from_item(item.clone()))
            .transpose()
            .map_err(|e| AuthProofStorageError::SerializationError(e.to_string()))?;

        Ok(item)
    }

    /// Pings an auth proof to refresh its TTL
    ///
    /// This method ONLY updates the TTL, not the `updated_at` timestamp.
    /// This is intentional for privacy reasons - we want to keep users' data alive
    /// without tracking their activity patterns (no "last seen" tracking).
    ///
    /// # Arguments
    ///
    /// * `nullifier` - The nullifier of the auth proof to ping
    ///
    /// # Errors
    ///
    /// Returns `AuthProofStorageError` if the Dynamo DB operation fails
    pub async fn ping_auth_proof(&self, nullifier: &str) -> AuthProofStorageResult<()> {
        let ttl = Self::generate_ttl();

        self.dynamodb_client
            .update_item()
            .table_name(&self.table_name)
            .key("nullifier", AttributeValue::S(nullifier.to_string()))
            .update_expression("SET #ttl = :ttl")
            .expression_attribute_names("#ttl", AuthProofAttribute::Ttl.to_string())
            .expression_attribute_values(":ttl", AttributeValue::N(ttl.to_string()))
            .send()
            .await?;

        Ok(())
    }
}
