//! Group join requests storage module for `DynamoDB` operations

mod error;

use aws_sdk_dynamodb::types::{AttributeValue, DeleteRequest, WriteRequest};
use aws_sdk_dynamodb::Client as DynamoDbClient;
pub use error::{GroupJoinRequestStorageError, GroupJoinRequestStorageResult};
use serde::{Deserialize, Serialize};
use serde_dynamo::{from_item, to_item};
use std::collections::HashMap;
use std::sync::Arc;
use strum::Display;

/// Status of a group join request
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum JoinRequestStatus {
    /// Request is pending
    Pending,
    /// Notification has been sent
    NotificationSent,
    /// Request has been accepted
    Accepted,
    /// Request has been rejected
    Rejected,
}

/// `DynamoDB` table for group join requests
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupJoinRequest {
    /// Primary key - unique join request ID (UUID v4)
    pub id: String,

    /// Group Invite ID linked to `GroupInvites` table
    pub group_invite_id: String,

    /// Encrypted inbox id of the invitee with enclave's public key
    pub encrypted_inbox_id: String,

    /// Status of the join request
    pub status: JoinRequestStatus,

    /// Optional timestamp when notification was sent
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notification_sent_at: Option<i64>,
}

/// Request to create a new group join request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupJoinRequestCreateRequest {
    /// Group Invite ID linked to `GroupInvites` table
    pub group_invite_id: String,

    /// Encrypted inbox id of the invitee with enclave's public key
    pub encrypted_inbox_id: String,

    /// Status of the join request
    pub status: JoinRequestStatus,

    /// Optional timestamp when notification was sent
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notification_sent_at: Option<i64>,
}

/// `DynamoDB` attribute names for the group join request table
#[derive(Debug, Display)]
#[strum(serialize_all = "snake_case")]
pub enum GroupJoinRequestAttribute {
    /// Primary key - unique join request ID
    Id,
    /// Group invite ID (used for GSI)
    GroupInviteId,
    /// Encrypted inbox ID
    EncryptedInboxId,
    /// Status of the request
    Status,
    /// Notification sent timestamp
    NotificationSentAt,
}

/// Storage client for group join request operations
pub struct GroupJoinRequestStorage {
    dynamodb_client: Arc<DynamoDbClient>,
    table_name: String,
    group_invite_index_name: String,
}

impl GroupJoinRequestStorage {
    /// Creates a new storage instance
    ///
    /// # Arguments
    ///
    /// * `dynamodb_client` - Pre-configured `DynamoDB` client
    /// * `table_name` - `DynamoDB` table name for group join requests
    /// * `group_invite_index_name` - Name of the GSI for group invite queries
    #[must_use]
    pub const fn new(
        dynamodb_client: Arc<DynamoDbClient>,
        table_name: String,
        group_invite_index_name: String,
    ) -> Self {
        Self {
            dynamodb_client,
            table_name,
            group_invite_index_name,
        }
    }

    /// Get a single group join request by ID
    ///
    /// # Errors
    ///
    /// Returns `GroupJoinRequestStorageError` if the `DynamoDB` get operation fails
    pub async fn get_one(
        &self,
        id: &str,
    ) -> GroupJoinRequestStorageResult<Option<GroupJoinRequest>> {
        let response = self
            .dynamodb_client
            .get_item()
            .table_name(&self.table_name)
            .key(
                GroupJoinRequestAttribute::Id.to_string(),
                AttributeValue::S(id.to_string()),
            )
            .send()
            .await?;

        response
            .item()
            .map(|item| {
                serde_dynamo::from_item(item.clone())
                    .map_err(|e| GroupJoinRequestStorageError::SerializationError(e.to_string()))
            })
            .transpose()
    }

    /// Get all group join requests for a given group invite ID
    ///
    /// # Errors
    ///
    /// Returns `GroupJoinRequestStorageError` if the `DynamoDB` query operation fails
    pub async fn get_by_group_invite_id(
        &self,
        group_invite_id: &str,
    ) -> GroupJoinRequestStorageResult<Vec<GroupJoinRequest>> {
        let response = self
            .dynamodb_client
            .query()
            .table_name(&self.table_name)
            .index_name(&self.group_invite_index_name)
            .key_condition_expression("#group_invite_id = :group_invite_id")
            .expression_attribute_names(
                "#group_invite_id",
                GroupJoinRequestAttribute::GroupInviteId.to_string(),
            )
            .expression_attribute_values(
                ":group_invite_id",
                AttributeValue::S(group_invite_id.to_string()),
            )
            .send()
            .await?;

        response
            .items()
            .iter()
            .map(|item| {
                from_item(item.clone())
                    .map_err(|e| GroupJoinRequestStorageError::SerializationError(e.to_string()))
            })
            .collect()
    }

