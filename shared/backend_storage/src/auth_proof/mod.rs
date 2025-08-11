//! Auth proof storage integration using Dynamo DB
//!
//! Auth Proof Storage is used to store latest encrypted push id for a user

mod error;

use std::sync::Arc;

use aws_sdk_dynamodb::{error::SdkError, types::AttributeValue, Client as DynamoDbClient};
use chrono::Utc;
use serde::{Deserialize, Serialize};

pub use error::{AuthProofStorageError, AuthProofStorageResult};
use strum::Display;

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

    /// Inserts a new auth proof
    ///
    /// # Arguments
    ///
    /// * `auth_proof` - The auth proof to insert
    ///
    /// # Errors
    ///
    /// Returns `AuthProofStorageError` if the Dynamo DB operation fails
    pub async fn insert(&self, auth_proof: &AuthProof) -> AuthProofStorageResult<()> {
        // Convert to DynamoDB item
        let item = serde_dynamo::to_item(auth_proof)
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

        Ok(())
    }

    /// Updates the encrypted push id for a given nullifier
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
        self.dynamodb_client
            .update_item()
            .table_name(&self.table_name)
            .key("nullifier", AttributeValue::S(nullifier.to_string()))
            .update_expression(
                "SET #encrypted_push_id = :encrypted_push_id, #updated_at = :updated_at",
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
            .expression_attribute_values(
                ":updated_at",
                AttributeValue::N(Utc::now().timestamp().to_string()),
            )
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
}
