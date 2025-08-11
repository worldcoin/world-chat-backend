use std::sync::Arc;
use std::time::Duration;

use aws_config::{BehaviorVersion, Region};
use aws_credential_types::Credentials;
use aws_sdk_dynamodb::types::{
    AttributeDefinition, KeySchemaElement, KeyType, ScalarAttributeType,
};
use aws_sdk_dynamodb::Client as DynamoDbClient;
use backend_storage::auth_proof::{
    AuthProof, AuthProofAttribute, AuthProofStorage, AuthProofStorageError,
};
use chrono::Utc;
use uuid::Uuid;

/// Test configuration for LocalStack
const LOCALSTACK_ENDPOINT: &str = "http://localhost:4566";
const TEST_REGION: &str = "us-east-1";

/// Test context that automatically cleans up the table on drop
struct TestContext {
    storage: AuthProofStorage,
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
    let table_name = format!("test-auth-proofs-{}", Uuid::new_v4());

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

    // Create a table to avoid race conditions among tests
    dynamodb_client
        .create_table()
        .table_name(&table_name)
        .attribute_definitions(
            AttributeDefinition::builder()
                .attribute_name(AuthProofAttribute::Nullifier.to_string())
                .attribute_type(ScalarAttributeType::S)
                .build()
                .unwrap(),
        )
        .key_schema(
            KeySchemaElement::builder()
                .attribute_name(AuthProofAttribute::Nullifier.to_string())
                .key_type(KeyType::Hash)
                .build()
                .unwrap(),
        )
        .billing_mode(aws_sdk_dynamodb::types::BillingMode::PayPerRequest)
        .send()
        .await
        .expect("Failed to create test table");

    // Enable TTL
    dynamodb_client
        .update_time_to_live()
        .table_name(&table_name)
        .time_to_live_specification(
            aws_sdk_dynamodb::types::TimeToLiveSpecification::builder()
                .enabled(true)
                .attribute_name(AuthProofAttribute::Ttl.to_string())
                .build()
                .unwrap(),
        )
        .send()
        .await
        .expect("Failed to enable TTL");

    // Wait a bit for table to be ready
    tokio::time::sleep(Duration::from_millis(100)).await;

    let storage = AuthProofStorage::new(dynamodb_client.clone(), table_name.clone());

    TestContext {
        storage,
        table_name,
        dynamodb_client,
    }
}

/// Creates a test auth proof with unique nullifier
fn create_test_auth_proof() -> AuthProof {
    AuthProof {
        nullifier: format!("test-nullifier-{}", Uuid::new_v4()),
        encrypted_push_id: format!("encrypted-{}", Uuid::new_v4()),
        updated_at: Utc::now().timestamp(),
        ttl: (Utc::now() + chrono::Duration::hours(24)).timestamp(),
    }
}

#[tokio::test]
async fn test_get_by_nullifier() {
    let context = setup_test().await;

    let auth_proof = create_test_auth_proof();

    // Insert auth proof
    context
        .storage
        .insert(&auth_proof)
        .await
        .expect("Failed to insert auth proof");

    // Get by nullifier - should exist
    let retrieved = context
        .storage
        .get_by_nullifier(&auth_proof.nullifier)
        .await
        .expect("Failed to get by nullifier");

    assert!(retrieved.is_some());
    let retrieved = retrieved.unwrap();
    assert_eq!(retrieved.nullifier, auth_proof.nullifier);
    assert_eq!(retrieved.encrypted_push_id, auth_proof.encrypted_push_id);
    assert_eq!(retrieved.updated_at, auth_proof.updated_at);
    assert_eq!(retrieved.ttl, auth_proof.ttl);

    // Get non-existent nullifier - should return None
    let non_existent = context
        .storage
        .get_by_nullifier("non-existent-nullifier")
        .await
        .expect("Failed to get non-existent");

    assert!(non_existent.is_none());
}

#[tokio::test]
async fn test_insert_duplicate_prevention() {
    let context = setup_test().await;

    let auth_proof = create_test_auth_proof();

    // First insert should succeed
    context
        .storage
        .insert(&auth_proof)
        .await
        .expect("First insert should succeed");

    // Second insert with same nullifier should fail
    let result = context.storage.insert(&auth_proof).await;
    assert!(matches!(
        result,
        Err(AuthProofStorageError::AuthProofExists)
    ));

    // Insert with different nullifier should succeed
    let mut auth_proof2 = auth_proof.clone();
    auth_proof2.nullifier = format!("different-nullifier-{}", Uuid::new_v4());

    context
        .storage
        .insert(&auth_proof2)
        .await
        .expect("Insert with different nullifier should succeed");
}

#[tokio::test]
async fn test_update_encrypted_push_id() {
    let context = setup_test().await;

    let auth_proof = create_test_auth_proof();

    // Insert auth proof
    context
        .storage
        .insert(&auth_proof)
        .await
        .expect("Failed to insert auth proof");

    // Get initial state
    let initial = context
        .storage
        .get_by_nullifier(&auth_proof.nullifier)
        .await
        .expect("Failed to get initial state")
        .expect("Auth proof should exist");

    let initial_updated_at = initial.updated_at;

    // Wait a bit to ensure timestamp difference
    tokio::time::sleep(Duration::from_millis(1000)).await;

    // Update encrypted push id
    let new_encrypted_push_id = format!("new-encrypted-{}", Uuid::new_v4());
    context
        .storage
        .update_encrypted_push_id(&auth_proof.nullifier, &new_encrypted_push_id)
        .await
        .expect("Failed to update encrypted push id");

    // Retrieve and verify changes
    let updated = context
        .storage
        .get_by_nullifier(&auth_proof.nullifier)
        .await
        .expect("Failed to get updated auth proof")
        .expect("Auth proof should exist");

    assert_eq!(updated.encrypted_push_id, new_encrypted_push_id);
    assert!(
        updated.updated_at > initial_updated_at,
        "updated_at should be newer after update"
    );
    assert_eq!(updated.nullifier, auth_proof.nullifier);
    assert_eq!(updated.ttl, auth_proof.ttl);
}
