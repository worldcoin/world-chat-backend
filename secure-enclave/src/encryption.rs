use std::{fs, path::Path};

use crypto_box::{aead::OsRng, PublicKey, SecretKey};

use chacha20poly1305::{
    aead::{Aead, AeadCore, Error, KeyInit, Payload},
    Key, XChaCha20Poly1305, XNonce,
};

/// An asymmetric key pair (X25519), used for end-to-end encrypted communications.
pub struct KeyPair {
    pub public_key: PublicKey,
    pub private_key: SecretKey,
}

impl KeyPair {
    /// Generates a new key pair using the OS RNG.
    pub fn generate() -> Self {
        // Safe: at startup we verify the kernel RNG is backed by `nsm-hwrng`.
        let private_key = SecretKey::generate(&mut OsRng);
        let public_key = private_key.public_key();

        Self {
            public_key,
            private_key,
        }
    }
}
/// Non-secret; just needs to be unique per message.
pub const NONCE_LEN: usize = 24; // XChaCha20-poly1305
pub const TAG_LEN: usize = 16; // Poly1305 tag size

/// Minimal wrapper around a XChaCha20-Poly1305 key.
pub struct XChaCha20Poly1305Box {
    key: Key,
}

impl XChaCha20Poly1305Box {
    /// Construct from an existing 32-byte key.
    pub fn new(key_bytes: [u8; 32]) -> Self {
        Self {
            key: Key::from_slice(&key_bytes).to_owned(),
        }
    }

    /// TODO: In the future enclaves will request a key from a backup-enclave.  
    /// Generate a fresh random 32-byte key and return a ready-to-use instance.
    pub fn generate() -> Self {
        let key = XChaCha20Poly1305::generate_key(&mut OsRng);
        Self { key }
    }

    /// Optionally expose the key (e.g., to persist in secure storage).
    /// Return a copy to avoid lifetime pitfalls.
    pub fn key_bytes(&self) -> [u8; 32] {
        let mut out = [0u8; 32];
        out.copy_from_slice(&self.key);
        out
    }

    /// Encrypt and pack as: [24-byte nonce][ciphertext||16-byte tag]
    pub fn encrypt_pack(&self, aad: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, Error> {
        let cipher = XChaCha20Poly1305::new(&self.key);

        // Fresh random nonce per message (XChaCha supports random nonces safely).
        let nonce = XChaCha20Poly1305::generate_nonce(&mut OsRng);

        // AEAD with optional associated data (AAD).
        let ct = cipher.encrypt(
            &nonce,
            Payload {
                msg: plaintext,
                aad,
            },
        )?;

        // Concatenate nonce || ct
        let mut out = Vec::with_capacity(NONCE_LEN + ct.len());
        out.extend_from_slice(nonce.as_slice());
        out.extend_from_slice(&ct);
        Ok(out)
    }

    /// Unpack and decrypt from: [24-byte nonce][ciphertext||16-byte tag]
    pub fn decrypt_unpack(&self, aad: &[u8], blob: &[u8]) -> Result<Vec<u8>, Error> {
        if blob.len() < NONCE_LEN + TAG_LEN {
            // Not enough bytes to contain nonce + tag, fail early.
            return Err(Error);
        }
        let (nonce_bytes, ct) = blob.split_at(NONCE_LEN);
        let nonce = XNonce::from_slice(nonce_bytes);

        let cipher = XChaCha20Poly1305::new(&self.key);
        cipher.decrypt(nonce, Payload { msg: ct, aad })
    }
}

/// Verify that the kernel's HW RNG source in use is `nsm-hwrng`.
/// This ensures the AWS Nitro RNG was registered and is periodically feeding entropy.
/// See Randomness Section in:
/// `<https://blog.trailofbits.com/2024/09/24/notes-on-aws-nitro-enclaves-attack-surface>`
pub fn verify_nsm_hwrng_current() -> anyhow::Result<()> {
    const SYSFS_PATHS: [&str; 2] = [
        "/sys/class/misc/hw_random/rng_current",
        "/sys/devices/virtual/misc/hw_random/rng_current",
    ];

    for path in SYSFS_PATHS {
        if Path::new(path).exists() {
            let contents = fs::read_to_string(path)?;
            let current = contents.trim();
            tracing::info!("rng_current={current}");

            return if current == "nsm-hwrng" {
                Ok(())
            } else {
                Err(anyhow::anyhow!(
                    "rng_current is '{current}', expected 'nsm-hwrng'"
                ))
            };
        }
    }

    Err(anyhow::anyhow!("rng_current sysfs path not found"))
}
