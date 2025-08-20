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
    /// Push ID Rotated At - timestamp when push ID was last changed
    PushIdRotatedAt,
    /// TTL timestamp
    Ttl,
}

/// Auth proof data structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthProof {
    /// World ID Nullifier (Primary Key)
    pub nullifier: String,
    /// Encrypted Push notification ID
    /// It's used to identify the user and send notifications, see Push Subscription storage for more details.
    /// It's encrypted with the enclave's public key with an added nonce, only the enclave can decrypt it.
    pub encrypted_push_id: String,
    /// Push ID Rotated At - timestamp when push ID was last changed (rounded to nearest day)
    pub push_id_rotated_at: i64,
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

/// `DynamoDB` storage for World ID authentication proofs.
///
/// Stores the mapping between World ID nullifiers (anonymous user identifiers) and
/// encrypted push notification IDs. Each nullifier can only have one active session,
/// providing Sybil resistance.
///
/// There is a 6-8 month randomly picked TTL to avoid keeping user's data forever.
/// The TTL is refreshed every time the user issues a new JWT using the `ping_auth_proof` method.
///
/// The `push_id_rotated_at` is used to track when the push ID was last changed. In the app layer we will handle cooldown period to avoid
/// impersonation attacks.
#[derive(Clone)]
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

    /// Rounds a timestamp to the nearest day (midnight UTC)
    ///
    /// This improves privacy by not storing exact activity times.
    /// If the time is past noon, it rounds to the next day;
    /// otherwise, it rounds to the current day.
    const fn round_to_nearest_day(timestamp: i64) -> i64 {
        const SECONDS_IN_DAY: i64 = 86400;
        const HALF_DAY_SECONDS: i64 = 43200;

        let seconds_since_midnight = timestamp % SECONDS_IN_DAY;

        if seconds_since_midnight >= HALF_DAY_SECONDS {
            // Round up to next day's midnight
            timestamp + (SECONDS_IN_DAY - seconds_since_midnight)
        } else {
            // Round down to current day's midnight
            timestamp - seconds_since_midnight
        }
    }

    /// Generates a random TTL between 6-8 months from now
    ///
    /// The TTL is refreshed every time the user issues a new JWT using the `ping_auth_proof` method.
    ///
    /// # Why randomness?
    ///
    /// We add randomness to avoid "last seen" tracking. Since the TTL is updated every time a new JWT is issued,
    /// without randomness, an observer could determine when a user was last active by looking at the TTL value.
    /// The random component makes it impossible to correlate TTL values with actual user activity patterns.
    ///
    /// # Why 6-8 months?
    ///
    /// This period is chosen to proactively delete stale user data. If a user hasn't used chat from World App
    /// in this period, we consider their data stale and delete it. Once the user log ins again, they will create a new auth proof row.
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
        let rounded_now = Self::round_to_nearest_day(now);
        let ttl = Self::generate_ttl();

        let auth_proof = AuthProof {
            nullifier: auth_proof_request.nullifier.clone(),
            encrypted_push_id: auth_proof_request.encrypted_push_id.clone(),
            push_id_rotated_at: rounded_now,
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
    /// This method updates the `encrypted_push_id`, `push_id_rotated_at` timestamp (rounded to nearest day), and `ttl`.
    /// Unlike `ping_auth_proof`, this method DOES update `push_id_rotated_at` since it's
    /// modifying actual user data (not just keeping the row alive).
    /// The timestamp is rounded to the nearest day for privacy reasons.
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
        let rounded_now = Self::round_to_nearest_day(now);
        let ttl = Self::generate_ttl();

        self.dynamodb_client
            .update_item()
            .table_name(&self.table_name)
            .key("nullifier", AttributeValue::S(nullifier.to_string()))
            .update_expression(
                "SET #encrypted_push_id = :encrypted_push_id, #push_id_rotated_at = :push_id_rotated_at, #ttl = :ttl",
            )
            .expression_attribute_names(
                "#encrypted_push_id",
                AuthProofAttribute::EncryptedPushId.to_string(),
            )
            .expression_attribute_values(
                ":encrypted_push_id",
                AttributeValue::S(encrypted_push_id.to_string()),
            )
            .expression_attribute_names("#push_id_rotated_at", AuthProofAttribute::PushIdRotatedAt.to_string())
            .expression_attribute_values(":push_id_rotated_at", AttributeValue::N(rounded_now.to_string()))
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

    /// Atomically gets an existing auth proof or inserts a new one if it doesn't exist
    ///
    /// This method performs an atomic get-or-insert operation in a single `DynamoDB` request
    /// without using transactions. It uses `UpdateItem` with conditional expressions to:
    /// - Return the existing item if it exists
    /// - Create and return a new item if it doesn't exist
    ///
    /// # Arguments
    ///
    /// * `auth_proof_request` - The auth proof to insert if it doesn't exist
    ///
    /// # Returns
    ///
    /// Returns the existing auth proof if found, or the newly created auth proof
    ///
    /// # Errors
    ///
    /// Returns `AuthProofStorageError` if the `DynamoDB` operation fails
    pub async fn get_or_insert(
        &self,
        auth_proof_request: AuthProofInsertRequest,
    ) -> AuthProofStorageResult<AuthProof> {
        let now = Utc::now().timestamp();
        let rounded_now = Self::round_to_nearest_day(now);
        let ttl = Self::generate_ttl();

        let response = self
            .dynamodb_client
            .update_item()
            .table_name(&self.table_name)
            .key(
                AuthProofAttribute::Nullifier.to_string(),
                AttributeValue::S(auth_proof_request.nullifier.clone()),
            )
            // Only set these attributes if they don't already exist
            .update_expression(
                "SET #encrypted_push_id = if_not_exists(#encrypted_push_id, :encrypted_push_id), \
                 #push_id_rotated_at = if_not_exists(#push_id_rotated_at, :push_id_rotated_at), \
                 #ttl = if_not_exists(#ttl, :ttl)",
            )
            .expression_attribute_names(
                "#encrypted_push_id",
                AuthProofAttribute::EncryptedPushId.to_string(),
            )
            .expression_attribute_names(
                "#push_id_rotated_at",
                AuthProofAttribute::PushIdRotatedAt.to_string(),
            )
            .expression_attribute_names("#ttl", AuthProofAttribute::Ttl.to_string())
            .expression_attribute_values(
                ":encrypted_push_id",
                AttributeValue::S(auth_proof_request.encrypted_push_id.clone()),
            )
            .expression_attribute_values(
                ":push_id_rotated_at",
                AttributeValue::N(rounded_now.to_string()),
            )
            .expression_attribute_values(":ttl", AttributeValue::N(ttl.to_string()))
            // Return all attributes after the update
            .return_values(aws_sdk_dynamodb::types::ReturnValue::AllNew)
            .send()
            .await?;

        // Parse the returned item
        let item = response.attributes().ok_or_else(|| {
            AuthProofStorageError::SerializationError(
                "No attributes returned from update operation".to_string(),
            )
        })?;

        let auth_proof = serde_dynamo::from_item(item.clone())
            .map_err(|e| AuthProofStorageError::SerializationError(e.to_string()))?;

        Ok(auth_proof)
    }

    /// Pings an auth proof to refresh its TTL
    ///
    /// This method ONLY updates the TTL, not the `push_id_rotated_at` timestamp.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_round_to_nearest_day() {
        use chrono::{DateTime, Utc};

        fn parse_iso(iso_str: &str) -> i64 {
            iso_str.parse::<DateTime<Utc>>().unwrap().timestamp()
        }

        // Morning (10:30 AM) - rounds down to midnight
        let morning = parse_iso("2024-12-01T10:30:00Z");
        let morning_midnight = parse_iso("2024-12-01T00:00:00Z");
        assert_eq!(
            AuthProofStorage::round_to_nearest_day(morning),
            morning_midnight
        );

        // Afternoon (2:30 PM) - rounds up to next midnight
        let afternoon = parse_iso("2024-12-01T14:30:00Z");
        let next_midnight = parse_iso("2024-12-02T00:00:00Z");
        assert_eq!(
            AuthProofStorage::round_to_nearest_day(afternoon),
            next_midnight
        );

        // Exactly noon - rounds up
        let noon = parse_iso("2024-12-01T12:00:00Z");
        assert_eq!(AuthProofStorage::round_to_nearest_day(noon), next_midnight);

        // Just before noon - rounds down
        let before_noon = parse_iso("2024-12-01T11:59:59Z");
        assert_eq!(
            AuthProofStorage::round_to_nearest_day(before_noon),
            morning_midnight
        );

        // Exactly midnight - stays at midnight
        let midnight = parse_iso("2024-12-01T00:00:00Z");
        assert_eq!(AuthProofStorage::round_to_nearest_day(midnight), midnight);

        // One second after midnight - rounds down
        let after_midnight = parse_iso("2024-12-01T00:00:01Z");
        assert_eq!(
            AuthProofStorage::round_to_nearest_day(after_midnight),
            midnight
        );

        // Late evening - rounds up to next midnight
        let late_evening = parse_iso("2024-12-01T23:59:59Z");
        assert_eq!(
            AuthProofStorage::round_to_nearest_day(late_evening),
            next_midnight
        );
    }
}
