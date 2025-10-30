use std::time::{SystemTime, UNIX_EPOCH};

use aws_nitro_enclaves_nsm_api::api::AttestationDoc;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use coset::{AsCborValue, CborSerializable, CoseSign1};
use crypto_box::{aead::OsRng, PublicKey};
use p384::ecdsa::{signature::Verifier as _, Signature, VerifyingKey};
use webpki::{EndEntityCert, TrustAnchor};
use x509_cert::{der::Decode, Certificate};

/// Constants for enclave verification
pub mod constants;

/// Types for enclave verification
pub mod types;

pub use types::{
    EnclaveAttestationError, EnclaveAttestationResult, VerifiedAttestation,
    VerifiedAttestationWithCiphertext,
};

use constants::{get_expected_pcr_length, AWS_NITRO_ROOT_CERT, MAX_ATTESTATION_AGE_MILLISECONDS};

/// Verifies AWS Nitro Enclave attestation documents
///
/// This class performs comprehensive verification of attestation documents including:
/// - COSE Sign1 signature verification
/// - Certificate chain validation against AWS Nitro root certificates
/// - PCR (Platform Configuration Register) value validation  
/// - Attestation document freshness checks
/// - Public key extraction
pub struct EnclaveAttestationVerifier {
    root_certificate: Vec<u8>,
    max_age_millis: u64,
    #[cfg(test)]
    skip_certificate_time_check: bool,
}

impl Default for EnclaveAttestationVerifier {
    fn default() -> Self {
        Self::new()
    }
}

impl EnclaveAttestationVerifier {
    /// Creates a new `EnclaveAttestationVerifier`
    ///
    /// # Arguments
    /// * `environment` - The environment to use for this verifier
    ///
    /// # Panics
    /// Panics if the Bedrock config is not initialized.
    #[must_use]
    pub fn new() -> Self {
        let root_certificate = AWS_NITRO_ROOT_CERT.to_vec();

        Self {
            root_certificate,
            max_age_millis: MAX_ATTESTATION_AGE_MILLISECONDS,
            #[cfg(test)]
            skip_certificate_time_check: false,
        }
    }

    /// Verifies a base64-encoded attestation document
    ///
    /// This is a convenience method that handles base64 decoding and then verifies the document
    ///
    /// # Arguments
    /// * `attestation_doc_base64` - The base64-encoded attestation document
    ///
    /// # Returns
    /// A verified attestation containing the enclave's public key and PCR values
    ///
    /// # Errors
    /// Returns an error if the base64 decoding fails or the attestation document verification fails
    pub fn verify_attestation_document_base64(
        &self,
        attestation_doc_base64: &str,
    ) -> EnclaveAttestationResult<VerifiedAttestation> {
        let attestation_doc_bytes = STANDARD.decode(attestation_doc_base64).map_err(|e| {
            EnclaveAttestationError::AttestationDocumentParseError(format!(
                "Failed to decode base64 attestation document: {e}"
            ))
        })?;

        self.verify_attestation_document(&attestation_doc_bytes)
    }

    /// Verifies two base64-encoded attestation documents against each other
    ///
    /// This method verifies both attestation documents and compares their PCR values.
    /// Instead of checking against hardcoded PCR values, it ensures that both attestations
    /// have matching PCR values for indices 0, 1, and 2.
    ///
    /// # Arguments
    /// * `attestation_doc_base64_1` - The first base64-encoded attestation document
    /// * `attestation_doc_base64_2` - The second base64-encoded attestation document
    ///
    /// # Returns
    /// A tuple of verified attestations containing each enclave's public key and PCR values
    ///
    /// # Errors
    /// Returns an error if base64 decoding fails, attestation document verification fails,
    /// or PCR values don't match between the two documents
    pub fn verify_dual_attestation_documents_base64(
        &self,
        attestation_doc_base64_1: &str,
        attestation_doc_base64_2: &str,
    ) -> EnclaveAttestationResult<(VerifiedAttestation, VerifiedAttestation)> {
        let attestation_doc_bytes_1 = STANDARD.decode(attestation_doc_base64_1).map_err(|e| {
            EnclaveAttestationError::AttestationDocumentParseError(format!(
                "Failed to decode base64 attestation document 1: {e}"
            ))
        })?;

        let attestation_doc_bytes_2 = STANDARD.decode(attestation_doc_base64_2).map_err(|e| {
            EnclaveAttestationError::AttestationDocumentParseError(format!(
                "Failed to decode base64 attestation document 2: {e}"
            ))
        })?;

        self.verify_dual_attestation_documents(&attestation_doc_bytes_1, &attestation_doc_bytes_2)
    }

