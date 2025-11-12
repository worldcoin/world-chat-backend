//! Group invites storage module for `DynamoDB` operations

mod error;

use aws_sdk_dynamodb::types::AttributeValue;
use aws_sdk_dynamodb::Client as DynamoDbClient;
pub use error::{GroupInviteStorageError, GroupInviteStorageResult};
use serde::{Deserialize, Serialize};
use serde_dynamo::{from_items, to_item};
use std::sync::Arc;
use strum::Display;

/// `DynamoDB` table for group invites
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupInvite {
    /// Primary key - unique invite ID (UUID v4)
    pub id: String,
    /// XMTP topic
    pub topic: String,
    /// Group Name used in invite link
    pub group_name: String,
    /// Encrypted push of the inviter used to send silent push notification
    pub creator_encrypted_push_id: String,
    /// Timestamp of invite creation
    pub created_at: i64,
    /// Optional max uses of the invite
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_uses: Option<i64>,
    /// Optional timestamp expiration of the invite
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
}

/// Request to create a new group invite
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupInviteCreateRequest {
    /// XMTP topic
    pub topic: String,
    /// Group Name used in invite link
    pub group_name: String,
    /// Encrypted push of the inviter used to send silent push notification
    pub creator_encrypted_push_id: String,
    /// Optional `max_uses` of the invite
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_uses: Option<i64>,
    /// Optional timestamp `expires_at` of the invite
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
}

/// `DynamoDB` attribute names for the group invite table
#[derive(Debug, Display)]
#[strum(serialize_all = "snake_case")]
pub enum GroupInviteAttribute {
    /// Primary key - unique invite ID
    Id,
    /// XMTP topic (used for GSI)
    Topic,
    /// Group name for invite link
    GroupName,
    /// Encrypted push ID of the creator
    CreatorEncryptedPushId,
    /// Creation timestamp
    CreatedAt,
    /// Maximum number of uses for the invite
    MaxUses,
    /// Expiration timestamp
    ExpiresAt,
}

/// Storage client for group invite operations
pub struct GroupInviteStorage {
    dynamodb_client: Arc<DynamoDbClient>,
    table_name: String,
    topic_index_name: String,
}

impl GroupInviteStorage {
    /// Creates a new storage instance
    ///
    /// # Arguments
    ///
    /// * `dynamodb_client` - Pre-configured `DynamoDB` client
    /// * `table_name` - `DynamoDB` table name for group invites
    /// * `topic_index_name` - Name of the GSI for topic queries
    #[must_use]
    pub const fn new(
        dynamodb_client: Arc<DynamoDbClient>,
        table_name: String,
        topic_index_name: String,
    ) -> Self {
        Self {
            dynamodb_client,
            table_name,
            topic_index_name,
        }
    }

    /// Get the most recent invite for a given (topic, `creator_encrypted_push_id`).
    ///
    /// # Errors
    ///
    /// Returns `GroupInviteStorageError` if the `DynamoDB` query operation fails
    pub async fn get_latest_by_topic(
        &self,
        creator_encrypted_push_id: &str,
        topic: &str,
    ) -> GroupInviteStorageResult<Option<GroupInvite>> {
        let response = self
            .dynamodb_client
            .query()
            .table_name(&self.table_name)
            .index_name(&self.topic_index_name)
            .key_condition_expression("#topic = :topic")
            .expression_attribute_names("#topic", GroupInviteAttribute::Topic.to_string())
            .expression_attribute_names(
                "#creator",
                GroupInviteAttribute::CreatorEncryptedPushId.to_string(),
            )
            .expression_attribute_values(":topic", AttributeValue::S(topic.to_string()))
            .expression_attribute_values(
                ":creator",
                AttributeValue::S(creator_encrypted_push_id.to_string()),
            )
            .filter_expression("#creator = :creator")
            .send()
            .await?;

        let items = response.items.unwrap_or_default();
        let mut invites = from_items::<_, GroupInvite>(items)?;
        invites.sort_by_key(|i| std::cmp::Reverse(i.created_at));
        let latest_invite = invites.into_iter().next();

        Ok(latest_invite)
    }

