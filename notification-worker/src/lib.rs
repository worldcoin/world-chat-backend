#![deny(clippy::all, clippy::pedantic, clippy::nursery, dead_code)]

// Only include the proto modules we actually need
mod xmtp {
    pub mod message_api {
        pub mod v1 {
            include!("generated/xmtp.message_api.v1.rs");
        }
    }

    pub mod message_contents {
        include!("generated/xmtp.message_contents.rs");
    }
}

pub mod types;
pub mod worker;
