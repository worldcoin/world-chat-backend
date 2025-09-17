use super::TestSetup;
use chrono::Utc;
use rand::{distributions::Alphanumeric, Rng};

pub fn generate_hmac_key() -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(84)
        .map(char::from)
        .collect()
}

pub async fn subscription_exists(
    context: &TestSetup,
    topic: &str,
    hmac_key: &str,
    encrypted_push_id: &str,
) -> bool {
    context
        .push_subscription_storage
        .get_one(topic, hmac_key)
        .await
        .expect("Failed to get subscription")
        // ensure subscription exists and encrypted_push_id matches
        .is_some_and(|sub| sub.encrypted_push_id == encrypted_push_id)
}

pub async fn create_subscription(
    context: &TestSetup,
    topic: &str,
    hmac_key: &str,
    encrypted_push_id: &str,
) {
    use backend_storage::push_subscription::PushSubscription;

    let subscription = PushSubscription {
        topic: topic.to_string(),
        hmac_key: hmac_key.to_string(),
        ttl: Utc::now().timestamp() + 3600,
        encrypted_push_id: encrypted_push_id.to_string(),
        deletion_request: None,
    };

    context
        .push_subscription_storage
        .insert(&subscription)
        .await
        .expect("Failed to insert subscription");
}

pub async fn subscription_has_deletion_request(
    context: &TestSetup,
    topic: &str,
    hmac_key: &str,
    encrypted_push_id: &str,
) -> bool {
    context
        .push_subscription_storage
        .get_one(topic, hmac_key)
        .await
        .expect("Failed to get subscription")
        .is_some_and(|sub| {
            sub.deletion_request
                .is_some_and(|requests| requests.contains(encrypted_push_id))
        })
}