    /// Verifies a base64-encoded attestation document and encrypts the given plaintext
    ///
    /// This is a convenience method that handles base64 decoding, verifying the attestation document,
    /// and encrypting the given plaintext using the enclave's public key using `crypto_box` sealed box.
    ///
    /// Learn about seal box [here](https://libsodium.gitbook.io/doc/public-key_cryptography/sealed_boxes)
    ///
    /// # Arguments
    /// * `attestation_doc_base64` - The base64-encoded attestation document
    /// * `plaintext` - The plaintext to encrypt
    ///
    /// # Returns
    /// A verified attestation containing the enclave's public key and the encrypted plaintext in base64 format.
    ///
    /// # Errors
    /// Returns an error if the base64 decoding fails or the attestation document verification fails
    pub fn verify_attestation_document_and_encrypt(
        &self,
        attestation_doc_base64: &str,
        plaintext: &[u8],
    ) -> EnclaveAttestationResult<VerifiedAttestationWithCiphertext> {
        let verified_attestation =
            self.verify_attestation_document_base64(attestation_doc_base64)?;

        let public_key = {
            let pk_bytes = STANDARD
                .decode(verified_attestation.enclave_public_key.clone())
                .map_err(|e| {
                    EnclaveAttestationError::InvalidEnclavePublicKey(format!(
                        "Failed to decode enclave public key: {e}"
                    ))
                })?;
            PublicKey::from_slice(&pk_bytes).map_err(|e| {
                EnclaveAttestationError::InvalidEnclavePublicKey(format!(
                    "Failed to parse enclave public key: {e}"
                ))
            })?
        };

        let ciphertext = public_key
            .seal(&mut OsRng, plaintext)
            .map_err(|_| EnclaveAttestationError::EncryptionError)?;

        Ok(VerifiedAttestationWithCiphertext {
            verified_attestation,
            ciphertext,
        })
    }
}

impl EnclaveAttestationVerifier {
    /// Verifies the attestation document from the enclave.
    ///
    /// Follows the AWS Nitro Enclave Attestation Document Specification:
    /// <https://docs.aws.amazon.com/enclaves/latest/user/nitro-enclave-attestation-document.html>
    ///
    /// Note: This method does not validate PCR values against hardcoded values.
    /// Use `verify_dual_attestation_documents` to compare PCR values between two attestations.
    fn verify_attestation_document(
        &self,
        attestation_doc_bytes: &[u8],
    ) -> EnclaveAttestationResult<VerifiedAttestation> {
        // 1. Syntactical validation
        let cose_sign1 = Self::parse_cose_sign1(attestation_doc_bytes)?;
        let attestation = Self::parse_cbor_payload(&cose_sign1)?;

        // 2. Semantic validation
        let leaf_cert = self.verify_certificate_chain(&attestation)?;

        // 3. Cryptographic validation
        Self::verify_cose_signature(&cose_sign1, &leaf_cert)?;
        // Note: We skip PCR validation for single documents
        // as we don't have hardcoded values to compare against
        self.check_attestation_freshness(&attestation)?;
        let public_key = Self::extract_public_key(&attestation)?;

        Ok(VerifiedAttestation::new(
            STANDARD.encode(public_key),
            attestation.timestamp,
            attestation.module_id,
        ))
    }

