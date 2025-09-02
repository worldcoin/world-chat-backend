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

pub use error::{PushNotificationStorageError, PushNotificationStorageResult};
use strum::Display;

/// Attribute names for push subscription table
#[derive(Debug, Clone, Display)]
#[strum(serialize_all = "snake_case")]
pub enum PushSubscriptionAttribute {
    /// HMAC identifier (Primary Key)
    Hmac,
    /// This field is a Global Secondary Index.
    ///
    /// This field references the conversation a user has enabled push notifications for.
    /// Topic or Topic ID is interchangeably used in the XMTP docs.
    ///
    /// Source: `https://docs.xmtp.org/inboxes/push-notifs/understand-push-notifs`
    Topic,
    /// TTL timestamp
    Ttl,
    /// Encrypted Push ID
    EncryptedPushId,
}

/// Push subscription data structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushSubscription {
    /// HMAC identifier (Primary Key)
    pub hmac: String,
    /// Topic name (Global Secondary Index)
    pub topic: String,
    /// TTL timestamp (Unix timestamp in seconds, rounded to minute)
    pub ttl: i64,
    /// Encrypted Push ID
    pub encrypted_push_id: String,
}

/// Push notification storage client for Dynamo DB operations
pub struct PushNotificationStorage {
    dynamodb_client: Arc<DynamoDbClient>,
    table_name: String,
    gsi_name: String,
}

impl PushNotificationStorage {
    /// Creates a new push notification storage client
    ///
    /// # Arguments
    ///
    /// * `dynamodb_client` - Pre-configured Dynamo DB client
    /// * `table_name` - Dynamo DB table name for push subscriptions
    /// * `gsi_name` - Global Secondary Index name for topic queries
    #[must_use]
    pub const fn new(
        dynamodb_client: Arc<DynamoDbClient>,
        table_name: String,
        gsi_name: String,
    ) -> Self {
        Self {
            dynamodb_client,
            table_name,
            gsi_name,
        }
    }

    /// Inserts a new push subscription
    ///
    /// # Arguments
    ///
    /// * `subscription` - The push subscription to insert
    ///
    /// # Errors
    ///
    /// Returns `PushNotificationStorageError` if the Dynamo DB operation fails
    pub async fn insert(
        &self,
        subscription: &PushSubscription,
    ) -> PushNotificationStorageResult<()> {
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
            .map_err(|e| PushNotificationStorageError::SerializationError(e.to_string()))?;

        self.dynamodb_client
            .put_item()
            .table_name(&self.table_name)
            .set_item(Some(item))
            .condition_expression("attribute_not_exists(#pk)")
            .expression_attribute_names("#pk", PushSubscriptionAttribute::Hmac.to_string())
            .send()
            .await
            .map_err(|err| {
                if matches!(
                    err,
                    SdkError::ServiceError(ref svc) if svc.err().is_conditional_check_failed_exception()
                ) {
                    PushNotificationStorageError::PushSubscriptionExists
                } else {
                    err.into()
                }
            })?;

        Ok(())
    }

    /// Deletes a push subscription by HMAC
    ///
    /// # Arguments
    ///
    /// * `hmac` - The HMAC identifier of the subscription to delete
    ///
    /// # Errors
    ///
    /// Returns `PushNotificationStorageError` if the Dynamo DB operation fails
    pub async fn delete_by_hmac(&self, hmac: &str) -> PushNotificationStorageResult<()> {
        self.dynamodb_client
            .delete_item()
            .table_name(&self.table_name)
            .key(
                PushSubscriptionAttribute::Hmac.to_string(),
                AttributeValue::S(hmac.to_string()),
            )
            .send()
            .await?;

        Ok(())
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
    /// Returns `PushNotificationStorageError` if the Dynamo DB operation fails
    pub async fn get_all_by_topic(
        &self,
        topic: &str,
    ) -> PushNotificationStorageResult<Vec<PushSubscription>> {
        let response = self
            .dynamodb_client
            .query()
            .table_name(&self.table_name)
            .index_name(&self.gsi_name)
            .key_condition_expression("#topic = :topic")
            .expression_attribute_names("#topic", PushSubscriptionAttribute::Topic.to_string())
            .expression_attribute_values(":topic", AttributeValue::S(topic.to_string()))
            .select(Select::AllAttributes)
            .send()
            .await?;

        let items = response.items();
        items
            .iter()
            .map(|item| {
                serde_dynamo::from_item(item.clone()).map_err(|e| {
                    PushNotificationStorageError::ParseSubscriptionError(e.to_string())
                })
            })
            .collect()
    }

    /// Checks if a push subscription exists by HMAC
    ///
    /// # Arguments
    ///
    /// * `hmac` - The HMAC identifier to check
    ///
    /// # Returns
    ///
    /// * `Ok(true)` if subscription exists
    /// * `Ok(false)` if subscription does not exist
    ///
    /// # Errors
    ///
    /// Returns `PushNotificationStorageError` if the Dynamo DB operation fails
    pub async fn exists_by_hmac(&self, hmac: &str) -> PushNotificationStorageResult<bool> {
        let response = self
            .dynamodb_client
            .get_item()
            .table_name(&self.table_name)
            .key(
                PushSubscriptionAttribute::Hmac.to_string(),
                AttributeValue::S(hmac.to_string()),
            )
            .projection_expression(PushSubscriptionAttribute::Hmac.to_string())
            .send()
            .await?;

        Ok(response.item().is_some())
    }
}
