use std::sync::Arc;

use axum::{extract::Query, http::StatusCode, Extension, Json};
use axum_valid::Valid;
use futures::future::join_all;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::{middleware::AuthenticatedUser, types::AppError};
use backend_storage::push_subscription::{PushSubscription, PushSubscriptionStorage};
use common_types::EnclaveTrack;

/// In the context of XMTP hmac keys for a conversation are rotated every 30-day epoch cycle
/// We set a maximum of 40 days to prevent bad actors subscribing to a topic for a longer period of time
const MAX_TTL_SECS: i64 = 40 * 24 * 60 * 60; // 40 days

#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(deny_unknown_fields)]
pub struct CreateSubscriptionRequest {
    /// Topic for the subscription
    #[validate(length(min = 1))]
    pub topic: String,
    /// HMAC key for subscription validation (42 bytes or 84 hex characters)
    #[validate(length(equal = 84))]
    pub hmac_key: String,
    /// TTL as unix timestamp
    #[validate(custom(function = "validate_ttl"))]
    pub ttl: i64,
}

#[derive(Debug, Deserialize, JsonSchema, Validate)]
pub struct UnsubscribeQuery {
    /// HMAC key for subscription validation (42 bytes or 84 hex characters)
    #[validate(length(equal = 84))]
    pub hmac_key: String,
    /// Topic to unsubscribe from
    #[validate(length(min = 1))]
    pub topic: String,
}

// Custom validator for TTL
fn validate_ttl(ttl: i64) -> Result<(), validator::ValidationError> {
    let now = chrono::Utc::now().timestamp();

    // strictly > now + 1 second
    if ttl <= now + 1 {
        let mut error = validator::ValidationError::new("invalid_ttl");
        error.message = Some(std::borrow::Cow::Borrowed(
            "TTL must be greater than now + 1 second",
        ));
        return Err(error);
    }

    // strictly < now + 40 days
    if ttl >= now + MAX_TTL_SECS {
        let mut error = validator::ValidationError::new("invalid_ttl");
        error.message = Some(std::borrow::Cow::Borrowed(
            "TTL must be less than 40 days in the future",
        ));
        return Err(error);
    }

    Ok(())
}

/// Subscribe to push notifications for multiple topics
///
/// Creates push notification subscriptions for the authenticated user. Each subscription
/// associates a topic with an HMAC key for validation and includes a TTL for automatic cleanup.
///
/// ## Idempotent Behavior
///
/// If a subscription already exists for the same topic and HMAC key, the error is logged
/// but the operation continues successfully. This provides idempotent behavior while maintaining
/// security - users cannot update their encrypted push ID for existing subscriptions to prevent
/// distinguishing between legitimate key rotation and potential security risks.
///
/// ## Push ID Rotation Security
///
/// When users legitimately rotate their encrypted push ID, they should resubscribe with new
/// HMAC keys in the next 30-day epoch cycle rather than updating existing subscriptions.
///
/// # Arguments
///
/// * `user` - The authenticated user making the subscription request
/// * `push_storage` - `DynamoDB` storage handler for push subscriptions
/// * `payload` - Array of subscription requests, each containing topic, HMAC key, and TTL
///
/// # Returns
///
/// Returns `201 CREATED` on successful subscription creation, even if some subscriptions
/// already existed (idempotent operation).
///
/// # Errors
///
/// Returns an error if:
/// - `400 BAD_REQUEST` - Empty payload array
/// - `401 UNAUTHORIZED` - Invalid or missing authentication
/// - `503 SERVICE_UNAVAILABLE` - Database connectivity issues
/// - `500 INTERNAL_SERVER_ERROR` - Other unexpected errors during storage operations
pub async fn subscribe(
    user: AuthenticatedUser,
    Extension(push_storage): Extension<Arc<PushSubscriptionStorage>>,
    Valid(Json(payload)): Valid<Json<Vec<CreateSubscriptionRequest>>>,
) -> Result<StatusCode, AppError> {
    // Validate that the payload is not empty
    if payload.is_empty() {
        return Err(AppError::new(
            StatusCode::BAD_REQUEST,
            "empty_payload",
            "Empty payload",
            false,
        ));
    }

    let push_subscriptions = payload
        .into_iter()
        .map(|s| PushSubscription {
            hmac_key: s.hmac_key,
            deletion_request: None,
            topic: s.topic,
            ttl: s.ttl,
            encrypted_push_id: user.encrypted_push_id.clone(),
            enclave_track: EnclaveTrack::default(),
        })
        .collect::<Vec<PushSubscription>>();

    let db_operations = push_subscriptions
        .iter()
        .map(|subscription| push_storage.upsert(subscription));

    // Run all upserts concurrently
    let results = join_all(db_operations).await;

    for result in results {
        if let Err(e) = result {
            return Err(AppError::from(e));
        }
    }

    Ok(StatusCode::CREATED)
}

