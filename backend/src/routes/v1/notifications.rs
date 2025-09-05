use std::sync::Arc;

use axum::{http::StatusCode, Extension};
use axum_jsonschema::Json;
use futures::future::join_all;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::{instrument, warn};

use crate::{middleware::AuthenticatedUser, types::AppError};
use backend_storage::push_subscription::{
    PushSubscription, PushSubscriptionStorage, PushSubscriptionStorageError,
};

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct CreateSubscriptionRequest {
    /// Topic for the subscription
    #[schemars(length(min = 1))]
    pub topic: String,
    /// HMAC key for subscription validation (64 hex characters)
    #[schemars(length(equal = 64))]
    pub hmac_key: String,
    /// TTL as unix timestamp
    #[schemars(range(min = 1))]
    pub ttl: i64,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct UnsubscribeRequest {
    /// HMAC key for subscription validation (64 hex characters)
    #[schemars(length(equal = 64))]
    pub hmac_key: String,
    /// Topic to unsubscribe from
    #[schemars(length(min = 1))]
    pub topic: String,
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
/// * `push_storage` - DynamoDB storage handler for push subscriptions
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
#[instrument(skip_all, fields(encrypted_push_id = user.encrypted_push_id))]
pub async fn subscribe(
    user: AuthenticatedUser,
    Extension(push_storage): Extension<Arc<PushSubscriptionStorage>>,
    Json(payload): Json<Vec<CreateSubscriptionRequest>>,
) -> Result<StatusCode, AppError> {
    // Payload is an empty array
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
        })
        .collect::<Vec<PushSubscription>>();

    let db_operations = push_subscriptions
        .iter()
        .map(|subscription| push_storage.insert(subscription));

    // Run all insertions concurrently
    let results = join_all(db_operations).await;

    for result in results {
        match result {
            Ok(()) => {}
            // We don't allow a user to update his encrypted push id, because we can't distinguish
            // between a legitimate rotation and a security risk.
            // If a user legitimately rotates his encrypted push id, it will be used in the
            // next 30-day epoch cycle where he would resubscibe with the new hmac keys.
            Err(PushSubscriptionStorageError::PushSubscriptionExists) => {
                warn!("subscription already exists");
            }
            Err(other) => {
                // Fail on any other error
                return Err(AppError::from(other));
            }
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
/// - **If the user is not the original subscriber**: The user's encrypted push ID is added to the deletion_request set,
///   this acts as a tombstone for the subscription, and if the plaintext push ids are the same it's lazily deleted.
///
/// # Arguments
///
/// * `user` - The authenticated user making the unsubscribe request
/// * `push_storage` - DynamoDB storage handler for push subscriptions
/// * `payload` - Unsubscribe request containing topic and HMAC key
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
/// - `503 SERVICE_UNAVAILABLE` - Database connectivity issues
/// - `500 INTERNAL_SERVER_ERROR` - Other unexpected errors during storage operations
#[instrument(skip(push_storage, payload))]
pub async fn unsubscribe(
    user: AuthenticatedUser,
    Extension(push_storage): Extension<Arc<PushSubscriptionStorage>>,
    Json(payload): Json<UnsubscribeRequest>,
) -> Result<StatusCode, AppError> {
    let push_subscription = push_storage
        .get_one(&payload.topic, &payload.hmac_key)
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
        push_storage
            .delete(&payload.topic, &payload.hmac_key)
            .await?;
    } else {
        // Add the user's encrypted push id to the deletion request using native DynamoDB string set ADD
        push_storage
            .append_delete_request(&payload.topic, &payload.hmac_key, &user.encrypted_push_id)
            .await?;
    }

    Ok(StatusCode::NO_CONTENT)
}
