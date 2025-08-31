use std::sync::Arc;

use axum::{http::StatusCode, Extension};
use axum_jsonschema::Json;
use futures::future::join_all;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::{middleware::AuthenticatedUser, types::AppError};
use backend_storage::push_notification::{PushNotificationStorage, PushSubscription};

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
    Extension(push_storage): Extension<Arc<PushNotificationStorage>>,
    Json(payload): Json<SubscribeRequest>,
) -> Result<StatusCode, AppError> {
    let push_subscriptions = payload
        .subscriptions
        .into_iter()
        .map(|s| PushSubscription {
            hmac: s.hmac,
            topic: s.topic,
            ttl: s.ttl,
            encrypted_push_id: user.encrypted_push_id.clone(),
        })
        .collect::<Vec<PushSubscription>>();

    let db_operations = push_subscriptions
        .iter()
        .map(|subscription| push_storage.insert(subscription));

    // Wait for all insertions to complete - fails fast on first error
    join_all(db_operations)
        .await
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .map_err(AppError::from)?;

    Ok(StatusCode::OK)
}

#[instrument(skip(_push_storage, _payload))]
pub async fn unsubscribe(
    Extension(_push_storage): Extension<Arc<PushNotificationStorage>>,
    Json(_payload): Json<UnsubscribeRequest>,
) -> Result<StatusCode, AppError> {
    Ok(StatusCode::ACCEPTED)
}