/// Unsubscribe from push notifications for a specific topic
///
/// Removes or marks for deletion a push notification subscription. The behavior depends on
/// whether the requesting user is the original subscriber:
///
/// - **If the user is the original subscriber**: The subscription is immediately deleted
/// - **If the user is not the original subscriber**: The user's encrypted push ID is added to the `deletion_request` set,
///   this acts as a tombstone for the subscription, and if the plaintext push ids are the same it's lazily deleted.
///
/// # Arguments
///
/// * `user` - The authenticated user making the unsubscribe request
/// * `push_storage` - `DynamoDB` storage handler for push subscriptions
/// * `query` - Query parameters containing topic and HMAC key
///
/// # Returns
///
/// Returns `204 NO_CONTENT` on successful unsubscription or deletion request.
///
/// # Errors
///
/// Returns an error if:
/// - `404 NOT_FOUND` - Subscription with the given topic and HMAC key does not exist
/// - `401 UNAUTHORIZED` - Invalid or missing authentication
/// - `400 BAD_REQUEST` - Missing or invalid query parameters
/// - `500 INTERNAL_SERVER_ERROR` - Other unexpected errors during storage operations
pub async fn unsubscribe(
    user: AuthenticatedUser,
    Extension(push_storage): Extension<Arc<PushSubscriptionStorage>>,
    Query(query): Query<UnsubscribeQuery>,
) -> Result<StatusCode, AppError> {
    // Validate that fields are not empty
    if query.topic.is_empty() {
        return Err(AppError::new(
            StatusCode::BAD_REQUEST,
            "empty_topic",
            "Topic cannot be empty",
            false,
        ));
    }

    if query.hmac_key.is_empty() || query.hmac_key.len() != 84 {
        return Err(AppError::new(
            StatusCode::BAD_REQUEST,
            "invalid_hmac_key",
            "HMAC key must be exactly 84 characters",
            false,
        ));
    }

    let push_subscription = push_storage
        .get_one(&query.topic, &query.hmac_key)
        .await?
        .ok_or_else(|| {
            AppError::new(
                StatusCode::NOT_FOUND,
                "push_subscription_not_found",
                "Push subscription not found",
                false,
            )
        })?;

    if push_subscription.encrypted_push_id == user.encrypted_push_id {
        push_storage.delete(&query.topic, &query.hmac_key).await?;
    } else {
        // Add the user's encrypted push id to the deletion request using native DynamoDB string set ADD
        push_storage
            .append_delete_request(&query.topic, &query.hmac_key, &user.encrypted_push_id)
            .await?;
    }

    Ok(StatusCode::NO_CONTENT)
}

/// Batch unsubscribe from push notifications for multiple topics
///
/// Efficiently removes or marks for deletion multiple push notification subscriptions.
/// The behavior for each subscription depends on whether the requesting user is the original subscriber:
///
/// - **If the user is the original subscriber**: The subscription is immediately deleted
/// - **If the user is not the original subscriber**: The user's encrypted push ID is added to the `deletion_request` set,
///   this acts as a tombstone for the subscription, and if the plaintext push ids are the same it's lazily deleted.
///
/// ## Efficiency
///
/// This endpoint uses `DynamoDB` batch operations to minimize round trips:
/// 1. Batch fetch all subscriptions (chunks of 25)
/// 2. Partition into deletions vs tombstones based on push ID match
/// 3. Execute batch delete and parallel tombstone updates concurrently
///
/// # Arguments
///
/// * `user` - The authenticated user making the unsubscribe request
/// * `push_storage` - `DynamoDB` storage handler for push subscriptions
/// * `payload` - Array of unsubscribe requests, each containing topic and HMAC key
///
/// # Returns
///
/// Returns `204 NO_CONTENT` on successful batch unsubscription.
/// Subscriptions that don't exist are silently skipped (idempotent behavior).
///
/// # Errors
///
/// Returns an error if:
/// - `400 BAD_REQUEST` - Empty payload or invalid parameters
/// - `401 UNAUTHORIZED` - Invalid or missing authentication
/// - `500 INTERNAL_SERVER_ERROR` - Database operation failures
pub async fn batch_unsubscribe(
    user: AuthenticatedUser,
    Extension(push_storage): Extension<Arc<PushSubscriptionStorage>>,
    Valid(Json(payload)): Valid<Json<Vec<UnsubscribeQuery>>>,
) -> Result<StatusCode, AppError> {
    // Validate payload is not empty
    if payload.is_empty() {
        return Err(AppError::new(
            StatusCode::BAD_REQUEST,
            "empty_payload",
            "Empty payload",
            false,
        ));
    }

    // Step 1: Batch fetch all subscriptions
    let subscription_keys: Vec<(&str, &str)> = payload
        .iter()
        .map(|p| (p.topic.as_str(), p.hmac_key.as_str()))
        .collect();

    let subscriptions = push_storage.batch_get(&subscription_keys).await?;

    // Step 2: Partition into delete vs tombstone based on push ID match
    // Subscriptions not found are silently skipped (idempotent behavior)
    let (to_delete, to_tombstone): (Vec<_>, Vec<_>) = subscriptions
        .iter()
        .partition(|s| s.encrypted_push_id == user.encrypted_push_id);

    let to_delete: Vec<_> = to_delete
        .iter()
        .map(|s| (s.topic.as_str(), s.hmac_key.as_str()))
        .collect();

    // Step 3: Execute deletions and tombstones concurrently
    let delete_future = push_storage.batch_delete_many(&to_delete);

    let tombstone_future = async {
        // Tombstones are best-effort - log errors but don't propagate
        // DynamoDB has no batch update, so we run individual updates in parallel
        let futures: Vec<_> = to_tombstone
            .iter()
            .map(|subscription| {
                push_storage.append_delete_request(
                    &subscription.topic,
                    &subscription.hmac_key,
                    &user.encrypted_push_id,
                )
            })
            .collect();

        let results = join_all(futures).await;
        for (i, result) in results.into_iter().enumerate() {
            if let Err(e) = result {
                let subscription = to_tombstone[i];
                tracing::error!(
                    topic = subscription.topic,
                    hmac_key = subscription.hmac_key,
                    error = ?e,
                    "Failed to tombstone subscription"
                );
            }
        }
    };

    let (delete_result, ()) = tokio::join!(delete_future, tombstone_future);
    delete_result?;

    Ok(StatusCode::NO_CONTENT)
}
