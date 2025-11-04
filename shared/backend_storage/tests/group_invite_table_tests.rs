use std::sync::Arc;
use std::time::Duration;

use aws_config::{BehaviorVersion, Region};
use aws_credential_types::Credentials;
use aws_sdk_dynamodb::types::{
    AttributeDefinition, BillingMode, GlobalSecondaryIndex, KeySchemaElement, KeyType, Projection,
    ProjectionType, ScalarAttributeType,
};
use aws_sdk_dynamodb::Client as DynamoDbClient;
use backend_storage::group_invite::{
    GroupInviteAttribute, GroupInviteCreateRequest, GroupInviteStorage,
};
use tokio::time::sleep;
use uuid::Uuid;

/// Test configuration for LocalStack
const LOCALSTACK_ENDPOINT: &str = "http://localhost:4566";
const TEST_REGION: &str = "us-east-1";
const TEST_TOPIC_INDEX_NAME: &str = "topic-index";

/// Test context that automatically cleans up the table on drop
struct TestContext {
    storage: GroupInviteStorage,
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
    let table_name = format!("test-group-invites-{}", Uuid::new_v4());

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
                .attribute_name(GroupInviteAttribute::Id.to_string())
                .key_type(KeyType::Hash)
                .build()
                .expect("Failed to build key schema"),
        )
        // Attribute definitions
        .attribute_definitions(
            AttributeDefinition::builder()
                .attribute_name(GroupInviteAttribute::Id.to_string())
                .attribute_type(ScalarAttributeType::S)
                .build()
                .expect("Failed to build attribute definition"),
        )
        .attribute_definitions(
            AttributeDefinition::builder()
                .attribute_name(GroupInviteAttribute::Topic.to_string())
                .attribute_type(ScalarAttributeType::S)
                .build()
                .expect("Failed to build attribute definition"),
        )
        // Global Secondary Index for topic queries
        .global_secondary_indexes(
            GlobalSecondaryIndex::builder()
                .index_name(TEST_TOPIC_INDEX_NAME)
                .key_schema(
                    KeySchemaElement::builder()
                        .attribute_name(GroupInviteAttribute::Topic.to_string())
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

    let storage = GroupInviteStorage::new(
        dynamodb_client.clone(),
        table_name.clone(),
        TEST_TOPIC_INDEX_NAME.to_string(),
    );

    TestContext {
        storage,
        table_name,
        dynamodb_client,
    }
}

/// Creates a test invite request with all optional fields populated
fn create_test_invite_request(topic: &str) -> GroupInviteCreateRequest {
    GroupInviteCreateRequest {
        topic: topic.to_string(),
        group_name: format!("Test Group for {}", topic),
        creator_encrypted_push_id: format!("encrypted_push_{}", Uuid::new_v4()),
        max_uses: Some(10),
        expires_at: Some(1234567890),
    }
}

/// Creates a test invite request with minimal fields (no optional fields)
fn create_test_invite_request_minimal(topic: &str) -> GroupInviteCreateRequest {
    GroupInviteCreateRequest {
        topic: topic.to_string(),
        group_name: format!("Test Group for {}", topic),
        creator_encrypted_push_id: format!("encrypted_push_{}", Uuid::new_v4()),
        max_uses: None,
        expires_at: None,
    }
}

#[tokio::test]
async fn test_create_group_invite() {
    let ctx = setup_test().await;
    let topic = format!("topic-{}", Uuid::new_v4());
    let request = create_test_invite_request(&topic);

    // Create the invite
    let invite = ctx.storage.create(request.clone()).await
        .expect("Failed to create group invite");

    // Verify the created invite has all expected fields
    assert!(!invite.id.is_empty());
    assert_eq!(invite.topic, request.topic);
    assert_eq!(invite.group_name, request.group_name);
    assert_eq!(
        invite.creator_encrypted_push_id,
        request.creator_encrypted_push_id
    );
    assert_eq!(invite.max_uses, request.max_uses);
    assert_eq!(invite.expires_at, request.expires_at);
}

#[tokio::test]
async fn test_create_group_invite_without_optional_fields() {
    let ctx = setup_test().await;
    let topic = format!("topic-{}", Uuid::new_v4());
    let request = create_test_invite_request_minimal(&topic);

    // Create the invite
    let invite = ctx.storage.create(request.clone()).await
        .expect("Failed to create group invite");

    // Verify the created invite
    assert!(!invite.id.is_empty());
    assert_eq!(invite.topic, request.topic);
    assert_eq!(invite.group_name, request.group_name);
    assert_eq!(
        invite.creator_encrypted_push_id,
        request.creator_encrypted_push_id
    );
    assert_eq!(invite.max_uses, None);
    assert_eq!(invite.expires_at, None);
}

#[tokio::test]
async fn test_get_one_existing_invite() {
    let ctx = setup_test().await;
    let topic = format!("topic-{}", Uuid::new_v4());
    let request = create_test_invite_request(&topic);

    // Create the invite
    let created_invite = ctx.storage.create(request.clone()).await
        .expect("Failed to create group invite");

    // Get the invite by ID
    let retrieved_invite = ctx.storage.get_one(&created_invite.id).await
        .expect("Failed to get group invite");

    // Verify we got the invite
    assert!(retrieved_invite.is_some());
    let retrieved_invite = retrieved_invite.unwrap();

    assert_eq!(retrieved_invite.id, created_invite.id);
    assert_eq!(retrieved_invite.topic, created_invite.topic);
    assert_eq!(retrieved_invite.group_name, created_invite.group_name);
    assert_eq!(
        retrieved_invite.creator_encrypted_push_id,
        created_invite.creator_encrypted_push_id
    );
    assert_eq!(retrieved_invite.max_uses, created_invite.max_uses);
    assert_eq!(retrieved_invite.expires_at, created_invite.expires_at);
}

#[tokio::test]
async fn test_get_one_non_existing_invite() {
    let ctx = setup_test().await;

    // Try to get a non-existing invite
    let non_existing_id = Uuid::new_v4().to_string();
    let result = ctx.storage.get_one(&non_existing_id).await
        .expect("Failed to query non-existing invite");

    // Should return None
    assert!(result.is_none());
}

#[tokio::test]
async fn test_get_by_topic_single_invite() {
    let ctx = setup_test().await;
    let topic = format!("topic-{}", Uuid::new_v4());
    let request = create_test_invite_request(&topic);

    // Create the invite
    let created_invite = ctx.storage.create(request).await
        .expect("Failed to create group invite");

    // Wait a bit for GSI to be updated
    sleep(Duration::from_millis(100)).await;

    // Get invites by topic
    let invites = ctx.storage.get_by_topic(&topic).await
        .expect("Failed to get invites by topic");

    // Should have exactly one invite
    assert_eq!(invites.len(), 1);
    assert_eq!(invites[0].id, created_invite.id);
    assert_eq!(invites[0].topic, topic);
}

#[tokio::test]
async fn test_get_by_topic_multiple_invites() {
    let ctx = setup_test().await;
    let topic = format!("topic-{}", Uuid::new_v4());

    // Create multiple invites for the same topic
    let mut created_ids = Vec::new();
    for i in 0..3 {
        let mut request = create_test_invite_request(&topic);
        request.group_name = format!("Group {}", i);
        let invite = ctx.storage.create(request).await
            .expect("Failed to create group invite");
        created_ids.push(invite.id);
    }

    // Wait a bit for GSI to be updated
    sleep(Duration::from_millis(200)).await;

    // Get invites by topic
    let invites = ctx.storage.get_by_topic(&topic).await
        .expect("Failed to get invites by topic");

    // Should have all three invites
    assert_eq!(invites.len(), 3);

    // Verify all created invites are present
    let retrieved_ids: Vec<String> = invites.iter().map(|i| i.id.clone()).collect();
    for id in created_ids {
        assert!(retrieved_ids.contains(&id));
    }
}

#[tokio::test]
async fn test_get_by_topic_no_invites() {
    let ctx = setup_test().await;
    let topic = format!("topic-{}", Uuid::new_v4());

    // Get invites for a topic with no invites
    let invites = ctx.storage.get_by_topic(&topic).await
        .expect("Failed to get invites by topic");

    // Should return empty vector
    assert_eq!(invites.len(), 0);
}

#[tokio::test]
async fn test_delete_existing_invite() {
    let ctx = setup_test().await;
    let topic = format!("topic-{}", Uuid::new_v4());
    let request = create_test_invite_request(&topic);

    // Create the invite
    let created_invite = ctx.storage.create(request).await
        .expect("Failed to create group invite");

    // Delete the invite
    ctx.storage.delete(&created_invite.id).await
        .expect("Failed to delete group invite");

    // Try to get the deleted invite
    let result = ctx.storage.get_one(&created_invite.id).await
        .expect("Failed to query deleted invite");

    // Should be gone
    assert!(result.is_none());
}

#[tokio::test]
async fn test_delete_non_existing_invite() {
    let ctx = setup_test().await;

    // Try to delete a non-existing invite
    let non_existing_id = Uuid::new_v4().to_string();

    // Delete should succeed even if item doesn't exist (DynamoDB behavior)
    ctx.storage.delete(&non_existing_id).await
        .expect("Failed to delete non-existing invite");
}

#[tokio::test]
async fn test_multiple_topics_isolation() {
    let ctx = setup_test().await;
    let topic1 = format!("topic-{}", Uuid::new_v4());
    let topic2 = format!("topic-{}", Uuid::new_v4());

    // Create invites for different topics
    let request1 = create_test_invite_request(&topic1);
    let request2 = create_test_invite_request(&topic2);

    let invite1 = ctx.storage.create(request1).await
        .expect("Failed to create invite for topic1");
    let invite2 = ctx.storage.create(request2).await
        .expect("Failed to create invite for topic2");

    // Wait for GSI updates
    sleep(Duration::from_millis(100)).await;

    // Get invites for topic1
    let invites_topic1 = ctx.storage.get_by_topic(&topic1).await
        .expect("Failed to get invites for topic1");
    assert_eq!(invites_topic1.len(), 1);
    assert_eq!(invites_topic1[0].id, invite1.id);

    // Get invites for topic2
    let invites_topic2 = ctx.storage.get_by_topic(&topic2).await
        .expect("Failed to get invites for topic2");
    assert_eq!(invites_topic2.len(), 1);
    assert_eq!(invites_topic2[0].id, invite2.id);
}