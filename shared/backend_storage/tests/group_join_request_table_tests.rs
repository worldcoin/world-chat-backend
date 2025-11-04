use std::sync::Arc;
use std::time::Duration;

use aws_config::{BehaviorVersion, Region};
use aws_credential_types::Credentials;
use aws_sdk_dynamodb::types::{
    AttributeDefinition, BillingMode, GlobalSecondaryIndex, KeySchemaElement, KeyType, Projection,
    ProjectionType, ScalarAttributeType,
};
use aws_sdk_dynamodb::Client as DynamoDbClient;
use backend_storage::group_join_request::{
    GroupJoinRequestAttribute, GroupJoinRequestCreateRequest, GroupJoinRequestStorage,
    JoinRequestStatus,
};
use tokio::time::sleep;
use uuid::Uuid;

/// Test configuration for LocalStack
const LOCALSTACK_ENDPOINT: &str = "http://localhost:4566";
const TEST_REGION: &str = "us-east-1";
const TEST_GROUP_INVITE_INDEX_NAME: &str = "group-invite-index";

/// Test context that automatically cleans up the table on drop
struct TestContext {
    storage: GroupJoinRequestStorage,
    table_name: String,
    dynamodb_client: Arc<DynamoDbClient>,
}

impl Drop for TestContext {
    fn drop(&mut self) {
        // Clean up the table
        let client = self.dynamodb_client.clone();
        let table = self.table_name.clone();

        // Use tokio runtime to delete table
        let handle = tokio::runtime::Handle::try_current();
        if let Ok(handle) = handle {
            handle.spawn(async move {
                let _ = client.delete_table().table_name(&table).send().await;
            });
        }
    }
}

/// Creates a test setup with a unique table
async fn setup_test() -> TestContext {
    // Create unique table name
    let table_name = format!("test-group-join-requests-{}", Uuid::new_v4());

    // Configure AWS SDK for LocalStack
    let credentials = Credentials::from_keys(
        "test", // AWS_ACCESS_KEY_ID
        "test", // AWS_SECRET_ACCESS_KEY
        None,   // no session token
    );
    let config = aws_config::defaults(BehaviorVersion::latest())
        .endpoint_url(LOCALSTACK_ENDPOINT)
        .region(Region::new(TEST_REGION))
        .credentials_provider(credentials)
        .load()
        .await;

    let dynamodb_client = Arc::new(DynamoDbClient::new(&config));

    // Create the table with GSI
    dynamodb_client
        .create_table()
        .table_name(&table_name)
        .billing_mode(BillingMode::PayPerRequest)
        // Primary key
        .key_schema(
            KeySchemaElement::builder()
                .attribute_name(GroupJoinRequestAttribute::Id.to_string())
                .key_type(KeyType::Hash)
                .build()
                .expect("Failed to build key schema"),
        )
        // Attribute definitions
        .attribute_definitions(
            AttributeDefinition::builder()
                .attribute_name(GroupJoinRequestAttribute::Id.to_string())
                .attribute_type(ScalarAttributeType::S)
                .build()
                .expect("Failed to build attribute definition"),
        )
        .attribute_definitions(
            AttributeDefinition::builder()
                .attribute_name(GroupJoinRequestAttribute::GroupInviteId.to_string())
                .attribute_type(ScalarAttributeType::S)
                .build()
                .expect("Failed to build attribute definition"),
        )
        // Global Secondary Index for group invite queries
        .global_secondary_indexes(
            GlobalSecondaryIndex::builder()
                .index_name(TEST_GROUP_INVITE_INDEX_NAME)
                .key_schema(
                    KeySchemaElement::builder()
                        .attribute_name(GroupJoinRequestAttribute::GroupInviteId.to_string())
                        .key_type(KeyType::Hash)
                        .build()
                        .expect("Failed to build GSI key schema"),
                )
                .projection(
                    Projection::builder()
                        .projection_type(ProjectionType::All)
                        .build(),
                )
                .build()
                .expect("Failed to build GSI"),
        )
        .send()
        .await
        .expect("Failed to create test table");

    // Wait for table to be ready
    sleep(Duration::from_millis(100)).await;

    let storage = GroupJoinRequestStorage::new(
        dynamodb_client.clone(),
        table_name.clone(),
        TEST_GROUP_INVITE_INDEX_NAME.to_string(),
    );

    TestContext {
        storage,
        table_name,
        dynamodb_client,
    }
}

