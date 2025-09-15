use std::{fs, path::Path};

use crypto_box::{aead::OsRng, PublicKey, SecretKey};

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
