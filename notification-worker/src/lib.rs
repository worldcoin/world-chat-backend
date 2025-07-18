// Only include the proto modules we actually need
pub mod xmtp {
    pub mod message_api {
        pub mod v1 {
            include!("generated/xmtp.message_api.v1.rs");
        }
    }

    pub mod message_contents {
        include!("generated/xmtp.message_contents.rs");
    }
}

// Re-export the client we'll use
pub use xmtp::message_api::v1::message_api_client::MessageApiClient;

pub mod generated {
    pub use crate::xmtp;
}

pub mod types;
pub mod worker;
