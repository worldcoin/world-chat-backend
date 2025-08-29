use anyhow::{Context, Result};
use prost::Message as ProstMessage;
use std::time::Duration;
use tonic::transport::Channel;

// Import the generated XMTP types from notification-worker
use notification_worker::xmtp::{
    message_api::v1::{message_api_client::MessageApiClient, Envelope, PublishRequest},
    mls::api::v1::{group_message, GroupMessage},
};

/// Test client for sending messages to XMTP node
pub struct XmtpTestClient {
    client: MessageApiClient<Channel>,
}

impl XmtpTestClient {
    /// Create a new test client with custom config
    pub async fn new(endpoint: String) -> Result<Self> {
        let channel = Channel::from_shared(endpoint)
            .context("Failed to create channel")?
            .connect_timeout(Duration::from_secs(5))
            .connect()
            .await
            .context("Failed to connect to XMTP node")?;

        let client: MessageApiClient<Channel> = MessageApiClient::new(channel);

        Ok(Self { client })
    }

    /// Send a V3 group message to trigger notification processing
    pub async fn send_v3_group_message(
        &mut self,
        group_id: &str,
        message_content: Vec<u8>,
        should_push: bool,
        sender_hmac: Option<Vec<u8>>,
    ) -> Result<()> {
        // Create the inner GroupMessage_V1
        let v1_message = group_message::V1 {
            id: 1, // Test message ID
            created_ns: chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0) as u64,
            group_id: group_id.as_bytes().to_vec(),
            data: message_content.clone(),
            sender_hmac: sender_hmac.unwrap_or_else(|| vec![4, 5, 6]), // Default test HMAC
            should_push,
        };

        // Wrap in GroupMessage
        let group_message = GroupMessage {
            version: Some(group_message::Version::V1(v1_message)),
        };

        // Encode to bytes
        let mut message_bytes = Vec::new();
        group_message.encode(&mut message_bytes)?;

        // Create the envelope
        let envelope = Envelope {
            content_topic: format!("/xmtp/mls/1/{}/proto", group_id),
            timestamp_ns: chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0) as u64,
            message: message_bytes,
        };

        // Publish the message
        let request = PublishRequest {
            envelopes: vec![envelope],
        };

        self.client
            .publish(request)
            .await
            .context("Failed to publish message to XMTP")?;

        Ok(())
    }

    /// Send a V3 welcome message
    pub async fn send_v3_welcome_message(
        &mut self,
        installation_id: &str,
        welcome_data: Vec<u8>,
    ) -> Result<()> {
        let envelope = Envelope {
            content_topic: format!("/xmtp/mls/1/w-{}/proto", installation_id),
            timestamp_ns: chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0) as u64,
            message: welcome_data,
        };

        let request = PublishRequest {
            envelopes: vec![envelope],
        };

        self.client
            .publish(request)
            .await
            .context("Failed to publish welcome message")?;

        Ok(())
    }

    /// Send a raw envelope (for testing edge cases)
    pub async fn send_raw_envelope(&mut self, envelope: Envelope) -> Result<()> {
        let request = PublishRequest {
            envelopes: vec![envelope],
        };

        self.client
            .publish(request)
            .await
            .context("Failed to publish raw envelope")?;

        Ok(())
    }
}

/// Builder pattern for creating test messages
pub struct V3MessageBuilder {
    group_id: String,
    content: Vec<u8>,
    should_push: bool,
    sender_hmac: Option<Vec<u8>>,
}

impl V3MessageBuilder {
    pub fn new(group_id: impl Into<String>) -> Self {
        Self {
            group_id: group_id.into(),
            content: Vec::new(),
            should_push: true,
            sender_hmac: None,
        }
    }

    pub fn content(mut self, content: impl Into<Vec<u8>>) -> Self {
        self.content = content.into();
        self
    }

    pub fn should_push(mut self, should_push: bool) -> Self {
        self.should_push = should_push;
        self
    }

    pub fn sender_hmac(mut self, hmac: Vec<u8>) -> Self {
        self.sender_hmac = Some(hmac);
        self
    }

    pub async fn send(self, client: &mut XmtpTestClient) -> Result<()> {
        // For now, we're simplifying - the actual should_push flag would be in the message content
        // This is a simplified version for testing
        client
            .send_v3_group_message(
                &self.group_id,
                self.content,
                self.should_push,
                self.sender_hmac,
            )
            .await
    }
}
