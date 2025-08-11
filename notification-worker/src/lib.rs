#![deny(clippy::all, clippy::pedantic, clippy::nursery, dead_code)]

// Only include the proto modules we actually need
pub mod xmtp {
    pub mod message_api {
        pub mod v1 {
            #![allow(clippy::all, clippy::pedantic, clippy::nursery)]
            include!("generated/xmtp.message_api.v1.rs");
        }
    }

    pub mod message_contents {
        #![allow(clippy::all, clippy::pedantic, clippy::nursery)]
        include!("generated/xmtp.message_contents.rs");
    }
}

pub mod types;
pub mod worker;
