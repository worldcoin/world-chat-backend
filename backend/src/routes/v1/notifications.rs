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

/// Subscribe to push notifications for the given topics
///
/// # Errors
///
/// Returns an error if:
/// - Database operations fail during subscription storage
/// - Subscription validation fails
/// - Internal server errors occur
#[instrument(skip_all, fields(encrypted_push_id = user.encrypted_push_id))]
pub async fn subscribe(
    user: AuthenticatedUser,
    Extension(push_storage): Extension<Arc<PushSubscriptionStorage>>,
    Json(payload): Json<Vec<CreateSubscriptionRequest>>,
) -> Result<StatusCode, AppError> {
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
            Err(PushSubscriptionStorageError::PushSubscriptionExists) => {
                // Log warning but don't fail
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
/// # Errors
///
/// Returns an error if:
/// - Database operations fail during subscription retrieval
/// - The subscription does not exist
/// - Internal server errors occur
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
