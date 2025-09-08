use enclave_types::{EnclaveRequest, EnclaveRequestType, EnclaveResponse};
use pontifex::ConnectionDetails;
use tracing::instrument;

use crate::types::AppError;

pub struct PontifexClient {
    conn: ConnectionDetails,
}

impl PontifexClient {
    pub fn new(enclave_cid: u32, enclave_port: u32) -> Self {
        Self {
            conn: ConnectionDetails::new(enclave_cid, enclave_port),
        }
    }

    /// Send a type-safe request to the enclave.
    ///
    /// Returns the response type associated with the request, eliminating the need
    /// for pattern matching on response variants.
    #[instrument(skip_all)]
    pub async fn send<R: EnclaveRequestType>(&self, request: R) -> Result<R::Response, AppError> {
        let enclave_request = request.into_request();
        let response =
            pontifex::send::<EnclaveRequest, EnclaveResponse>(self.conn, &enclave_request).await?;

        R::from_response(response).map_err(AppError::from)
    }
}
