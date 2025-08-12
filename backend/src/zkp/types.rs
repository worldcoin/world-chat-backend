use std::ops::Deref;

use ruint::aliases::U256;
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

use super::error::ZkpError;
use crate::types::Environment;

/// A wrapper around `U256` to represent a field element in the protocol.
/// Provides clean serialization as 0x-prefixed hex strings and type safety.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct U256Wrapper(pub U256);

impl U256Wrapper {
    /// Outputs a hex string representation of the `U256` value padded to 32 bytes (plus 0x prefix).
    #[must_use]
    pub fn to_hex_string(&self) -> String {
        format!("{:#066x}", self.0)
    }

    /// Attempts to parse a hex string as a `U256` value (wrapped).
    ///
    /// # Errors
    /// Will return an error if the input is not a valid hex-string-presented number up to 256 bits.
    pub fn try_from_hex_string(hex_string: &str) -> Result<Self, ZkpError> {
        let hex_string = hex_string.trim().trim_start_matches("0x");

        let number = U256::from_str_radix(hex_string, 16)
            .map_err(|e| ZkpError::InvalidProofData(format!("Invalid hex number: {e}")))?;

        Ok(Self(number))
    }

    /// Creates a `U256` value from a `u64` value.
    #[must_use]
    pub fn from_u64(value: u64) -> Self {
        Self(U256::from(value))
    }

    /// Creates a `U256` value from a `u32` value.
    #[must_use]
    pub fn from_u32(value: u32) -> Self {
        Self(U256::from(value))
    }
}

impl From<U256Wrapper> for U256 {
    fn from(val: U256Wrapper) -> Self {
        val.0
    }
}

impl From<U256> for U256Wrapper {
    fn from(val: U256) -> Self {
        Self(val)
    }
}

impl std::fmt::Display for U256Wrapper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_hex_string())
    }
}

impl Deref for U256Wrapper {
    type Target = U256;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Serialize for U256Wrapper {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_hex_string())
    }
}

impl<'de> Deserialize<'de> for U256Wrapper {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::try_from_hex_string(&s).map_err(serde::de::Error::custom)
    }
}

/// Represents different verification levels for World ID credentials.
/// Each level corresponds to a different type of identity verification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumString, Hash, Display)]
#[strum(serialize_all = "snake_case")]
pub enum VerificationLevel {
    /// Orb-verified identity - highest level of verification
    Orb,
    /// Government-issued document verification
    #[strum(serialize = "document")]
    Document,
    /// Government-issued document with additional security checks
    SecureDocument,
    /// Device-based verification
    Device,
}

impl Serialize for VerificationLevel {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl VerificationLevel {
    /// Returns the host name for the relevant sign up sequencer to use. The sign up sequencer is used to fetch Merkle inclusion proofs.
    ///
    /// [Reference](https://github.com/worldcoin/signup-sequencer)
    ///
    /// # Future
    /// - Support custom sign up sequencer hosts
    #[must_use]
    pub const fn get_sign_up_sequencer_host(&self, environment: &Environment) -> &str {
        match environment {
            Environment::Staging | Environment::Development { .. } => match self {
                Self::Orb => "https://signup-orb-ethereum.stage-crypto.worldcoin.org",
                Self::Device => "https://signup-phone-ethereum.stage-crypto.worldcoin.org",
                Self::Document => "https://signup-document.stage-crypto.worldcoin.org",
                Self::SecureDocument => "https://signup-document-secure.stage-crypto.worldcoin.org",
            },
            Environment::Production => match self {
                Self::Orb => "https://signup-orb-ethereum.crypto.worldcoin.org",
                Self::Device => "https://signup-phone-ethereum.crypto.worldcoin.org",
                Self::Document => "https://signup-document.crypto.worldcoin.org",
                Self::SecureDocument => "https://signup-document-secure.crypto.worldcoin.org",
            },
        }
    }

    /// Returns the v2 verification endpoint for the sequencer
    #[must_use]
    pub fn get_verification_endpoint(&self, environment: &Environment) -> String {
        format!(
            "{}/v2/semaphore-proof/verify",
            self.get_sign_up_sequencer_host(environment)
        )
    }
}

/// Response structure from v2 Semaphore proof verification
#[derive(Debug, Deserialize)]
pub struct VerificationResponse {
    pub valid: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use ruint::uint;

    #[test]
    fn test_u256_to_hex_string() {
        assert_eq!(
            U256Wrapper(U256::from(1)).to_hex_string(),
            "0x0000000000000000000000000000000000000000000000000000000000000001"
        );
        assert_eq!(
            U256Wrapper(U256::from(42)).to_hex_string(),
            "0x000000000000000000000000000000000000000000000000000000000000002a"
        );
        assert_eq!(
            U256Wrapper(uint!(999999_U256)).to_hex_string(),
            "0x00000000000000000000000000000000000000000000000000000000000f423f"
        );
    }

    #[test]
    fn test_u256_from_hex_string() {
        assert_eq!(
            U256Wrapper::try_from_hex_string(
                "0x0000000000000000000000000000000000000000000000000000000000000001"
            )
            .unwrap(),
            U256Wrapper(U256::from(1))
        );
        assert_eq!(
            U256Wrapper::try_from_hex_string(
                "0x000000000000000000000000000000000000000000000000000000000000002a"
            )
            .unwrap(),
            U256Wrapper(U256::from(42))
        );
        assert_eq!(
            U256Wrapper::try_from_hex_string(
                "0x00000000000000000000000000000000000000000000000000000000000f423f"
            )
            .unwrap(),
            U256Wrapper(uint!(999999_U256))
        );
    }

    #[test]
    fn test_invalid_hex_string() {
        assert!(U256Wrapper::try_from_hex_string("0xZZZZ").is_err());
        assert!(U256Wrapper::try_from_hex_string("not a hex string").is_err());
    }

    #[test]
    fn test_json_serialization() {
        let number = U256Wrapper(uint!(
            0x036b6384b5eca791c62761152d0c79bb0604c104a5fb6f4eb0703f3154bb3db0_U256
        ));

        let json = serde_json::to_string(&number).unwrap();
        assert_eq!(
            json,
            "\"0x036b6384b5eca791c62761152d0c79bb0604c104a5fb6f4eb0703f3154bb3db0\""
        );

        let deserialized: U256Wrapper = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, number);
    }

    #[test]
    fn test_verification_level_serialization() {
        let level = VerificationLevel::Device;
        let serialized = serde_json::to_string(&level).unwrap();
        assert_eq!(serialized, "\"device\"");

        let level = VerificationLevel::SecureDocument;
        let serialized = serde_json::to_string(&level).unwrap();
        assert_eq!(serialized, "\"secure_document\"");
    }
}
