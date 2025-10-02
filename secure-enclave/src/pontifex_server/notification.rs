use std::sync::Arc;

use crate::state::EnclaveState;
use enclave_types::{EnclaveError, EnclaveNotificationRequest};
use hyper::{Body, Method, Request, Version};
use pontifex::http::HttpClient;
use serde_json::json;
use tokio::sync::RwLock;

pub async fn handler(
    state: Arc<RwLock<EnclaveState>>,
    request: EnclaveNotificationRequest,
) -> Result<(), EnclaveError> {
    let state = state.read().await;

    if !state.initialized {
        return Err(EnclaveError::NotInitialized);
    }

    let client = state.http_proxy_client.as_ref().unwrap();
    let braze_api_key = state.braze_api_key.clone().unwrap();
    let braze_api_endpoint = state.braze_api_url.clone().unwrap();
    let braze_api_endpoint = format!("{braze_api_endpoint}/messages/send");

    send_braze_notification(
        client,
        braze_api_key,
        braze_api_endpoint,
        request.topic,
        request.subscribed_encrypted_push_ids,
        request.encrypted_message_base64,
    )
    .await?;

    Ok(())
}

async fn send_braze_notification(
    client: &HttpClient,
    braze_api_key: String,
    braze_api_endpoint: String,
    topic: String,
    subscribed_encrypted_push_ids: Vec<String>,
    encrypted_message_base64: String,
) -> Result<(), EnclaveError> {
    let body = json!({
        // TODO: Decrypt push IDs and use Braze user aliases instead
        "external_user_ids": subscribed_encrypted_push_ids,
        "messages": {
            "apple_push": {
                "alert": {
                    "title": "world_chat_notification",
                    "body": "world_chat_notification"
                },
                "badge": 1,
                "sound": "default",
                "mutable_content": true,
                "extra": {
                    "topic": topic,
                    "encryptedMessageBase64": encrypted_message_base64,
                    "messageKind": "v3-conversation"
                }
            },
            "android_push": {
                "title": "world_chat_notification",
                "alert": "world_chat_notification",
                "priority": "high",
                "extra": {
                    "topic": topic,
                    "encryptedMessageBase64": encrypted_message_base64,
                    "messageKind": "v3-conversation"
                }
            }
        }
    });
    let body = Body::from(body.to_string());

    let req = Request::builder()
        .method(Method::POST)
        .uri(braze_api_endpoint)
        .version(Version::HTTP_2)
        .header("Authorization", format!("Bearer {}", braze_api_key))
        .header("Content-Type", "application/json")
        .body(body)
        .map_err(|e| EnclaveError::BrazeRequestFailed(format!("{e:?}")))?;

    client
        .request(req)
        .await
        .map_err(|e| EnclaveError::BrazeRequestFailed(format!("{e:?}")))?;

    Ok(())
}
