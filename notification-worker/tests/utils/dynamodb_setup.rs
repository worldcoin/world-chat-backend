use aws_sdk_dynamodb::types::{
    AttributeDefinition, GlobalSecondaryIndex, KeySchemaElement, KeyType, Projection,
    ProjectionType, ScalarAttributeType,
};
use aws_sdk_dynamodb::Client as DynamoDbClient;
use backend_storage::push_notification::PushSubscriptionAttribute;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

/// Helper for creating and managing DynamoDB tables in tests
///
/// Creates every table used in backend server.
pub struct DynamoDbTestSetup {
    client: Arc<DynamoDbClient>,
    pub push_subscriptions_table_name: String,
    pub push_subscription_gsi_name: String,
}

impl DynamoDbTestSetup {
    pub async fn new(client: Arc<DynamoDbClient>) -> Self {
        let (push_subscriptions_table_name, push_subscription_gsi_name) =
            Self::create_push_subscriptions_table(&client).await;

        Self {
            client,
            push_subscriptions_table_name,
            push_subscription_gsi_name,
        }
    }

    /// Creates a test auth proofs table with a unique name
    async fn create_push_subscriptions_table(client: &DynamoDbClient) -> (String, String) {
        let table_name = format!("test-push-subscriptions-{}", Uuid::new_v4());
        let gsi_name = format!("test-push-subscriptions-gsi-{}", Uuid::new_v4());

        // Create table with nullifier as the primary key
        client
            .create_table()
            .table_name(&table_name)
            .attribute_definitions(
                AttributeDefinition::builder()
                    .attribute_name(PushSubscriptionAttribute::Hmac.to_string())
                    .attribute_type(ScalarAttributeType::S)
                    .build()
                    .unwrap(),
            )
            .attribute_definitions(
                AttributeDefinition::builder()
                    .attribute_name(PushSubscriptionAttribute::Topic.to_string())
                    .attribute_type(ScalarAttributeType::S)
                    .build()
                    .unwrap(),
            )
            .key_schema(
                KeySchemaElement::builder()
                    .attribute_name(PushSubscriptionAttribute::Hmac.to_string())
                    .key_type(KeyType::Hash)
                    .build()
                    .unwrap(),
            )
            .global_secondary_indexes(
                GlobalSecondaryIndex::builder()
                    .index_name(&gsi_name)
                    .key_schema(
                        KeySchemaElement::builder()
                            .attribute_name(PushSubscriptionAttribute::Topic.to_string())
                            .key_type(KeyType::Hash)
                            .build()
                            .unwrap(),
                    )
                    .projection(
                        Projection::builder()
                            .projection_type(ProjectionType::All)
                            .build(),
                    )
                    .build()
                    .unwrap(),
            )
            .billing_mode(aws_sdk_dynamodb::types::BillingMode::PayPerRequest)
            .send()
            .await
            .expect("Failed to create test table");

        // Enable TTL
        client
            .update_time_to_live()
            .table_name(&table_name)
            .time_to_live_specification(
                aws_sdk_dynamodb::types::TimeToLiveSpecification::builder()
                    .enabled(true)
                    .attribute_name(PushSubscriptionAttribute::Ttl.to_string())
                    .build()
                    .unwrap(),
            )
            .send()
            .await
            .expect("Failed to enable TTL");

        // Wait for table to be ready
        tokio::time::sleep(Duration::from_millis(100)).await;

        (table_name, gsi_name)
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
        let push_subscriptions_table_name = self.push_subscriptions_table_name.clone();

        // Use tokio runtime to delete table
        let handle = tokio::runtime::Handle::try_current();
        if let Ok(handle) = handle {
            handle.spawn(async move {
                let _ = client
                    .delete_table()
                    .table_name(&push_subscriptions_table_name)
                    .send()
                    .await;
            });
        }
    }
}
