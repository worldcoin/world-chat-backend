use anyhow::{anyhow, Context};

use crate::xmtp::message_api::v1::Envelope;
use crate::xmtp::mls::api::v1::{group_message, GroupMessage};
use prost::Message as ProstMessage;

use hmac::{Hmac, Mac};
use sha2::Sha256;

// Define type alias for convenience
type HmacSha256 = Hmac<Sha256>;

const V3_GROUP_TOPIC_PREFIX: &str = "/xmtp/mls/1/g-";
const V3_WELCOME_TOPIC_PREFIX: &str = "/xmtp/mls/1/w-";

/// Checks if a topic is a V3 topic (either conversation or welcome)
#[must_use]
pub fn is_v3_topic(content_topic: &str) -> bool {
    content_topic.starts_with(V3_GROUP_TOPIC_PREFIX)
        || content_topic.starts_with(V3_WELCOME_TOPIC_PREFIX)
}

/// Message types in the XMTP protocol
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageType {
    Test,
    V3Group,
    V3Welcome,
    Unknown,
}

impl From<&str> for MessageType {
    fn from(content_topic: &str) -> Self {
        if content_topic.starts_with("test-") {
            Self::Test
        } else if content_topic.starts_with(V3_GROUP_TOPIC_PREFIX) {
            Self::V3Group
        } else if content_topic.starts_with(V3_WELCOME_TOPIC_PREFIX) {
            Self::V3Welcome
        } else {
            Self::Unknown
        }
    }
}

/// Message context for notification routing
#[derive(Debug, Clone)]
pub struct MessageContext {
    pub message_type: MessageType,
    pub sender_hmac: Option<Vec<u8>>,
    pub should_push: Option<bool>,
    pub hmac_inputs: Option<Vec<u8>>,
}

impl MessageContext {
    /// Creates a message context from an XMTP envelope.
    ///
    /// # Errors
    ///
    /// Returns an error if the envelope contains a V3 group message that cannot be decoded.
    pub fn from_xmtp_envelope(envelope: &Envelope) -> anyhow::Result<Self> {
        let message_type = MessageType::from(envelope.content_topic.as_str());

        if message_type != MessageType::V3Group {
            return Ok(Self {
                message_type,
                sender_hmac: None,
                should_push: None,
                hmac_inputs: None,
            });
        }

        let group_message = decode_group_message(envelope)?;
        Ok(Self {
            message_type,
            sender_hmac: Some(group_message.sender_hmac),
            should_push: Some(group_message.should_push),
            hmac_inputs: Some(group_message.data),
        })
    }

    /// Checks if the message sender matches the provided HMAC key.
    ///
    /// # Errors
    ///
    /// Returns an error if sender HMAC or HMAC inputs are missing, or if the HMAC key is invalid.
    pub fn is_sender(&self, hex_hmac_key: &str) -> anyhow::Result<bool> {
        let hmac_key = hex::decode(hex_hmac_key).context("Invalid HMAC key")?;
        let sender = self
            .sender_hmac
            .as_deref()
            .context("Sender HMAC is required")?;
        let input = self
            .hmac_inputs
            .as_deref()
            .context("HMAC inputs are required")?;

        let mut mac = HmacSha256::new_from_slice(&hmac_key).context("invalid HMAC key")?;

        mac.update(input);

        Ok(mac.verify_slice(sender).is_ok())
    }
}

/// Decodes a `GroupMessage` V1 from an XMTP envelope.
///
/// # Errors
///
/// Returns an error if:
/// - The envelope message cannot be decoded as a `GroupMessage`
/// - The `GroupMessage` is not a V1 variant
pub fn decode_group_message(envelope: &Envelope) -> anyhow::Result<group_message::V1> {
    let group_message = GroupMessage::decode(envelope.message.as_slice())
        .context("Failed to decode GroupMessage")?;

    match group_message.version {
        Some(group_message::Version::V1(v1)) => Ok(v1),
        _ => Err(anyhow!("Not a V1 group message")),
    }
}