    /// Get a single group invite by ID
    ///
    /// # Errors
    ///
    /// Returns `GroupInviteStorageError` if the `DynamoDB` get operation fails
    pub async fn get_one(&self, id: &str) -> GroupInviteStorageResult<Option<GroupInvite>> {
        let response = self
            .dynamodb_client
            .get_item()
            .table_name(&self.table_name)
            .key(
                GroupInviteAttribute::Id.to_string(),
                AttributeValue::S(id.to_string()),
            )
            .send()
            .await?;

        response
            .item()
            .map(|item| {
                serde_dynamo::from_item(item.clone())
                    .map_err(|e| GroupInviteStorageError::SerializationError(e.to_string()))
            })
            .transpose()
    }

    /// Create a new group invite with generated UUID
    ///
    /// # Errors
    ///
    /// Returns `GroupInviteStorageError` if the `DynamoDB` put operation fails
    pub async fn create(
        &self,
        request: GroupInviteCreateRequest,
    ) -> GroupInviteStorageResult<GroupInvite> {
        // Generate UUID v4 for the invite ID
        let id = uuid::Uuid::new_v4().to_string();

        let invite = GroupInvite {
            id: id.clone(),
            topic: request.topic,
            group_name: request.group_name,
            creator_encrypted_push_id: request.creator_encrypted_push_id,
            max_uses: request.max_uses,
            expires_at: request.expires_at,
            created_at: chrono::Utc::now().timestamp(),
        };

        let item = to_item(&invite)?;

        self.dynamodb_client
            .put_item()
            .table_name(&self.table_name)
            .set_item(Some(item))
            .send()
            .await?;

        Ok(invite)
    }

    /// Delete a group invite by ID
    ///
    /// # Errors
    ///
    /// Returns `GroupInviteStorageError` if the `DynamoDB` delete operation fails
    pub async fn delete(&self, id: &str) -> GroupInviteStorageResult<()> {
        self.dynamodb_client
            .delete_item()
            .table_name(&self.table_name)
            .key(
                GroupInviteAttribute::Id.to_string(),
                AttributeValue::S(id.to_string()),
            )
            .send()
            .await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_group_invite_serialization() {
        let invite = GroupInvite {
            id: "test-id".to_string(),
            topic: "test-topic".to_string(),
            group_name: "Test Group".to_string(),
            creator_encrypted_push_id: "encrypted-push-id".to_string(),
            max_uses: Some(10),
            expires_at: Some(1_234_567_890),
            created_at: chrono::Utc::now().timestamp(),
        };

        let serialized = serde_json::to_string(&invite).unwrap();
        let deserialized: GroupInvite = serde_json::from_str(&serialized).unwrap();

        assert_eq!(invite.id, deserialized.id);
        assert_eq!(invite.topic, deserialized.topic);
        assert_eq!(invite.group_name, deserialized.group_name);
        assert_eq!(
            invite.creator_encrypted_push_id,
            deserialized.creator_encrypted_push_id
        );
        assert_eq!(invite.max_uses, deserialized.max_uses);
        assert_eq!(invite.expires_at, deserialized.expires_at);
    }

    #[test]
    fn test_group_invite_optional_fields() {
        let invite = GroupInvite {
            id: "test-id".to_string(),
            topic: "test-topic".to_string(),
            group_name: "Test Group".to_string(),
            creator_encrypted_push_id: "encrypted-push-id".to_string(),
            max_uses: None,
            expires_at: None,
            created_at: chrono::Utc::now().timestamp(),
        };

        let serialized = serde_json::to_string(&invite).unwrap();
        let json: serde_json::Value = serde_json::from_str(&serialized).unwrap();

        assert!(json.get("max_uses").is_none());
        assert!(json.get("expires_at").is_none());
    }
}
