use pontifex::http::HttpClient;

use crate::encryption::KeyPair;

pub struct EnclaveState {
    pub braze_api_key: Option<String>,
    pub braze_api_url: Option<String>,
    pub http_proxy_client: Option<HttpClient>,
    pub initialized: bool,
    pub keys: KeyPair,
}

impl EnclaveState {
    pub fn new(keys: KeyPair) -> Self {
        Self {
            keys,
            braze_api_key: None,
            braze_api_url: None,
            http_proxy_client: None,
            initialized: false,
        }
    }
}
