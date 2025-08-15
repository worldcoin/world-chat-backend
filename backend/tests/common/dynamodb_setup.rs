use aws_sdk_dynamodb::types::{
    AttributeDefinition, BillingMode, KeySchemaElement, KeyType, ScalarAttributeType,
    TimeToLiveSpecification,
};
use aws_sdk_dynamodb::Client as DynamoDbClient;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

/// Helper for creating and managing DynamoDB tables in tests
///
/// Creates every table used in backend server.
pub struct DynamoDbTestSetup {
    client: Arc<DynamoDbClient>,
    pub auth_proofs_table_name: String,
}

impl DynamoDbTestSetup {
    pub async fn new(client: Arc<DynamoDbClient>) -> Self {
        let auth_proofs_table_name = Self::create_auth_proofs_table(&client).await;

        Self {
            client,
            auth_proofs_table_name,
        }
    }

    /// Creates a test auth proofs table with a unique name
    async fn create_auth_proofs_table(client: &DynamoDbClient) -> String {
        let table_name = format!("test-auth-proofs-{}", Uuid::new_v4());

        // Create table with nullifier as the primary key
        client
            .create_table()
            .table_name(&table_name)
            .attribute_definitions(
                AttributeDefinition::builder()
                    .attribute_name("nullifier")
                    .attribute_type(ScalarAttributeType::S)
                    .build()
                    .unwrap(),
            )
            .key_schema(
                KeySchemaElement::builder()
                    .attribute_name("nullifier")
                    .key_type(KeyType::Hash)
                    .build()
                    .unwrap(),
            )
            .billing_mode(BillingMode::PayPerRequest)
            .send()
            .await
            .expect("Failed to create test table");

        // Enable TTL on the table
        client
            .update_time_to_live()
            .table_name(&table_name)
            .time_to_live_specification(
                TimeToLiveSpecification::builder()
                    .enabled(true)
                    .attribute_name("ttl")
                    .build()
                    .unwrap(),
            )
            .send()
            .await
            .expect("Failed to enable TTL");

        // Wait for table to be ready
        tokio::time::sleep(Duration::from_millis(100)).await;

        table_name
    }

    /// Deletes a test table (used for cleanup)
    pub async fn delete_table(&self, table_name: &str) {
        let _ = self
            .client
            .delete_table()
            .table_name(table_name)
            .send()
            .await;
    }
}

impl Drop for DynamoDbTestSetup {
    fn drop(&mut self) {
        // Clean up all tables
        let client = self.client.clone();
        let auth_proofs_table_name = self.auth_proofs_table_name.clone();

        // Use tokio runtime to delete table
        let handle = tokio::runtime::Handle::try_current();
        if let Ok(handle) = handle {
            handle.spawn(async move {
                let _ = client
                    .delete_table()
                    .table_name(&auth_proofs_table_name)
                    .send()
                    .await;
            });
        }
    }
}