/// Creates a test join request with all fields populated
fn create_test_join_request(group_invite_id: &str) -> GroupJoinRequestCreateRequest {
    GroupJoinRequestCreateRequest {
        group_invite_id: group_invite_id.to_string(),
        encrypted_inbox_id: format!("encrypted_inbox_{}", Uuid::new_v4()),
        status: JoinRequestStatus::Pending,
        notification_sent_at: Some(1_234_567_890),
    }
}

/// Creates a test join request with minimal fields
fn create_test_join_request_minimal(group_invite_id: &str) -> GroupJoinRequestCreateRequest {
    GroupJoinRequestCreateRequest {
        group_invite_id: group_invite_id.to_string(),
        encrypted_inbox_id: format!("encrypted_inbox_{}", Uuid::new_v4()),
        status: JoinRequestStatus::Pending,
        notification_sent_at: None,
    }
}

#[tokio::test]
async fn test_create_join_request() {
    let ctx = setup_test().await;
    let group_invite_id = format!("invite-{}", Uuid::new_v4());
    let request = create_test_join_request(&group_invite_id);

    // Create the join request
    let join_request = ctx
        .storage
        .create(request.clone())
        .await
        .expect("Failed to create join request");

    // Verify the created join request has all expected fields
    assert!(!join_request.id.is_empty());
    assert_eq!(join_request.group_invite_id, request.group_invite_id);
    assert_eq!(join_request.encrypted_inbox_id, request.encrypted_inbox_id);
    assert_eq!(join_request.status, request.status);
    assert_eq!(
        join_request.notification_sent_at,
        request.notification_sent_at
    );
}

#[tokio::test]
async fn test_create_join_request_without_optional_fields() {
    let ctx = setup_test().await;
    let group_invite_id = format!("invite-{}", Uuid::new_v4());
    let request = create_test_join_request_minimal(&group_invite_id);

    // Create the join request
    let join_request = ctx
        .storage
        .create(request.clone())
        .await
        .expect("Failed to create join request");

    // Verify the created join request
    assert!(!join_request.id.is_empty());
    assert_eq!(join_request.group_invite_id, request.group_invite_id);
    assert_eq!(join_request.encrypted_inbox_id, request.encrypted_inbox_id);
    assert_eq!(join_request.status, JoinRequestStatus::Pending);
    assert_eq!(join_request.notification_sent_at, None);
}

#[tokio::test]
async fn test_get_by_id_existing() {
    let ctx = setup_test().await;
    let group_invite_id = format!("invite-{}", Uuid::new_v4());
    let request = create_test_join_request(&group_invite_id);

    // Create the join request
    let created = ctx
        .storage
        .create(request.clone())
        .await
        .expect("Failed to create join request");

    // Get the join request by ID
    let retrieved = ctx
        .storage
        .get_one(&created.id)
        .await
        .expect("Failed to get join request");

    // Verify we got the join request
    assert!(retrieved.is_some());
    let retrieved = retrieved.unwrap();

    assert_eq!(retrieved.id, created.id);
    assert_eq!(retrieved.group_invite_id, created.group_invite_id);
    assert_eq!(retrieved.encrypted_inbox_id, created.encrypted_inbox_id);
    assert_eq!(retrieved.status, created.status);
    assert_eq!(retrieved.notification_sent_at, created.notification_sent_at);
}

#[tokio::test]
async fn test_get_by_id_non_existing() {
    let ctx = setup_test().await;

    // Try to get a non-existing join request
    let non_existing_id = Uuid::new_v4().to_string();
    let result = ctx
        .storage
        .get_one(&non_existing_id)
        .await
        .expect("Failed to query non-existing join request");

    // Should return None
    assert!(result.is_none());
}

#[tokio::test]
async fn test_get_by_group_invite_id() {
    let ctx = setup_test().await;
    let group_invite_id = format!("invite-{}", Uuid::new_v4());

    // Create multiple join requests for the same group invite
    let mut created_requests = Vec::new();
    for i in 0..3 {
        let mut request = create_test_join_request(&group_invite_id);
        request.status = match i {
            0 => JoinRequestStatus::Pending,
            1 => JoinRequestStatus::NotificationSent,
            _ => JoinRequestStatus::Accepted,
        };
        let created = ctx
            .storage
            .create(request)
            .await
            .expect("Failed to create join request");
        created_requests.push(created);
    }

    // Wait for GSI to be updated
    sleep(Duration::from_millis(200)).await;

    // Query all join requests for this group invite
    let results = ctx
        .storage
        .get_by_group_invite_id(&group_invite_id)
        .await
        .expect("Failed to get by group invite id");

    // Verify we got all 3 join requests
    assert_eq!(results.len(), 3);

    // Verify all returned requests have the correct group_invite_id
    for result in &results {
        assert_eq!(result.group_invite_id, group_invite_id);
    }
}

