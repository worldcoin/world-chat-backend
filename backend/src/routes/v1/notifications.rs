use std::sync::Arc;

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
    /// Array of subscriptions
    pub subscriptions: Vec<Subscription>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct UnsubscribeRequest {
    /// Encrypted Push ID
    pub encrypted_push_id: String,
    /// HMAC to unsubscribe from -- Identifier for a user's notification subscription
    pub hmac: String,
    /// Topic to unsubscribe from
    pub topic: String,
}

#[instrument(skip(push_storage, payload))]
pub async fn subscribe(
    user: AuthenticatedUser,
    Extension(push_storage): Extension<Arc<PushSubscriptionStorage>>,
    Json(payload): Json<SubscribeRequest>,
) -> Result<StatusCode, AppError> {
    let push_subscriptions = payload
        .subscriptions
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

    Ok(StatusCode::OK)
}

#[instrument(skip(push_storage, payload))]
pub async fn unsubscribe(
    Extension(push_storage): Extension<Arc<PushSubscriptionStorage>>,
    Json(payload): Json<UnsubscribeRequest>,
) -> Result<StatusCode, AppError> {
    push_storage
        .get_one(&payload.topic, &payload.hmac)
        .await
        .map_err(AppError::from)?;

    // TODO: tombstone logic here

    Ok(StatusCode::OK)
}
