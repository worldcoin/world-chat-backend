use std::sync::Arc;

use axum::{http::StatusCode, Extension};
use axum_jsonschema::Json;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::{middleware::AuthenticatedUser, types::AppError};
use backend_storage::push_subscription::PushSubscriptionStorage;

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