    /// Verifies two attestation documents and compares their PCR values
    ///
    /// This method performs full verification of both attestation documents and
    /// ensures their PCR values match for indices 0, 1, and 2.
    fn verify_dual_attestation_documents(
        &self,
        attestation_doc_bytes_1: &[u8],
        attestation_doc_bytes_2: &[u8],
    ) -> EnclaveAttestationResult<(VerifiedAttestation, VerifiedAttestation)> {
        // 1. Parse both attestation documents
        let cose_sign1_1 = Self::parse_cose_sign1(attestation_doc_bytes_1)?;
        let attestation_1 = Self::parse_cbor_payload(&cose_sign1_1)?;

        let cose_sign1_2 = Self::parse_cose_sign1(attestation_doc_bytes_2)?;
        let attestation_2 = Self::parse_cbor_payload(&cose_sign1_2)?;

        // 2. Verify certificate chains for both
        let leaf_cert_1 = self.verify_certificate_chain(&attestation_1)?;
        let leaf_cert_2 = self.verify_certificate_chain(&attestation_2)?;

        // 3. Verify signatures for both
        Self::verify_cose_signature(&cose_sign1_1, &leaf_cert_1)?;
        Self::verify_cose_signature(&cose_sign1_2, &leaf_cert_2)?;

        // 4. Compare PCR values between the two attestations
        self.compare_pcr_values(&attestation_1, &attestation_2)?;

        // 5. Check freshness for both
        self.check_attestation_freshness(&attestation_1)?;
        self.check_attestation_freshness(&attestation_2)?;

        // 6. Extract public keys
        let public_key_1 = Self::extract_public_key(&attestation_1)?;
        let public_key_2 = Self::extract_public_key(&attestation_2)?;

        Ok((
            VerifiedAttestation::new(
                STANDARD.encode(public_key_1),
                attestation_1.timestamp,
                attestation_1.module_id,
            ),
            VerifiedAttestation::new(
                STANDARD.encode(public_key_2),
                attestation_2.timestamp,
                attestation_2.module_id,
            ),
        ))
    }

    fn parse_cose_sign1(bytes: &[u8]) -> EnclaveAttestationResult<CoseSign1> {
        // Validate before loading into buffer
        if bytes.is_empty() {
            return Err(EnclaveAttestationError::AttestationDocumentParseError(
                "Empty attestation document".to_string(),
            ));
        }

        let first_byte = bytes[0];
        if !(0x80..=0x97).contains(&first_byte) && first_byte != 0x9f {
            return Err(EnclaveAttestationError::AttestationDocumentParseError(
                format!("Invalid CBOR magic byte: expected array marker (0x80-0x97 or 0x9f), got 0x{first_byte:02x}")
            ));
        }

        let cbor_value: ciborium::Value = ciborium::from_reader(bytes).map_err(|e| {
            EnclaveAttestationError::AttestationDocumentParseError(format!(
                "Failed to parse CBOR: {e}"
            ))
        })?;

        CoseSign1::from_cbor_value(cbor_value).map_err(|e| {
            EnclaveAttestationError::AttestationDocumentParseError(format!(
                "Failed to parse COSE Sign1: {e}"
            ))
        })
    }

    fn parse_cbor_payload(cose_sign1: &CoseSign1) -> EnclaveAttestationResult<AttestationDoc> {
        let payload = cose_sign1.payload.as_ref().ok_or_else(|| {
            EnclaveAttestationError::AttestationDocumentParseError(
                "Missing payload in COSE Sign1".to_string(),
            )
        })?;

        ciborium::from_reader::<AttestationDoc, _>(payload.as_slice()).map_err(|e| {
            EnclaveAttestationError::AttestationDocumentParseError(format!(
                "Failed to parse attestation document: {e}"
            ))
        })
    }

    fn verify_certificate_chain(
        &self,
        attestation: &AttestationDoc,
    ) -> EnclaveAttestationResult<Certificate> {
        let root_cert_der = self.root_certificate.as_slice();

        // Create trust anchor from root certificate
        let trust_anchor = TrustAnchor::try_from_cert_der(root_cert_der).map_err(|e| {
            EnclaveAttestationError::AttestationChainInvalid(format!(
                "Failed to create trust anchor from root certificate: {e}"
            ))
        })?;

        // Collect intermediate certificates from cabundle,
        let intermediate_certs: Vec<&[u8]> = attestation
            .cabundle
            .iter()
            .skip(1)
            .map(|cert| cert.as_slice())
            .collect();

        // Get current time for certificate validity checking
        let should_skip_time_check = {
            #[cfg(test)]
            {
                self.skip_certificate_time_check
            }
            #[cfg(not(test))]
            {
                false
            }
        };

        let current_time = if should_skip_time_check {
            // ONLY USED FOR TESTING
            // Use the attestation timestamp converted to seconds for certificate validation
            // This ensures we're using the same time context as when the attestation was created
            webpki::Time::from_seconds_since_unix_epoch(attestation.timestamp / 1000)
        } else {
            let now = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e| {
                EnclaveAttestationError::AttestationInvalidTimestamp(format!(
                    "Failed to get current time: {e}"
                ))
            })?;
            webpki::Time::from_seconds_since_unix_epoch(now.as_secs())
        };

