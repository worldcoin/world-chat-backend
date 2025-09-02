//! Push notification storage integration using Dynamo DB
//!
//! Push Notification Storage holds subscription to topics, used by the backend and enclave worker

mod error;

use std::sync::Arc;

use aws_sdk_dynamodb::{
    error::SdkError,
    types::{AttributeValue, Select},
    Client as DynamoDbClient,
};
use rand::Rng;
use serde::{Deserialize, Serialize};

pub use error::{PushSubscriptionStorageError, PushSubscriptionStorageResult};
use strum::Display;

/// Attribute names for push subscription table
#[derive(Debug, Clone, Display)]
#[strum(serialize_all = "snake_case")]
pub enum PushSubscriptionAttribute {
    /// Topic (Primary Key)
    ///
    /// This field references the conversation a user has enabled push notifications for.
    /// Topic or Topic ID is interchangeably used in the XMTP docs.
    ///
    /// Source: `https://docs.xmtp.org/chat-apps/push-notifs/understand-push-notifs`
    Topic,
    /// HMAC key (Sort Key)
    ///
    /// This field is derived from the user's installation, topic and rotates every 30-day epoch cycle.
    ///
    /// Source: `https://docs.xmtp.org/chat-apps/push-notifs/understand-push-notifs#understand-hmac-keys-and-push-notifications`
    HmacKey,
    /// TTL timestamp
    Ttl,
    /// Encrypted Push ID
    EncryptedPushId,
    /// Optional set of deletion request strings
    DeletionRequest,
}

/// Push subscription data structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushSubscription {
    /// Topic name (Primary Key)
    pub topic: String,
    /// HMAC key (Sort Key)
    pub hmac_key: String,
    /// TTL timestamp (Unix timestamp in seconds, rounded to minute)
    pub ttl: i64,
    /// Encrypted Push ID
    pub encrypted_push_id: String,
    /// Optional set of deletion request strings
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deletion_request: Option<std::collections::HashSet<String>>,
}

/// Push notification storage client for Dynamo DB operations
pub struct PushSubscriptionStorage {
    dynamodb_client: Arc<DynamoDbClient>,
    table_name: String,
}

impl PushSubscriptionStorage {
    /// Creates a new push notification storage client
    ///
    /// # Arguments
    ///
    /// * `dynamodb_client` - Pre-configured Dynamo DB client
    /// * `table_name` - Dynamo DB table name for push subscriptions
    #[must_use]
    pub const fn new(dynamodb_client: Arc<DynamoDbClient>, table_name: String) -> Self {
        Self {
            dynamodb_client,
            table_name,
        }
    }

    /// Gets all push subscriptions for a specific topic
    ///
    /// # Arguments
    ///
    /// * `topic` - The topic to query subscriptions for
    ///
    /// # Returns
    ///
    /// A vector of push subscriptions for the given topic
    ///
    /// # Errors
    ///
    /// Returns `PushSubscriptionStorageError` if the Dynamo DB operation fails
    pub async fn get_all_by_topic(
        &self,
        topic: &str,
    ) -> PushSubscriptionStorageResult<Vec<PushSubscription>> {
        let response = self
            .dynamodb_client
            .query()
            .table_name(&self.table_name)
            .key_condition_expression("#topic = :topic")
            .expression_attribute_names("#topic", PushSubscriptionAttribute::Topic.to_string())
            .expression_attribute_values(":topic", AttributeValue::S(topic.to_string()))
            .select(Select::AllAttributes)
            .send()
            .await?;

        response
            .items()
            .iter()
            .map(|item| {
                serde_dynamo::from_item(item.clone()).map_err(|e| {
                    PushSubscriptionStorageError::ParseSubscriptionError(e.to_string())
                })
            })
            .collect()
    }

