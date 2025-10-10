mod common;

use http::{Method, StatusCode};
use uuid::Uuid;

use crate::common::{
    create_subscription, generate_hmac_key, subscription_exists, subscription_has_deletion_request,
    TestSetup,
};

/// Assert we get 401, if enable auth
/// Tests that route is protected
#[tokio::test]
async fn test_unsubscribe_without_auth_header() {
    let context = TestSetup::new(None, false).await; // Auth enabled

    let topic = format!("topic-{}", Uuid::new_v4());
    let hmac_key = generate_hmac_key();
    let url = format!("/v1/subscriptions?topic={}&hmac_key={}", topic, hmac_key);

    let response = context
        .send_request(Method::DELETE, &url, None, None)
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

/// Assert we get 401, if enable auth
/// Tests that route is protected
#[tokio::test]
async fn test_unsubscribe_with_invalid_auth_header() {
    let context = TestSetup::new(None, false).await; // Auth enabled

    let topic = format!("topic-{}", Uuid::new_v4());
    let hmac_key = generate_hmac_key();
    let url = format!("/v1/subscriptions?topic={}&hmac_key={}", topic, hmac_key);

    let response = context
        .send_request(
            Method::DELETE,
            &url,
            None,
            Some(vec![(
                "Authorization",
                "Bearer invalid.jwt.encrypted_push_id",
            )]),
        )
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_unsubscribe_missing_required_fields() {
    let context = TestSetup::default().await;
    let encrypted_push_id = format!("encrypted-push-{}", Uuid::new_v4());

    let test_cases = vec![
        (
            // Missing topic
            format!("/v1/subscriptions?hmac_key={}", generate_hmac_key()),
            "missing topic",
        ),
        (
            // Missing hmac_key
            format!(
                "/v1/subscriptions?topic={}",
                format!("topic-{}", Uuid::new_v4())
            ),
            "missing hmac_key",
        ),
    ];

    for (url, case_name) in test_cases {
        let response = context
            .send_request(
                Method::DELETE,
                &url,
                None,
                Some(vec![(
                    "Authorization",
                    &format!("Bearer {}", encrypted_push_id),
                )]),
            )
            .await
            .expect("Failed to send request");

        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "Request with {} should return 400",
            case_name
        );
    }
}

#[tokio::test]
async fn test_unsubscribe_empty_string_fields() {
    let context = TestSetup::default().await;
    let encrypted_push_id = format!("encrypted-push-{}", Uuid::new_v4());

    let test_cases = vec![
        // Empty topic string
        format!("/v1/subscriptions?topic=&hmac_key={}", generate_hmac_key()),
        // Empty hmac_key string
        format!(
            "/v1/subscriptions?topic={}&hmac_key=",
            format!("topic-{}", Uuid::new_v4())
        ),
        // Too short hmac_key
        format!(
            "/v1/subscriptions?topic={}&hmac_key=abc123",
            format!("topic-{}", Uuid::new_v4())
        ),
    ];

    for url in test_cases {
        let response = context
            .send_request(
                Method::DELETE,
                &url,
                None,
                Some(vec![(
                    "Authorization",
                    &format!("Bearer {}", encrypted_push_id),
                )]),
            )
            .await
            .expect("Failed to send request");

        // Note: With query parameters, validation behavior might differ
        // Empty strings might be accepted differently than with JSON body
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}

#[tokio::test]
async fn test_unsubscribe_extra_fields_ignored() {
    let context = TestSetup::default().await;
    let encrypted_push_id = format!("encrypted-push-{}", Uuid::new_v4());

    let topic = format!("topic-{}", Uuid::new_v4());
    let hmac_key = generate_hmac_key();

    // Create a subscription first so we can test successful unsubscribe
    create_subscription(&context, &topic, &hmac_key, &encrypted_push_id).await;

    // With query parameters, extra fields are typically ignored, not rejected
    let url = format!(
        "/v1/subscriptions?topic={}&hmac_key={}&extra_field=should_be_ignored",
        topic, hmac_key
    );

    let response = context
        .send_request(
            Method::DELETE,
            &url,
            None,
            Some(vec![(
                "Authorization",
                &format!("Bearer {}", encrypted_push_id),
            )]),
        )
        .await
        .expect("Failed to send request");

    // Extra query parameters should be ignored, operation should succeed
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn test_unsubscribe_nonexistent_subscription() {
    let context = TestSetup::default().await;
    let encrypted_push_id = format!("encrypted-push-{}", Uuid::new_v4());

    let topic = format!("nonexistent-topic-{}", Uuid::new_v4());
    let hmac_key = generate_hmac_key();
    let url = format!("/v1/subscriptions?topic={}&hmac_key={}", topic, hmac_key);

    let response = context
        .send_request(
            Method::DELETE,
            &url,
            None,
            Some(vec![(
                "Authorization",
                &format!("Bearer {}", encrypted_push_id),
            )]),
        )
        .await
        .expect("Failed to send request");

    // Should return 404 when subscription doesn't exist
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_unsubscribe_matching_push_id_deletes_document() {
    let context = TestSetup::default().await;
    let encrypted_push_id = format!("encrypted-push-{}", Uuid::new_v4());

    let topic = format!("topic-{}", Uuid::new_v4());
    let hmac_key = generate_hmac_key();

    // Create a subscription first
    create_subscription(&context, &topic, &hmac_key, &encrypted_push_id).await;
    assert!(subscription_exists(&context, &topic, &hmac_key, &encrypted_push_id).await);

    // Now unsubscribe with the same encrypted_push_id
    let url = format!("/v1/subscriptions?topic={}&hmac_key={}", topic, hmac_key);

    let response = context
        .send_request(
            Method::DELETE,
            &url,
            None,
            Some(vec![(
                "Authorization",
                &format!("Bearer {}", encrypted_push_id),
            )]),
        )
        .await
        .expect("Failed to send request");

    // Should return 204 NO_CONTENT on successful deletion
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    // Subscription should be completely deleted
    assert!(!subscription_exists(&context, &topic, &hmac_key, &encrypted_push_id).await);
}

#[tokio::test]
async fn test_unsubscribe_nonmatching_push_id_appends_deletion_request() {
    let context = TestSetup::default().await;
    let original_encrypted_push_id = format!("encrypted-push-{}", Uuid::new_v4());
    let different_encrypted_push_id = format!("different-encrypted-push-{}", Uuid::new_v4());

    let topic = format!("topic-{}", Uuid::new_v4());
    let hmac_key = generate_hmac_key();

    // Create a subscription with the original encrypted_push_id
    create_subscription(&context, &topic, &hmac_key, &original_encrypted_push_id).await;
    assert!(subscription_exists(&context, &topic, &hmac_key, &original_encrypted_push_id).await);

    // Now try to unsubscribe with a different encrypted_push_id
    let url = format!("/v1/subscriptions?topic={}&hmac_key={}", topic, hmac_key);

    let response = context
        .send_request(
            Method::DELETE,
            &url,
            None,
            Some(vec![(
                "Authorization",
                &format!("Bearer {}", different_encrypted_push_id),
            )]),
        )
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    // Subscription should still exist
    // But should have a deletion request for the different encrypted_push_id
    assert!(subscription_exists(&context, &topic, &hmac_key, &original_encrypted_push_id).await);
    assert!(
        subscription_has_deletion_request(
            &context,
            &topic,
            &hmac_key,
            &different_encrypted_push_id
        )
        .await
    );
}
