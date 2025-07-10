//! Push notification storage integration using DynamoDB

#![deny(
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    missing_docs,
    dead_code
)]

mod error;

use std::sync::Arc;

use aws_sdk_dynamodb::{
    types::{AttributeValue, Select},
    Client as DynamoDbClient,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub use error::{PushNotificationStorageError, PushNotificationStorageResult};

/// Push subscription data structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushSubscription {
    /// HMAC identifier (Primary Key)
    pub hmac: String,
    /// Topic name (Global Secondary Index)
    pub topic: String,
    /// TTL timestamp (Unix timestamp in seconds, rounded to minute)
    pub ttl: i64,
    /// Encrypted Braze ID
    pub encrypted_braze_id: String,
}

/// Push notification storage client for DynamoDB operations
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
    /// * `dynamodb_client` - Pre-configured DynamoDB client
    /// * `table_name` - DynamoDB table name for push subscriptions
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

    /// Rounds a timestamp to the nearest minute for privacy
    fn round_to_minute(timestamp: DateTime<Utc>) -> i64 {
        let seconds = timestamp.timestamp();
        let remainder = seconds % 60;
        if remainder >= 30 {
            seconds + (60 - remainder)
        } else {
            seconds - remainder
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
    /// Returns `PushNotificationStorageError` if the DynamoDB operation fails
    pub async fn insert(
        &self,
        subscription: &PushSubscription,
    ) -> PushNotificationStorageResult<()> {
        // Round TTL to nearest minute for privacy
        let timestamp = DateTime::from_timestamp(subscription.ttl, 0)
            .ok_or(PushNotificationStorageError::InvalidTtlError)?;
        let rounded_ttl = Self::round_to_minute(timestamp);

        self.dynamodb_client
            .put_item()
            .table_name(&self.table_name)
            .item("hmac", AttributeValue::S(subscription.hmac.to_string()))
            .item("topic", AttributeValue::S(subscription.topic.to_string()))
            .item("ttl", AttributeValue::N(rounded_ttl.to_string()))
            .item(
                "encrypted_braze_id",
                AttributeValue::S(subscription.encrypted_braze_id.to_string()),
            )
            .send()
            .await?;

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
    /// Returns `PushNotificationStorageError` if the DynamoDB operation fails
    pub async fn delete_by_hmac(&self, hmac: &str) -> PushNotificationStorageResult<()> {
        self.dynamodb_client
            .delete_item()
            .table_name(&self.table_name)
            .key("hmac", AttributeValue::S(hmac.to_string()))
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
    /// Returns `PushNotificationStorageError` if the DynamoDB operation fails
    pub async fn get_all_by_topic(
        &self,
        topic: &str,
    ) -> PushNotificationStorageResult<Vec<PushSubscription>> {
        let response = self
            .dynamodb_client
            .query()
            .table_name(&self.table_name)
            .index_name(&self.gsi_name)
            .key_condition_expression("topic = :topic")
            .expression_attribute_values(":topic", AttributeValue::S(topic.to_string()))
            .select(Select::AllAttributes)
            .send()
            .await?;

        response
            .items()
            .iter()
            .map(Self::parse_subscription_from_item)
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
    /// Returns `PushNotificationStorageError` if the DynamoDB operation fails
    pub async fn exists_by_hmac(&self, hmac: &str) -> PushNotificationStorageResult<bool> {
        let response = self
            .dynamodb_client
            .get_item()
            .table_name(&self.table_name)
            .key("hmac", AttributeValue::S(hmac.to_string()))
            .projection_expression("hmac")
            .send()
            .await?;

        Ok(response.item().is_some())
    }

    /// Parses a push subscription from DynamoDB item attributes
    fn parse_subscription_from_item(
        item: &std::collections::HashMap<String, AttributeValue>,
    ) -> PushNotificationStorageResult<PushSubscription> {
        let hmac = item
            .get("hmac")
            .and_then(|v| v.as_s().ok())
            .ok_or_else(|| {
                PushNotificationStorageError::ParseSubscriptionError(
                    "Missing hmac field".to_string(),
                )
            })?
            .to_string();

        let topic = item
            .get("topic")
            .and_then(|v| v.as_s().ok())
            .ok_or_else(|| {
                PushNotificationStorageError::ParseSubscriptionError(
                    "Missing topic field".to_string(),
                )
            })?
            .to_string();

        let ttl = item
            .get("ttl")
            .and_then(|v| v.as_n().ok())
            .and_then(|n| n.parse::<i64>().ok())
            .ok_or_else(|| {
                PushNotificationStorageError::ParseSubscriptionError(
                    "Invalid ttl field".to_string(),
                )
            })?;

        let encrypted_braze_id = item
            .get("encrypted_braze_id")
            .and_then(|v| v.as_s().ok())
            .ok_or_else(|| {
                PushNotificationStorageError::ParseSubscriptionError(
                    "Missing encrypted_braze_id field".to_string(),
                )
            })?
            .to_string();

        Ok(PushSubscription {
            hmac,
            topic,
            ttl,
            encrypted_braze_id,
        })
    }
}
