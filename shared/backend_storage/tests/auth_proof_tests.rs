use std::sync::Arc;
use std::time::Duration;

use aws_config::{BehaviorVersion, Region};
use aws_credential_types::Credentials;
use aws_sdk_dynamodb::types::{
    AttributeDefinition, KeySchemaElement, KeyType, ScalarAttributeType,
};
use aws_sdk_dynamodb::Client as DynamoDbClient;
use backend_storage::auth_proof::{
    AuthProofAttribute, AuthProofInsertRequest, AuthProofStorage, AuthProofStorageError,
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

/// Creates a test auth proof insert request with unique nullifier
fn create_test_auth_proof_request() -> AuthProofInsertRequest {
    AuthProofInsertRequest {
        nullifier: format!("test-nullifier-{}", Uuid::new_v4()),
        encrypted_push_id: format!("encrypted-{}", Uuid::new_v4()),
    }
}

#[tokio::test]
async fn test_get_by_nullifier() {
    let context = setup_test().await;

    let auth_proof_request = create_test_auth_proof_request();

    // Insert auth proof and get the returned AuthProof
    let inserted_auth_proof = context
        .storage
        .insert(auth_proof_request.clone())
        .await
        .expect("Failed to insert auth proof");

    // Get by nullifier - should exist
    let retrieved = context
        .storage
        .get_by_nullifier(&auth_proof_request.nullifier)
        .await
        .expect("Failed to get by nullifier");

    assert!(retrieved.is_some());
    let retrieved = retrieved.unwrap();
    assert_eq!(retrieved.nullifier, inserted_auth_proof.nullifier);
    assert_eq!(
        retrieved.encrypted_push_id,
        inserted_auth_proof.encrypted_push_id
    );
    assert_eq!(
        retrieved.push_id_rotated_at,
        inserted_auth_proof.push_id_rotated_at
    );
    assert_eq!(retrieved.ttl, inserted_auth_proof.ttl);

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

    let auth_proof_request = create_test_auth_proof_request();

    // First insert should succeed
    context
        .storage
        .insert(auth_proof_request.clone())
        .await
        .expect("First insert should succeed");

    // Second insert with same nullifier should fail
    let result = context.storage.insert(auth_proof_request.clone()).await;
    assert!(matches!(
        result,
        Err(AuthProofStorageError::AuthProofExists)
    ));

    // Insert with different nullifier should succeed
    let mut auth_proof_request2 = auth_proof_request.clone();
    auth_proof_request2.nullifier = format!("different-nullifier-{}", Uuid::new_v4());

    context
        .storage
        .insert(auth_proof_request2)
        .await
        .expect("Insert with different nullifier should succeed");
}

#[tokio::test]
async fn test_update_encrypted_push_id() {
    let context = setup_test().await;

    let auth_proof_request = create_test_auth_proof_request();

    // Insert auth proof and get the returned AuthProof
    let _inserted_auth_proof = context
        .storage
        .insert(auth_proof_request.clone())
        .await
        .expect("Failed to insert auth proof");

    // Get initial state
    let initial = context
        .storage
        .get_by_nullifier(&auth_proof_request.nullifier)
        .await
        .expect("Failed to get initial state")
        .expect("Auth proof should exist");

    let initial_push_id_rotated_at = initial.push_id_rotated_at;

    // Update encrypted push id
    let new_encrypted_push_id = format!("new-encrypted-{}", Uuid::new_v4());
    context
        .storage
        .update_encrypted_push_id(&auth_proof_request.nullifier, &new_encrypted_push_id)
        .await
        .expect("Failed to update encrypted push id");

    // Retrieve and verify changes
    let updated = context
        .storage
        .get_by_nullifier(&auth_proof_request.nullifier)
        .await
        .expect("Failed to get updated auth proof")
        .expect("Auth proof should exist");

    assert_eq!(updated.encrypted_push_id, new_encrypted_push_id);
    // With rounding to nearest day, push_id_rotated_at will likely be the same unless test runs across midnight
    assert!(
        updated.push_id_rotated_at >= initial_push_id_rotated_at,
        "push_id_rotated_at should not go backwards"
    );
    assert_eq!(updated.nullifier, auth_proof_request.nullifier);
    // TTL is randomized, just verify it's set to a valid future value
    let now = chrono::Utc::now().timestamp();
    assert!(updated.ttl > now, "TTL should be set to a future timestamp");
}

#[tokio::test]
async fn test_ping_auth_proof() {
    let context = setup_test().await;

    let auth_proof_request = create_test_auth_proof_request();

    // Insert auth proof
    let _inserted_auth_proof = context
        .storage
        .insert(auth_proof_request.clone())
        .await
        .expect("Failed to insert auth proof");

    // Get initial state
    let initial = context
        .storage
        .get_by_nullifier(&auth_proof_request.nullifier)
        .await
        .expect("Failed to get initial state")
        .expect("Auth proof should exist");

    let initial_push_id_rotated_at = initial.push_id_rotated_at;
    let initial_encrypted_push_id = initial.encrypted_push_id.clone();

    // Wait a bit to ensure we can detect timestamp differences if they occur
    tokio::time::sleep(Duration::from_millis(1000)).await;

    // Ping the auth proof to refresh TTL
    context
        .storage
        .ping_auth_proof(&auth_proof_request.nullifier)
        .await
        .expect("Failed to ping auth proof");

    // Retrieve and verify changes
    let pinged = context
        .storage
        .get_by_nullifier(&auth_proof_request.nullifier)
        .await
        .expect("Failed to get pinged auth proof")
        .expect("Auth proof should exist");

    // Verify that ONLY TTL changed - for privacy reasons, push_id_rotated_at should NOT change
    assert_eq!(
        pinged.encrypted_push_id, initial_encrypted_push_id,
        "encrypted_push_id should not change on ping"
    );
    assert_eq!(
        pinged.push_id_rotated_at, initial_push_id_rotated_at,
        "push_id_rotated_at should NOT change on ping (privacy: no 'last seen' tracking)"
    );
    // TTL is randomized, just verify it's set to a valid future value
    let now = Utc::now().timestamp();
    assert!(pinged.ttl > now, "TTL should be set to a future timestamp");
    assert_eq!(pinged.nullifier, auth_proof_request.nullifier);
}

#[tokio::test]
async fn test_get_or_insert_creates_new() {
    let context = setup_test().await;

    let auth_proof_request = create_test_auth_proof_request();
    let nullifier = auth_proof_request.nullifier.clone();

    // Verify it doesn't exist yet
    let not_exists = context
        .storage
        .get_by_nullifier(&nullifier)
        .await
        .expect("Failed to check existence");
    assert!(not_exists.is_none(), "Should not exist initially");

    // Call get_or_insert - should create new entry
    let created = context
        .storage
        .get_or_insert(auth_proof_request.clone())
        .await
        .expect("Failed to get_or_insert");

    // Verify it was created with correct values
    assert_eq!(created.nullifier, nullifier);
    assert_eq!(
        created.encrypted_push_id,
        auth_proof_request.encrypted_push_id
    );

    // Verify TTL is set to future
    let now: i64 = Utc::now().timestamp();
    assert!(created.ttl > now, "TTL should be set to a future timestamp");

    // Verify it's actually rounded to midnight (00:00:00)
    const SECONDS_IN_DAY: i64 = 86400;
    assert_eq!(
        created.push_id_rotated_at % SECONDS_IN_DAY,
        0,
        "push_id_rotated_at should be rounded to midnight (00:00:00)"
    );

    // Verify it's close to current time (within 1 day)
    let diff = (now - created.push_id_rotated_at).abs();
    assert!(
        diff <= SECONDS_IN_DAY,
        "push_id_rotated_at should be within 1 day of current time, but diff is {} seconds",
        diff
    );
}

#[tokio::test]
async fn test_get_or_insert_returns_existing() {
    let context = setup_test().await;

    let auth_proof_request = create_test_auth_proof_request();
    let nullifier = auth_proof_request.nullifier.clone();

    // First, insert an auth proof
    let inserted = context
        .storage
        .insert(auth_proof_request.clone())
        .await
        .expect("Failed to insert auth proof");

    // Create a different request with same nullifier but different encrypted_push_id
    let different_request = AuthProofInsertRequest {
        nullifier: nullifier.clone(),
        encrypted_push_id: format!("different-encrypted-{}", Uuid::new_v4()),
    };

    // Call get_or_insert - should return existing entry, NOT create new
    let existing = context
        .storage
        .get_or_insert(different_request.clone())
        .await
        .expect("Failed to get_or_insert");

    // Verify it returned the ORIGINAL values, not the new ones
    assert_eq!(existing.nullifier, inserted.nullifier);
    assert_eq!(
        existing.encrypted_push_id, inserted.encrypted_push_id,
        "Should return original encrypted_push_id, not the new one"
    );
    assert_eq!(
        existing.push_id_rotated_at, inserted.push_id_rotated_at,
        "Should return original push_id_rotated_at"
    );
    assert_eq!(existing.ttl, inserted.ttl, "Should return original ttl");

    // Verify the different_request values were NOT used
    assert_ne!(
        existing.encrypted_push_id, different_request.encrypted_push_id,
        "Should NOT have used the new encrypted_push_id"
    );
}

#[tokio::test]
async fn test_get_or_insert_atomic_concurrent() {
    let context = setup_test().await;

    // Create multiple requests with the same nullifier
    let nullifier = format!("concurrent-nullifier-{}", Uuid::new_v4());
    let requests: Vec<AuthProofInsertRequest> = (0..5)
        .map(|i| AuthProofInsertRequest {
            nullifier: nullifier.clone(),
            encrypted_push_id: format!("encrypted-concurrent-{}-{}", i, Uuid::new_v4()),
        })
        .collect();

    // Run get_or_insert concurrently
    let futures: Vec<_> = requests
        .into_iter()
        .map(|req| {
            let storage = context.storage.clone();
            async move { storage.get_or_insert(req.clone()).await }
        })
        .collect();

    let results = futures::future::join_all(futures).await;

    // All should succeed
    for result in &results {
        assert!(
            result.is_ok(),
            "All concurrent get_or_insert should succeed"
        );
    }

    // Extract successful results
    let auth_proofs: Vec<_> = results.into_iter().map(|r| r.unwrap()).collect();

    // All should have the same values (atomicity check)
    let first = &auth_proofs[0];
    for auth_proof in &auth_proofs[1..] {
        assert_eq!(auth_proof.nullifier, first.nullifier);
        assert_eq!(
            auth_proof.encrypted_push_id, first.encrypted_push_id,
            "All concurrent calls should return the same encrypted_push_id"
        );
        assert_eq!(
            auth_proof.push_id_rotated_at, first.push_id_rotated_at,
            "All concurrent calls should return the same push_id_rotated_at"
        );
        assert_eq!(
            auth_proof.ttl, first.ttl,
            "All concurrent calls should return the same ttl"
        );
    }

    // Verify only one entry exists in the database
    let final_check = context
        .storage
        .get_by_nullifier(&nullifier)
        .await
        .expect("Failed to get final state")
        .expect("Should exist");

    assert_eq!(final_check.encrypted_push_id, first.encrypted_push_id);
}
