use std::collections::HashSet;
use std::sync::Arc;

use axum::{http::StatusCode, Extension};
use axum_jsonschema::Json;
use futures::future::join_all;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::types::AppError;
use backend_storage::{
    push_notification::PushNotificationStorage,
    queue::{SubscriptionRequest, SubscriptionRequestQueue},
};

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct Subscription {
    /// Topic for the subscription
    pub topic: String,
    /// HMAC for subscription validation
    pub hmac: String,
    /// TTL as unix timestamp
    #[schemars(range(min = 1))]
    pub ttl: i64,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct SubscribeRequest {
    /// Encrypted Braze ID
    pub encrypted_braze_id: String,
    /// Array of subscriptions
    pub subscriptions: Vec<Subscription>,
}

/// Creates or updates push notification subscriptions
///
/// This function handles subscribing to push notifications:
/// 1. Validates the subscription request
/// 2. Filters out existing subscriptions
/// 3. Queues new subscription requests for processing
/// 4. Returns 202 Accepted status
///
/// # Arguments
///
/// * `subscription_queue` - The subscription request queue service instance
/// * `push_storage` - The push notification storage service instance
/// * `payload` - Subscribe request containing encrypted Braze ID and subscriptions
///
/// # Returns
///
/// Returns `Ok(StatusCode::ACCEPTED)` when subscriptions are successfully queued
///
/// # Errors
///
/// This function can return the following errors:
/// - Storage errors when checking existing subscriptions
/// - Queue errors when sending subscription requests
/// - Validation errors for invalid input
#[instrument(skip(subscription_queue, push_storage, payload))]
pub async fn subscribe(
    Extension(subscription_queue): Extension<Arc<SubscriptionRequestQueue>>,
    Extension(push_storage): Extension<Arc<PushNotificationStorage>>,
    Json(payload): Json<SubscribeRequest>,
) -> Result<StatusCode, AppError> {
    // Extract all HMACs from the request
    let hmacs: Vec<String> = payload
        .subscriptions
        .iter()
        .map(|s| s.hmac.clone())
        .collect();

    // Check which HMACs already exist in the database
    let existing_hmacs = push_storage.get_by_hmacs(&hmacs).await?;
    let existing_set: HashSet<_> = existing_hmacs.into_iter().collect();

    // Send all new subscriptions concurrently
    let send_futures = payload
        .subscriptions
        .into_iter()
        .filter(|s| !existing_set.contains(&s.hmac))
        .map(|subscription| {
            let queue_clone = subscription_queue.clone();
            let encrypted_braze_id = payload.encrypted_braze_id.clone();

            async move {
                let subscription_request = SubscriptionRequest::Subscribe {
                    hmac: subscription.hmac.clone(),
                    encrypted_braze_id,
                    topic: subscription.topic.clone(),
                    ttl: subscription.ttl,
                };

                queue_clone
                    .send_message(&subscription_request)
                    .await
                    .map_err(|e| {
                        tracing::warn!(
                            "Failed to queue subscription for hmac {}: {:?}",
                            subscription.hmac,
                            e
                        );
                    })
            }
        });

    // Execute all sends concurrently
    join_all(send_futures).await;

    Ok(StatusCode::ACCEPTED)
}
