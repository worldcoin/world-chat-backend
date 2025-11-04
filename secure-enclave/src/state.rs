use anyhow::anyhow;
use pontifex::{http::HttpClient, SecureModule};

use crate::encryption::KeyPair;

use attestation_verifier::EnclaveAttestationVerifier;

pub struct EnclaveState {
    /// Braze API key
    pub braze_api_key: Option<String>,
    /// Braze API Url
    pub braze_api_url: Option<String>,
    /// HTTP client configured to use the HTTP proxy for Braze
    pub http_proxy_client: Option<HttpClient>,
    /// Whether the enclave has been initialized by creating a private key or receiving a key from another enclave
    pub initialized: bool,
    /// Encryption key pair used for encrypting/decrypting push IDs
    pub encryption_keys: Option<KeyPair>,
    /// Ephemeral key pair used for exchanging keys, destroyed after initialization
    pub ephemeral_key_pair: Option<KeyPair>,
    /// Attestation document generated with the enclave's ephemeral public key.
    pub attestation_doc_with_ephemeral_pk: Vec<u8>,
    /// Attestation verifier initialized with the enclave's attestation document.
    /// Used for verifying incoming attestation documents come from enclaves running the same bytecode.
    pub attestation_verifier: EnclaveAttestationVerifier,
}

impl EnclaveState {
    pub async fn new() -> anyhow::Result<Self> {
        let ephemeral_key_pair = KeyPair::generate();

        let nsm = pontifex::SecureModule::try_init_global()
            .await
            .map_err(|e| anyhow!("Error initializing nsm: {e}"))?;

        let ephemeral_pk = ephemeral_key_pair.public_key.to_bytes();
        let raw_attestation_doc = nsm
            .raw_attest(None::<Vec<u8>>, None::<Vec<u8>>, Some(ephemeral_pk))
            .map_err(|e| anyhow!("Error generating initial attestation: {e}"))?;
        let parsed_attestation_doc = SecureModule::parse_raw_attestation_doc(&raw_attestation_doc)
            .map_err(|e| anyhow!("Error parsing initial attestation: {e}"))?;

        let attestation_verifier =
            EnclaveAttestationVerifier::from_attestation_doc(&parsed_attestation_doc)
                .map_err(|e| anyhow!("Error initializing attestation verifier: {e}"))?;

        Ok(Self {
            encryption_keys: None,
            braze_api_key: None,
            braze_api_url: None,
            http_proxy_client: None,
            initialized: false,
            ephemeral_key_pair: Some(ephemeral_key_pair),
            attestation_doc_with_ephemeral_pk: raw_attestation_doc,
            attestation_verifier,
        })
    }
}