        // Create end entity certificate from the leaf certificate
        let end_entity_cert =
            EndEntityCert::try_from(attestation.certificate.as_slice()).map_err(|e| {
                EnclaveAttestationError::AttestationChainInvalid(format!(
                    "Failed to parse leaf certificate: {e}"
                ))
            })?;

        // Verify the certificate chain
        end_entity_cert
            .verify_is_valid_tls_server_cert(
                &[&webpki::ECDSA_P384_SHA384],
                &webpki::TlsServerTrustAnchors(&[trust_anchor]),
                &intermediate_certs,
                current_time,
            )
            .map_err(|e| {
                EnclaveAttestationError::AttestationChainInvalid(format!(
                    "Certificate chain validation failed: {e}"
                ))
            })?;

        // Parse the leaf certificate for return
        Certificate::from_der(&attestation.certificate).map_err(|e| {
            EnclaveAttestationError::AttestationChainInvalid(format!(
                "Failed to parse leaf certificate for return: {e}"
            ))
        })
    }

    fn verify_cose_signature(
        cose_sign1: &CoseSign1,
        leaf_cert: &Certificate,
    ) -> EnclaveAttestationResult<()> {
        // Extract public key from certificate
        let spki = &leaf_cert.tbs_certificate.subject_public_key_info;
        let public_key_bytes = spki.subject_public_key.as_bytes().ok_or_else(|| {
            EnclaveAttestationError::AttestationSignatureInvalid(
                "Failed to extract public key bytes".to_string(),
            )
        })?;

        // Parse as P-384 public key
        let verifying_key = VerifyingKey::from_sec1_bytes(public_key_bytes).map_err(|e| {
            EnclaveAttestationError::AttestationSignatureInvalid(format!(
                "Failed to parse P-384 public key: {e}"
            ))
        })?;

        let signature = &cose_sign1.signature;

        // Nitro uses P-384 signatures which should be exactly 96 bytes
        if signature.len() != 96 {
            return Err(EnclaveAttestationError::AttestationSignatureInvalid(
                format!(
                    "Invalid signature length: expected 96 bytes, got {}",
                    signature.len()
                ),
            ));
        }

        // Reconstruct the signed data according to COSE Sign1 structure
        let protected_bytes = cose_sign1.protected.clone().to_vec().map_err(|e| {
            EnclaveAttestationError::AttestationSignatureInvalid(format!(
                "Failed to serialize protected headers: {e}"
            ))
        })?;

        let payload = cose_sign1.payload.as_ref().ok_or_else(|| {
            EnclaveAttestationError::AttestationSignatureInvalid(
                "Missing payload in COSE Sign1".to_string(),
            )
        })?;

        // Create the Sig_structure for COSE_Sign1
        let mut sig_structure = Vec::new();
        let sig_structure_cbor = ciborium::Value::Array(vec![
            ciborium::Value::Text("Signature1".to_string()),
            ciborium::Value::Bytes(protected_bytes),
            ciborium::Value::Bytes(vec![]),
            ciborium::Value::Bytes(payload.clone()),
        ]);

        ciborium::into_writer(&sig_structure_cbor, &mut sig_structure).map_err(|e| {
            EnclaveAttestationError::AttestationSignatureInvalid(format!(
                "Failed to encode Sig_structure: {e}"
            ))
        })?;

        // Parse and verify the signature
        let ecdsa_signature = Signature::try_from(signature.as_slice()).map_err(|e| {
            EnclaveAttestationError::AttestationSignatureInvalid(format!(
                "Failed to parse ECDSA signature (need 96 raw bytes): {e}"
            ))
        })?;

        verifying_key
            .verify(&sig_structure, &ecdsa_signature)
            .map_err(|e| {
                EnclaveAttestationError::AttestationSignatureInvalid(format!(
                    "Signature verification failed: {e}"
                ))
            })?;

        Ok(())
    }

    /// Compares PCR values between two attestation documents
    ///
    /// This method ensures that PCR values at indices 0, 1, and 2 match between
    /// the two attestation documents.
    fn compare_pcr_values(
        &self,
        attestation_1: &AttestationDoc,
        attestation_2: &AttestationDoc,
    ) -> EnclaveAttestationResult<()> {
        // Ensure both attestations have PCRs
        if attestation_1.pcrs.is_empty() {
            return Err(EnclaveAttestationError::CodeUntrusted {
                pcr_index: 0,
                actual: "empty PCRs in first attestation".to_string(),
            });
        }

        if attestation_2.pcrs.is_empty() {
            return Err(EnclaveAttestationError::CodeUntrusted {
                pcr_index: 0,
                actual: "empty PCRs in second attestation".to_string(),
            });
        }

        // Get the expected PCR length depending on the hashing algorithm used
        let expected_pcr_length_1 = get_expected_pcr_length(attestation_1.digest);
        let expected_pcr_length_2 = get_expected_pcr_length(attestation_2.digest);

        // Ensure both use the same digest algorithm
        if attestation_1.digest != attestation_2.digest {
            return Err(EnclaveAttestationError::CodeUntrusted {
                pcr_index: 0,
                actual: format!(
                    "Digest mismatch: {:?} vs {:?}",
                    attestation_1.digest, attestation_2.digest
                ),
            });
        }

        // Check PCR values for indices 0, 1, and 2
        for pcr_index in [0, 1, 2] {
            let index = pcr_index as usize;

            // Get PCR value from first attestation
            let pcr_value_1 = attestation_1.pcrs.get(&index).ok_or_else(|| {
                EnclaveAttestationError::CodeUntrusted {
                    pcr_index,
                    actual: format!("missing PCR{} in first attestation", pcr_index),
                }
            })?;

            // Get PCR value from second attestation
            let pcr_value_2 = attestation_2.pcrs.get(&index).ok_or_else(|| {
                EnclaveAttestationError::CodeUntrusted {
                    pcr_index,
                    actual: format!("missing PCR{} in second attestation", pcr_index),
                }
            })?;

            // Validate PCR value lengths
            if pcr_value_1.len() != expected_pcr_length_1 {
                return Err(EnclaveAttestationError::CodeUntrusted {
                    pcr_index,
                    actual: format!(
                        "Invalid PCR{} length in first attestation: {}, expected: {}",
                        pcr_index,
                        pcr_value_1.len(),
                        expected_pcr_length_1
                    ),
                });
            }

            if pcr_value_2.len() != expected_pcr_length_2 {
                return Err(EnclaveAttestationError::CodeUntrusted {
                    pcr_index,
                    actual: format!(
                        "Invalid PCR{} length in second attestation: {}, expected: {}",
                        pcr_index,
                        pcr_value_2.len(),
                        expected_pcr_length_2
                    ),
                });
            }

            // Compare PCR values
            if pcr_value_1.as_slice() != pcr_value_2.as_slice() {
                return Err(EnclaveAttestationError::CodeUntrusted {
                    pcr_index,
                    actual: format!(
                        "PCR{} mismatch: {} vs {}",
                        pcr_index,
                        hex::encode(pcr_value_1),
                        hex::encode(pcr_value_2)
                    ),
                });
            }
        }

        Ok(())
    }

    fn check_attestation_freshness(
        &self,
        attestation: &AttestationDoc,
    ) -> EnclaveAttestationResult<()> {
        let now = u64::try_from(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|e| {
                    EnclaveAttestationError::AttestationInvalidTimestamp(format!(
                        "Failed to get current time: {e}"
                    ))
                })?
                .as_millis(),
        )
        .map_err(|e| {
            EnclaveAttestationError::AttestationInvalidTimestamp(format!(
                "Failed to convert current time to milliseconds: {e}"
            ))
        })?;

        let age = now.checked_sub(attestation.timestamp).ok_or_else(|| {
            EnclaveAttestationError::AttestationInvalidTimestamp(format!(
                "Attestation timestamp is {} ms in the future",
                attestation.timestamp - now
            ))
        })?;

        if age > self.max_age_millis {
            return Err(EnclaveAttestationError::AttestationStale {
                age_millis: age,
                max_age: self.max_age_millis,
            });
        }

        Ok(())
    }

    fn extract_public_key(attestation: &AttestationDoc) -> EnclaveAttestationResult<Vec<u8>> {
        attestation.public_key.clone().map_or_else(
            || {
                Err(EnclaveAttestationError::InvalidEnclavePublicKey(
                    "No public key in attestation document".to_string(),
                ))
            },
            |key| Ok(key.into_vec()),
        )
    }
}