    /// Gets a single push subscription by topic and HMAC key
    ///
    /// # Arguments
    ///
    /// * `topic` - The topic of the subscription
    /// * `hmac_key` - The HMAC key identifier
    ///
    /// # Returns
    ///
    /// An optional push subscription if found
    ///
    /// # Errors
    ///
    /// Returns `PushSubscriptionStorageError` if the Dynamo DB operation fails
    pub async fn get_one(
        &self,
        topic: &str,
        hmac_key: &str,
    ) -> PushSubscriptionStorageResult<Option<PushSubscription>> {
        let response = self
            .dynamodb_client
            .get_item()
            .table_name(&self.table_name)
            .key(
                PushSubscriptionAttribute::Topic.to_string(),
                AttributeValue::S(topic.to_string()),
            )
            .key(
                PushSubscriptionAttribute::HmacKey.to_string(),
                AttributeValue::S(hmac_key.to_string()),
            )
            .send()
            .await?;

        response
            .item()
            .map(|item| {
                serde_dynamo::from_item(item.clone()).map_err(|e| {
                    PushSubscriptionStorageError::ParseSubscriptionError(e.to_string())
                })
            })
            .transpose()
    }

    /// Inserts a push subscription, failing if it already exists with the same topic and `hmac_key`
    ///
    /// # Arguments
    ///
    /// * `subscription` - The push subscription to insert
    ///
    /// # Errors
    ///
    /// Returns `PushSubscriptionStorageError::PushSubscriptionExists` if a subscription with the same
    /// `topic` and `hmac_key` already exists, or other `PushSubscriptionStorageError`
    /// if the Dynamo DB operation fails
    pub async fn insert(
        &self,
        subscription: &PushSubscription,
    ) -> PushSubscriptionStorageResult<()> {
        // Add random offset: 1 minute to 24 hours (uniform distribution)
        let random_offset = {
            let mut rng = rand::thread_rng();
            rng.gen_range(60..=86400) // 60 seconds to 24 hours
        };
        let distributed_ttl = subscription.ttl + random_offset;

        // Create a modified subscription with distributed TTL
        let subscription_to_store = PushSubscription {
            ttl: distributed_ttl,
            ..subscription.clone()
        };

        // Convert to DynamoDB item
        let item = serde_dynamo::to_item(&subscription_to_store)
            .map_err(|e| PushSubscriptionStorageError::SerializationError(e.to_string()))?;

        // Create only if *no item with this PK+SK* exists.
        self
            .dynamodb_client
            .put_item()
            .table_name(&self.table_name)
            .set_item(Some(item))
            .condition_expression("attribute_not_exists(#pk) AND attribute_not_exists(#sk)")
            .expression_attribute_names("#pk", PushSubscriptionAttribute::Topic.to_string())
            .expression_attribute_names("#sk", PushSubscriptionAttribute::HmacKey.to_string())
            .send()
            .await
            .map_err(|err| {
                if matches!(
                    err,
                    SdkError::ServiceError(ref svc) if svc.err().is_conditional_check_failed_exception()
                ) {
                    PushSubscriptionStorageError::PushSubscriptionExists
                } else {
                    err.into()
                }
            })?;

        Ok(())
    }

    /// Deletes a push subscription
    ///
    /// # Arguments
    ///
    /// * `topic` - The topic of the subscription
    /// * `hmac_key` - The HMAC key identifier
    ///
    /// # Errors
    ///
    /// Returns `PushSubscriptionStorageError` if the Dynamo DB operation fails
    pub async fn delete(&self, topic: &str, hmac_key: &str) -> PushSubscriptionStorageResult<()> {
        self.dynamodb_client
            .delete_item()
            .table_name(&self.table_name)
            .key(
                PushSubscriptionAttribute::Topic.to_string(),
                AttributeValue::S(topic.to_string()),
            )
            .key(
                PushSubscriptionAttribute::HmacKey.to_string(),
                AttributeValue::S(hmac_key.to_string()),
            )
            .send()
            .await?;

        Ok(())
    }
}
