use pontifex::http::HttpClient;

#[derive(Default)]
pub struct EnclaveState {
    pub braze_api_key: Option<String>,
    pub braze_api_endpoint: Option<String>,
    pub http_proxy_client: Option<HttpClient>,
    pub initialized: bool,
}
