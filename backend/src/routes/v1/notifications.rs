use std::{collections::HashSet, sync::Arc};

use axum::{http::StatusCode, Extension};
use axum_jsonschema::Json;
use futures::future::join_all;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::{middleware::AuthenticatedUser, types::AppError};
use backend_storage::push_subscription::{PushSubscription, PushSubscriptionStorage};

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct CreateSubscriptionRequest {
    /// Topic for the subscription
    pub topic: String,
    /// HMAC key for subscription validation
    pub hmac: String,
    /// TTL as unix timestamp
    #[schemars(range(min = 1))]
    pub ttl: i64,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct UnsubscribeRequest {
    /// HMAC key to unsubscribe from
    pub hmac: String,
    /// Topic to unsubscribe from
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
#[instrument(skip(push_storage, payload))]
pub async fn subscribe(
    user: AuthenticatedUser,
    Extension(push_storage): Extension<Arc<PushSubscriptionStorage>>,
    Json(payload): Json<Vec<CreateSubscriptionRequest>>,
) -> Result<StatusCode, AppError> {
    let push_subscriptions = payload
        .into_iter()
        .map(|s| PushSubscription {
            hmac_key: s.hmac,
            deletion_request: None,
            topic: s.topic,
            ttl: s.ttl,
            encrypted_push_id: user.encrypted_push_id.clone(),
        })
        .collect::<Vec<PushSubscription>>();

    let db_operations = push_subscriptions
        .iter()
        .map(|subscription| push_storage.upsert(subscription));

    // Wait for all insertions to complete - fails fast on first error
    join_all(db_operations)
        .await
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .map_err(AppError::from)?;

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
    let mut push_subscription = match push_storage
        .get_one(&payload.topic, &payload.hmac)
        .await
        .map_err(AppError::from)?
    {
        Some(subscription) => subscription,
        None => {
            return Err(AppError::new(
                StatusCode::NOT_FOUND,
                "push_subscription_not_found",
                "Push subscription not found",
                false,
            ));
        }
    };

    if push_subscription.encrypted_push_id == user.encrypted_push_id {
        push_storage
            .delete(&payload.topic, &payload.hmac)
            .await
            .map_err(AppError::from)?;
    } else {
        // Add the user's encrypted push id to the deletion request
        push_subscription
            .deletion_request
            .get_or_insert_with(HashSet::new)
            .insert(user.encrypted_push_id);
        push_storage
            .upsert(&push_subscription)
            .await
            .map_err(AppError::from)?;
    }

    Ok(StatusCode::NO_CONTENT)
}