#[tokio::test]
async fn test_delete_by_group_invite_id_single() {
    let ctx = setup_test().await;
    let group_invite_id = format!("invite-{}", Uuid::new_v4());
    let request = create_test_join_request(&group_invite_id);

    // Create a join request
    let created = ctx
        .storage
        .create(request)
        .await
        .expect("Failed to create join request");

    // Wait for GSI to be updated
    sleep(Duration::from_millis(100)).await;

    // Delete all join requests for this group invite
    ctx.storage
        .delete_by_group_invite_id(&group_invite_id)
        .await
        .expect("Failed to delete by group invite id");

    // Verify the join request is gone
    let result = ctx
        .storage
        .get_one(&created.id)
        .await
        .expect("Failed to query deleted join request");

    assert!(result.is_none());
}

#[tokio::test]
async fn test_delete_by_group_invite_id_multiple() {
    let ctx = setup_test().await;
    let group_invite_id = format!("invite-{}", Uuid::new_v4());

    // Create multiple join requests for the same group invite
    let mut created_ids = Vec::new();
    for _ in 0..3 {
        let request = create_test_join_request(&group_invite_id);
        let created = ctx
            .storage
            .create(request)
            .await
            .expect("Failed to create join request");
        created_ids.push(created.id);
    }

    // Wait for GSI to be updated
    sleep(Duration::from_millis(200)).await;

    // Delete all join requests for this group invite
    ctx.storage
        .delete_by_group_invite_id(&group_invite_id)
        .await
        .expect("Failed to delete by group invite id");

    // Verify all join requests are gone
    for id in created_ids {
        let result = ctx
            .storage
            .get_one(&id)
            .await
            .expect("Failed to query deleted join request");
        assert!(result.is_none());
    }
}

#[tokio::test]
async fn test_delete_by_group_invite_id_non_existing() {
    let ctx = setup_test().await;
    let non_existing_group_invite_id = format!("invite-{}", Uuid::new_v4());

    // Should succeed even if no join requests exist for this group invite
    ctx.storage
        .delete_by_group_invite_id(&non_existing_group_invite_id)
        .await
        .expect("Failed to delete by non-existing group invite id");
}

#[tokio::test]
async fn test_different_statuses() {
    let ctx = setup_test().await;
    let group_invite_id = format!("invite-{}", Uuid::new_v4());

    // Test different statuses
    let statuses = vec![
        JoinRequestStatus::Pending,
        JoinRequestStatus::NotificationSent,
        JoinRequestStatus::Accepted,
        JoinRequestStatus::Rejected,
    ];

    for status in statuses {
        let mut request = create_test_join_request(&group_invite_id);
        request.status = status.clone();

        let created = ctx
            .storage
            .create(request)
            .await
            .expect("Failed to create join request");

        assert_eq!(created.status, status);

        // Verify it can be retrieved
        let retrieved = ctx
            .storage
            .get_one(&created.id)
            .await
            .expect("Failed to get join request")
            .expect("Join request not found");

        assert_eq!(retrieved.status, status);
    }
}

#[tokio::test]
async fn test_batch_delete_over_25_items() {
    let ctx = setup_test().await;
    let group_invite_id = format!("invite-{}", Uuid::new_v4());

    // Create 30 join requests (more than DynamoDB's batch limit of 25)
    let mut created_ids = Vec::new();
    for _ in 0..30 {
        let request = create_test_join_request(&group_invite_id);
        let created = ctx
            .storage
            .create(request)
            .await
            .expect("Failed to create join request");
        created_ids.push(created.id);
    }

    // Wait for GSI to be updated
    sleep(Duration::from_millis(300)).await;

    // Delete all join requests (should handle batching internally)
    ctx.storage
        .delete_by_group_invite_id(&group_invite_id)
        .await
        .expect("Failed to delete by group invite id");

    // Verify all 30 join requests are gone
    for id in created_ids {
        let result = ctx
            .storage
            .get_one(&id)
            .await
            .expect("Failed to query deleted join request");
        assert!(result.is_none());
    }
}
