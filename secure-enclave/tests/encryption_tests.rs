use secure_enclave::encryption::XChaCha20Poly1305Box;

#[cfg(test)]
mod xchacha20poly1305_tests {
    use super::*;

    #[test]
    fn test_basic_round_trip() {
        // Test that encryption followed by decryption returns the original data
        let cipher = XChaCha20Poly1305Box::generate();
        let plaintext = b"Hello, World!";
        let aad = b"";

        let encrypted = cipher
            .encrypt_pack(aad, plaintext)
            .expect("Encryption should succeed");

        let decrypted = cipher
            .decrypt_unpack(aad, &encrypted)
            .expect("Decryption should succeed");

        assert_eq!(
            plaintext.to_vec(),
            decrypted,
            "Round-trip should preserve data"
        );
    }

    #[test]
    fn test_aad_authentication() {
        // Verify that Associated Authenticated Data (AAD) is properly authenticated
        let cipher = XChaCha20Poly1305Box::generate();
        let plaintext = b"Secret message";
        let aad = b"metadata";
        let wrong_aad = b"wrong_metadata";

        let encrypted = cipher
            .encrypt_pack(aad, plaintext)
            .expect("Encryption with AAD should succeed");

        // Decrypt with correct AAD should succeed
        let decrypted = cipher
            .decrypt_unpack(aad, &encrypted)
            .expect("Decryption with correct AAD should succeed");
        assert_eq!(plaintext.to_vec(), decrypted);

        // Decrypt with wrong AAD should fail
        let result = cipher.decrypt_unpack(wrong_aad, &encrypted);
        assert!(
            result.is_err(),
            "Decryption with wrong AAD should fail authentication"
        );
    }

    #[test]
    fn test_tampering_detection() {
        // Ensure any modification to the ciphertext is detected
        let cipher = XChaCha20Poly1305Box::generate();
        let plaintext = b"Tamper-proof message";
        let aad = b"";

        let mut encrypted = cipher
            .encrypt_pack(aad, plaintext)
            .expect("Encryption should succeed");

        // Ensure we have enough bytes to tamper with (nonce + ciphertext + tag)
        assert!(encrypted.len() > 40);

        // Flip a bit in the ciphertext portion (after the 24-byte nonce)
        encrypted[30] ^= 0x01;

        let result = cipher.decrypt_unpack(aad, &encrypted);
        assert!(
            result.is_err(),
            "Decryption of tampered ciphertext should fail"
        );
    }

    #[test]
    fn test_malformed_input() {
        // Test error handling for invalid inputs
        let cipher = XChaCha20Poly1305Box::generate();
        let aad = b"";

        // Test with blob shorter than minimum (24-byte nonce + 16-byte tag = 40 bytes)
        let too_short = vec![0u8; 39];
        let result = cipher.decrypt_unpack(aad, &too_short);
        assert!(
            result.is_err(),
            "Decryption of too-short blob should fail gracefully"
        );

        // Test with empty blob
        let empty = vec![];
        let result = cipher.decrypt_unpack(aad, &empty);
        assert!(result.is_err(), "Decryption of empty blob should fail");
    }

    #[test]
    fn test_key_isolation() {
        // Verify that different keys produce incompatible ciphertexts
        let cipher1 = XChaCha20Poly1305Box::generate();
        let cipher2 = XChaCha20Poly1305Box::generate();
        let plaintext = b"Key-specific message";
        let aad = b"";

        // Encrypt with first key
        let encrypted = cipher1
            .encrypt_pack(aad, plaintext)
            .expect("Encryption with key1 should succeed");

        // Try to decrypt with second key
        let result = cipher2.decrypt_unpack(aad, &encrypted);
        assert!(result.is_err(), "Decryption with different key should fail");

        // Verify the first key can still decrypt it
        let decrypted = cipher1
            .decrypt_unpack(aad, &encrypted)
            .expect("Decryption with original key should succeed");
        assert_eq!(plaintext.to_vec(), decrypted);
    }
}