    /// Delete all group join requests linked to a given group invite ID
    ///
    /// # Errors
    ///
    /// Returns `GroupJoinRequestStorageError` if the `DynamoDB` operations fail
    pub async fn delete_by_group_invite_id(
        &self,
        group_invite_id: &str,
    ) -> GroupJoinRequestStorageResult<()> {
        // Query all join requests for this group invite
        let group_join_requests = self.get_by_group_invite_id(group_invite_id).await?;

        if group_join_requests.is_empty() {
            return Ok(());
        }

        // Extract IDs for batch deletion
        let ids: Vec<String> = group_join_requests.into_iter().map(|jr| jr.id).collect();

        // Use batch delete
        self.batch_delete(&ids).await
    }

    /// Batch delete multiple join requests by their IDs
    ///
    /// # Errors
    ///
    /// Returns `GroupJoinRequestStorageError` if the `DynamoDB` batch write operation fails
    pub async fn batch_delete(&self, ids: &[String]) -> GroupJoinRequestStorageResult<()> {
        // DynamoDB batch delete has a limit of 25 items per request
        for chunk in ids.chunks(25) {
            let write_requests = chunk
                .iter()
                .map(|id| Self::build_delete_request(id.clone()))
                .collect::<Result<Vec<_>, _>>()?;

            self.dynamodb_client
                .batch_write_item()
                .request_items(&self.table_name, write_requests)
                .send()
                .await?;
        }

        Ok(())
    }

    /// Builds a delete request for a join request
    ///
    /// # Returns
    ///
    /// A delete request for the join request
    fn build_delete_request(id: String) -> GroupJoinRequestStorageResult<WriteRequest> {
        let key = HashMap::from([(
            GroupJoinRequestAttribute::Id.to_string(),
            AttributeValue::S(id),
        )]);

        Ok(WriteRequest::builder()
            .delete_request(
                DeleteRequest::builder()
                    .set_key(Some(key))
                    .build()
                    .map_err(|e| {
                        GroupJoinRequestStorageError::SerializationError(format!(
                            "Failed to build delete request: {e:?}",
                        ))
                    })?,
            )
            .build())
    }

    /// Create a new group join request with generated UUID
    ///
    /// # Errors
    ///
    /// Returns `GroupJoinRequestStorageError` if the `DynamoDB` put operation fails
    pub async fn create(
        &self,
        request: GroupJoinRequestCreateRequest,
    ) -> GroupJoinRequestStorageResult<GroupJoinRequest> {
        // Generate UUID v4 for the join request ID
        let id = uuid::Uuid::new_v4().to_string();

        let join_request = GroupJoinRequest {
            id: id.clone(),
            group_invite_id: request.group_invite_id,
            encrypted_inbox_id: request.encrypted_inbox_id,
            status: request.status,
            notification_sent_at: request.notification_sent_at,
        };

        let item = to_item(&join_request)?;

        self.dynamodb_client
            .put_item()
            .table_name(&self.table_name)
            .set_item(Some(item))
            .send()
            .await?;

        Ok(join_request)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_group_join_request_serialization() {
        let request = GroupJoinRequest {
            id: "test-id".to_string(),
            group_invite_id: "invite-123".to_string(),
            encrypted_inbox_id: "encrypted-inbox".to_string(),
            status: JoinRequestStatus::Pending,
            notification_sent_at: Some(1_234_567_890),
        };

        let serialized = serde_json::to_string(&request).unwrap();
        let deserialized: GroupJoinRequest = serde_json::from_str(&serialized).unwrap();

        assert_eq!(request.id, deserialized.id);
        assert_eq!(request.group_invite_id, deserialized.group_invite_id);
        assert_eq!(request.encrypted_inbox_id, deserialized.encrypted_inbox_id);
        assert_eq!(request.status, deserialized.status);
        assert_eq!(
            request.notification_sent_at,
            deserialized.notification_sent_at
        );
    }

    #[test]
    fn test_join_request_status_serialization() {
        // Test that status serializes to PascalCase
        let pending = JoinRequestStatus::Pending;
        let serialized = serde_json::to_string(&pending).unwrap();
        assert_eq!(serialized, "\"Pending\"");

        let notification_sent = JoinRequestStatus::NotificationSent;
        let serialized = serde_json::to_string(&notification_sent).unwrap();
        assert_eq!(serialized, "\"NotificationSent\"");
    }

    #[test]
    fn test_group_join_request_optional_fields() {
        let request = GroupJoinRequest {
            id: "test-id".to_string(),
            group_invite_id: "invite-123".to_string(),
            encrypted_inbox_id: "encrypted-inbox".to_string(),
            status: JoinRequestStatus::Pending,
            notification_sent_at: None,
        };

        let serialized = serde_json::to_string(&request).unwrap();
        let json: serde_json::Value = serde_json::from_str(&serialized).unwrap();

        assert!(json.get("notification_sent_at").is_none());
    }
}
